import { EventEmitter } from 'events';
import { Logger } from '../utils/logger';
import { DChannelHandler, DChannelConfig, DChannelMode, NFASGroup, Q931Message } from './d-channel';

export interface NFASConfiguration {
  enabled: boolean;
  groups: NFASGroupConfig[];
  switchoverTimeout: number;
  heartbeatInterval: number;
  maxSwitchoverAttempts: number;
}

export interface NFASGroupConfig {
  groupId: number;
  primarySpan: number;
  backupSpans: number[];
  loadBalancing: boolean;
  ces: number;  // Connection Endpoint Suffix for the group
}

export interface NFASStatistics {
  groupId: number;
  activeSpan: number;
  primarySpan: number;
  availableSpans: number[];
  switchoverCount: number;
  lastSwitchover?: Date;
  heartbeatsSent: number;
  heartbeatsLost: number;
  callsHandled: number;
}

export class NFASManager extends EventEmitter {
  private config: NFASConfiguration;
  private logger: Logger;
  private groups: Map<number, NFASGroup> = new Map();
  private dChannelHandlers: Map<string, DChannelHandler> = new Map();
  private isRunning = false;
  private heartbeatInterval?: NodeJS.Timeout;
  private statistics: Map<number, NFASStatistics> = new Map();

  constructor(config: NFASConfiguration, logger: Logger) {
    super();
    this.config = config;
    this.logger = logger.child({ component: 'nfas-manager' });
  }

  async start(): Promise<void> {
    if (this.isRunning) {
      throw new Error('NFAS Manager already running');
    }

    this.logger.info('Starting NFAS Manager', {
      groupCount: this.config.groups.length
    });

    // Initialize NFAS groups
    for (const groupConfig of this.config.groups) {
      await this.initializeGroup(groupConfig);
    }

    // Start heartbeat monitoring
    this.startHeartbeatMonitoring();

    this.isRunning = true;
    this.emit('started');
  }

  async stop(): Promise<void> {
    if (!this.isRunning) {
      return;
    }

    this.logger.info('Stopping NFAS Manager');

    // Stop heartbeat monitoring
    if (this.heartbeatInterval) {
      clearInterval(this.heartbeatInterval);
      this.heartbeatInterval = undefined;
    }

    // Stop all D-Channel handlers
    for (const handler of this.dChannelHandlers.values()) {
      await handler.stop();
    }

    // Clear all groups
    this.groups.clear();
    this.dChannelHandlers.clear();

    this.isRunning = false;
    this.emit('stopped');
  }

  private async initializeGroup(groupConfig: NFASGroupConfig): Promise<void> {
    this.logger.debug('Initializing NFAS group', { groupId: groupConfig.groupId });

    const group: NFASGroup = {
      groupId: groupConfig.groupId,
      primarySpan: groupConfig.primarySpan,
      backupSpans: groupConfig.backupSpans,
      activeSpan: groupConfig.primarySpan,
      spans: new Map(),
      state: 'inactive'
    };

    // Create D-Channel handlers for all spans in the group
    const allSpans = [groupConfig.primarySpan, ...groupConfig.backupSpans];
    
    for (const spanId of allSpans) {
      const isPrimary = spanId === groupConfig.primarySpan;
      
      const dChannelConfig: DChannelConfig = {
        mode: DChannelMode.NFAS,
        spanId,
        channelId: 16, // Typically D-channel is on timeslot 16 for E1
        tei: 0, // TEI 0 for NFAS primary, automatic assignment for backups
        sapi: 0, // Call control SAPI
        ces: groupConfig.ces,
        primaryInterface: isPrimary,
        backupInterface: !isPrimary,
        interfaceGroup: groupConfig.groupId,
        maxRetransmissions: 3,
        t200Timer: 1000,
        t201Timer: 1000,
        t202Timer: 2000,
        t203Timer: 10000,
        n200Counter: 3,
        n201Counter: 260
      };

      const handler = new DChannelHandler(dChannelConfig, this.logger);
      this.setupDChannelEventHandlers(handler, group);
      
      group.spans.set(spanId, handler);
      this.dChannelHandlers.set(`${groupConfig.groupId}-${spanId}`, handler);
    }

    // Initialize group statistics
    this.statistics.set(groupConfig.groupId, {
      groupId: groupConfig.groupId,
      activeSpan: groupConfig.primarySpan,
      primarySpan: groupConfig.primarySpan,
      availableSpans: allSpans,
      switchoverCount: 0,
      heartbeatsSent: 0,
      heartbeatsLost: 0,
      callsHandled: 0
    });

    this.groups.set(groupConfig.groupId, group);

    // Start the primary interface
    const primaryHandler = group.spans.get(groupConfig.primarySpan);
    if (primaryHandler) {
      try {
        await primaryHandler.start();
        group.state = 'active';
        this.logger.info('NFAS group activated', {
          groupId: groupConfig.groupId,
          activeSpan: groupConfig.primarySpan
        });
      } catch (error) {
        this.logger.error('Failed to start primary interface', {
          groupId: groupConfig.groupId,
          spanId: groupConfig.primarySpan,
          error
        });
        
        // Try backup interfaces
        await this.performSwitchover(groupConfig.groupId, 'primary_failed');
      }
    }
  }

  private setupDChannelEventHandlers(handler: DChannelHandler, group: NFASGroup): void {
    handler.on('established', () => {
      this.handleDChannelEstablished(handler, group);
    });

    handler.on('disconnected', () => {
      this.handleDChannelDisconnected(handler, group);
    });

    handler.on('error', (error) => {
      this.handleDChannelError(handler, group, error);
    });

    handler.on('q931Message', (message: Q931Message) => {
      this.handleQ931Message(handler, group, message);
    });

    handler.on('frameOut', (frameData: Buffer) => {
      this.emit('frameOut', handler.getConfig().spanId, frameData);
    });
  }

  private handleDChannelEstablished(handler: DChannelHandler, group: NFASGroup): void {
    const config = handler.getConfig();
    
    this.logger.info('D-Channel established', {
      groupId: group.groupId,
      spanId: config.spanId,
      isPrimary: config.primaryInterface
    });

    if (config.primaryInterface && group.state !== 'active') {
      group.state = 'active';
      group.activeSpan = config.spanId;
      
      this.emit('groupActivated', {
        groupId: group.groupId,
        activeSpan: config.spanId
      });
    }
  }

  private handleDChannelDisconnected(handler: DChannelHandler, group: NFASGroup): void {
    const config = handler.getConfig();
    
    this.logger.warn('D-Channel disconnected', {
      groupId: group.groupId,
      spanId: config.spanId,
      isPrimary: config.primaryInterface
    });

    // If the active interface disconnected, perform switchover
    if (config.spanId === group.activeSpan) {
      this.performSwitchover(group.groupId, 'interface_down');
    }
  }

  private handleDChannelError(handler: DChannelHandler, group: NFASGroup, error: Error): void {
    const config = handler.getConfig();
    
    this.logger.error('D-Channel error', {
      groupId: group.groupId,
      spanId: config.spanId,
      error: error.message
    });

    // If the active interface has an error, perform switchover
    if (config.spanId === group.activeSpan) {
      this.performSwitchover(group.groupId, 'interface_error');
    }
  }

  private handleQ931Message(handler: DChannelHandler, group: NFASGroup, message: Q931Message): void {
    const config = handler.getConfig();
    
    // Only process messages from the active interface
    if (config.spanId !== group.activeSpan) {
      this.logger.debug('Ignoring Q.931 message from inactive interface', {
        groupId: group.groupId,
        spanId: config.spanId,
        activeSpan: group.activeSpan
      });
      return;
    }

    // Update statistics
    const stats = this.statistics.get(group.groupId);
    if (stats) {
      stats.callsHandled++;
    }

    this.emit('q931Message', {
      groupId: group.groupId,
      spanId: config.spanId,
      message
    });
  }

  private async performSwitchover(groupId: number, reason: string): Promise<void> {
    const group = this.groups.get(groupId);
    if (!group) {
      this.logger.error('Group not found for switchover', { groupId });
      return;
    }

    this.logger.info('Performing NFAS switchover', {
      groupId,
      currentActive: group.activeSpan,
      reason
    });

    group.state = 'switching';
    
    // Find next available backup interface
    const availableSpans = [group.primarySpan, ...group.backupSpans].filter(spanId => {
      const handler = group.spans.get(spanId);
      return handler && handler.getState() !== 'down' && spanId !== group.activeSpan;
    });

    if (availableSpans.length === 0) {
      this.logger.error('No available backup interfaces for switchover', { groupId });
      group.state = 'inactive';
      
      this.emit('groupInactive', {
        groupId,
        reason: 'no_backup_available'
      });
      return;
    }

    // Try each available span
    for (const spanId of availableSpans) {
      const handler = group.spans.get(spanId);
      if (!handler) continue;

      try {
        this.logger.debug('Attempting switchover to span', { groupId, spanId });
        
        // Stop current active interface if it's still running
        const currentHandler = group.spans.get(group.activeSpan);
        if (currentHandler) {
          await currentHandler.stop();
        }

        // Start new interface
        if (!handler.isEstablished()) {
          await handler.start();
        }

        // Switchover successful
        group.activeSpan = spanId;
        group.state = 'active';

        // Update statistics
        const stats = this.statistics.get(groupId);
        if (stats) {
          stats.switchoverCount++;
          stats.lastSwitchover = new Date();
          stats.activeSpan = spanId;
        }

        this.logger.info('NFAS switchover completed', {
          groupId,
          newActiveSpan: spanId,
          reason
        });

        this.emit('switchoverCompleted', {
          groupId,
          previousSpan: group.activeSpan,
          newActiveSpan: spanId,
          reason
        });

        return;

      } catch (error) {
        this.logger.warn('Failed to switchover to span', {
          groupId,
          spanId,
          error: error instanceof Error ? error.message : error
        });
        continue;
      }
    }

    // All switchover attempts failed
    this.logger.error('All switchover attempts failed', { groupId });
    group.state = 'inactive';
    
    this.emit('groupInactive', {
      groupId,
      reason: 'switchover_failed'
    });
  }

  private startHeartbeatMonitoring(): void {
    if (this.config.heartbeatInterval <= 0) {
      return;
    }

    this.heartbeatInterval = setInterval(() => {
      this.sendHeartbeats();
    }, this.config.heartbeatInterval);
  }

  private sendHeartbeats(): void {
    for (const [groupId, group] of this.groups) {
      if (group.state !== 'active') {
        continue;
      }

      const activeHandler = group.spans.get(group.activeSpan);
      if (!activeHandler || !activeHandler.isEstablished()) {
        continue;
      }

      try {
        // Send Q.931 STATUS ENQUIRY message as heartbeat
        const statusEnquiry: Q931Message = {
          protocolDiscriminator: 0x08,
          callReference: {
            length: 1,
            flag: false,
            value: 0
          },
          messageType: 0x75, // STATUS ENQUIRY
          informationElements: []
        };

        activeHandler.sendQ931Message(statusEnquiry);
        
        // Update statistics
        const stats = this.statistics.get(groupId);
        if (stats) {
          stats.heartbeatsSent++;
        }

      } catch (error) {
        this.logger.warn('Failed to send heartbeat', {
          groupId,
          spanId: group.activeSpan,
          error: error instanceof Error ? error.message : error
        });

        // Update statistics
        const stats = this.statistics.get(groupId);
        if (stats) {
          stats.heartbeatsLost++;
        }
      }
    }
  }

  // Public API methods
  async sendQ931Message(groupId: number, message: Q931Message): Promise<void> {
    const group = this.groups.get(groupId);
    if (!group) {
      throw new Error(`NFAS group ${groupId} not found`);
    }

    if (group.state !== 'active') {
      throw new Error(`NFAS group ${groupId} is not active`);
    }

    const activeHandler = group.spans.get(group.activeSpan);
    if (!activeHandler || !activeHandler.isEstablished()) {
      throw new Error(`Active interface for group ${groupId} is not established`);
    }

    await activeHandler.sendQ931Message(message);
  }

  processReceivedFrame(spanId: number, frameData: Buffer): void {
    // Find the group and handler for this span
    for (const [groupId, group] of this.groups) {
      const handler = group.spans.get(spanId);
      if (handler) {
        handler.processReceivedFrame(frameData);
        return;
      }
    }

    this.logger.warn('Received frame for unknown span', { spanId });
  }

  async forceSwitchover(groupId: number, targetSpanId?: number): Promise<void> {
    const group = this.groups.get(groupId);
    if (!group) {
      throw new Error(`NFAS group ${groupId} not found`);
    }

    if (targetSpanId && !group.spans.has(targetSpanId)) {
      throw new Error(`Span ${targetSpanId} not found in group ${groupId}`);
    }

    this.logger.info('Manual switchover initiated', {
      groupId,
      targetSpanId,
      currentActive: group.activeSpan
    });

    if (targetSpanId) {
      // Switch to specific span
      const currentActive = group.activeSpan;
      group.activeSpan = targetSpanId;
      
      try {
        await this.performSwitchover(groupId, 'manual');
      } catch (error) {
        // Restore previous active span if switchover fails
        group.activeSpan = currentActive;
        throw error;
      }
    } else {
      // Switch to next available span
      await this.performSwitchover(groupId, 'manual');
    }
  }

  getGroupStatus(groupId: number): any {
    const group = this.groups.get(groupId);
    const stats = this.statistics.get(groupId);
    
    if (!group || !stats) {
      return null;
    }

    const spanStatuses = new Map();
    for (const [spanId, handler] of group.spans) {
      spanStatuses.set(spanId, {
        state: handler.getState(),
        statistics: handler.getStatistics(),
        isEstablished: handler.isEstablished()
      });
    }

    return {
      groupId,
      state: group.state,
      activeSpan: group.activeSpan,
      primarySpan: group.primarySpan,
      backupSpans: group.backupSpans,
      spans: Object.fromEntries(spanStatuses),
      statistics: stats
    };
  }

  getAllGroupStatuses(): any[] {
    const statuses = [];
    for (const groupId of this.groups.keys()) {
      statuses.push(this.getGroupStatus(groupId));
    }
    return statuses;
  }

  getDChannelHandler(groupId: number, spanId: number): DChannelHandler | undefined {
    const group = this.groups.get(groupId);
    return group?.spans.get(spanId);
  }

  getActiveHandler(groupId: number): DChannelHandler | undefined {
    const group = this.groups.get(groupId);
    if (!group) return undefined;
    return group.spans.get(group.activeSpan);
  }

  isGroupActive(groupId: number): boolean {
    const group = this.groups.get(groupId);
    return group?.state === 'active';
  }

  getConfiguration(): NFASConfiguration {
    return { ...this.config };
  }

  getStatistics(): Map<number, NFASStatistics> {
    return new Map(this.statistics);
  }
}