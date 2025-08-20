import { EventEmitter } from 'events';
import { Logger } from '../utils/logger';
import { FreeTDMConfig, FreeTDMSpan, FreeTDMChannel } from '../types';

export interface FreeTDMEvent {
  type: 'call_incoming' | 'call_outgoing' | 'call_answered' | 'call_hangup' | 'dtmf' | 'alarm';
  spanId: number;
  channelId: number;
  data?: any;
  timestamp: Date;
}

export interface FreeTDMCallInfo {
  uniqueId: string;
  spanId: number;
  channelId: number;
  direction: 'inbound' | 'outbound';
  callerNumber?: string;
  calledNumber?: string;
  state: 'idle' | 'dialing' | 'ringing' | 'answered' | 'busy' | 'hangup';
  startTime: Date;
  answerTime?: Date;
  endTime?: Date;
}

export interface FreeTDMSpanStatus {
  spanId: number;
  name: string;
  type: string;
  state: 'up' | 'down' | 'alarm';
  totalChannels: number;
  activeChannels: number;
  availableChannels: number;
  alarms: string[];
  signaling: string;
}

export class FreeTDMInterface extends EventEmitter {
  private config: FreeTDMConfig;
  private logger: Logger;
  private isRunning = false;
  private spans: Map<number, FreeTDMSpan> = new Map();
  private activeCalls: Map<string, FreeTDMCallInfo> = new Map();
  private spanStatus: Map<number, FreeTDMSpanStatus> = new Map();
  private nativeBinding: any; // Native C++ binding to FreeTDM

  constructor(config: FreeTDMConfig, logger: Logger) {
    super();
    this.config = config;
    this.logger = logger;
    this.initializeSpans();
  }

  private initializeSpans(): void {
    for (const span of this.config.spans) {
      this.spans.set(span.spanId, span);
      this.spanStatus.set(span.spanId, {
        spanId: span.spanId,
        name: span.name,
        type: span.trunk_type,
        state: 'down',
        totalChannels: span.channels.length,
        activeChannels: 0,
        availableChannels: span.channels.filter(ch => ch.enabled).length,
        alarms: [],
        signaling: span.channels.find(ch => ch.signaling)?.signaling || 'none'
      });
    }
  }

  async start(): Promise<void> {
    if (this.isRunning) {
      throw new Error('FreeTDM interface already running');
    }

    if (!this.config.enabled) {
      this.logger.info('FreeTDM interface disabled in configuration');
      return;
    }

    try {
      // Load FreeTDM native binding
      this.loadNativeBinding();

      // Initialize FreeTDM library
      await this.initializeFreeTDM();

      // Configure spans
      await this.configureSpans();

      // Start event monitoring
      this.startEventMonitoring();

      this.isRunning = true;
      this.logger.info('FreeTDM interface started successfully');
      this.emit('started');
    } catch (error) {
      this.logger.error('Failed to start FreeTDM interface', error);
      throw error;
    }
  }

  async stop(): Promise<void> {
    if (!this.isRunning) {
      return;
    }

    try {
      // Stop all active calls
      this.hangupAllCalls();

      // Stop spans
      await this.stopSpans();

      // Shutdown FreeTDM
      await this.shutdownFreeTDM();

      this.isRunning = false;
      this.logger.info('FreeTDM interface stopped');
      this.emit('stopped');
    } catch (error) {
      this.logger.error('Error stopping FreeTDM interface', error);
      throw error;
    }
  }

  private loadNativeBinding(): void {
    try {
      // In a real implementation, this would load a native C++ module
      // that interfaces with the FreeTDM library
      // this.nativeBinding = require('./native/freetdm_binding');
      
      // For now, we'll simulate the binding
      this.nativeBinding = {
        init: () => Promise.resolve(),
        configure: () => Promise.resolve(),
        shutdown: () => Promise.resolve(),
        makeCall: () => Promise.resolve(),
        hangupCall: () => Promise.resolve(),
        sendDTMF: () => Promise.resolve(),
        getSpanStatus: () => ({}),
        getChannelStatus: () => ({}),
        on: () => {}
      };
      
      this.logger.info('FreeTDM native binding loaded');
    } catch (error) {
      throw new Error(`Failed to load FreeTDM native binding: ${error}`);
    }
  }

  private async initializeFreeTDM(): Promise<void> {
    return new Promise((resolve, reject) => {
      try {
        this.nativeBinding.init({
          configFile: this.config.configFile,
          logLevel: 'info'
        });

        // Set up event callbacks
        this.nativeBinding.on('call_incoming', (event: any) => {
          this.handleIncomingCall(event);
        });

        this.nativeBinding.on('call_answered', (event: any) => {
          this.handleCallAnswered(event);
        });

        this.nativeBinding.on('call_hangup', (event: any) => {
          this.handleCallHangup(event);
        });

        this.nativeBinding.on('dtmf', (event: any) => {
          this.handleDTMF(event);
        });

        this.nativeBinding.on('alarm', (event: any) => {
          this.handleAlarm(event);
        });

        resolve();
      } catch (error) {
        reject(error);
      }
    });
  }

  private async configureSpans(): Promise<void> {
    for (const [spanId, span] of this.spans) {
      try {
        await this.configureSpan(span);
        this.updateSpanStatus(spanId, 'up');
        this.logger.info(`Span ${spanId} (${span.name}) configured and started`);
      } catch (error) {
        this.updateSpanStatus(spanId, 'alarm');
        this.logger.error(`Failed to configure span ${spanId}`, error);
      }
    }
  }

  private async configureSpan(span: FreeTDMSpan): Promise<void> {
    const spanConfig = {
      spanId: span.spanId,
      name: span.name,
      trunkType: span.trunk_type,
      dChannel: span.d_channel,
      channels: span.channels.map(ch => ({
        id: ch.id,
        type: ch.type,
        enabled: ch.enabled,
        signaling: ch.signaling
      }))
    };

    return new Promise((resolve, reject) => {
      try {
        this.nativeBinding.configure(spanConfig);
        resolve();
      } catch (error) {
        reject(error);
      }
    });
  }

  private startEventMonitoring(): void {
    // Start periodic status monitoring
    setInterval(() => {
      this.updateSpanStatuses();
    }, 5000); // Update every 5 seconds
  }

  private updateSpanStatuses(): void {
    for (const spanId of this.spans.keys()) {
      try {
        const status = this.nativeBinding.getSpanStatus(spanId);
        if (status) {
          this.updateSpanStatusFromNative(spanId, status);
        }
      } catch (error) {
        this.logger.warn(`Failed to get status for span ${spanId}`, error);
      }
    }
  }

  private updateSpanStatusFromNative(spanId: number, nativeStatus: any): void {
    const currentStatus = this.spanStatus.get(spanId);
    if (!currentStatus) return;

    const updated = {
      ...currentStatus,
      state: nativeStatus.state || currentStatus.state,
      activeChannels: nativeStatus.activeChannels || 0,
      availableChannels: nativeStatus.availableChannels || currentStatus.totalChannels,
      alarms: nativeStatus.alarms || []
    };

    this.spanStatus.set(spanId, updated);
    this.emit('spanStatusUpdate', updated);
  }

  // Call management methods
  async makeCall(spanId: number, channelId: number, destination: string): Promise<string> {
    if (!this.isRunning) {
      throw new Error('FreeTDM interface not running');
    }

    const span = this.spans.get(spanId);
    if (!span) {
      throw new Error(`Span ${spanId} not found`);
    }

    const channel = span.channels.find(ch => ch.id === channelId);
    if (!channel || !channel.enabled) {
      throw new Error(`Channel ${channelId} not available on span ${spanId}`);
    }

    const uniqueId = this.generateUniqueCallId();
    const callInfo: FreeTDMCallInfo = {
      uniqueId,
      spanId,
      channelId,
      direction: 'outbound',
      calledNumber: destination,
      state: 'dialing',
      startTime: new Date()
    };

    this.activeCalls.set(uniqueId, callInfo);

    try {
      await this.nativeBinding.makeCall({
        spanId,
        channelId,
        destination,
        uniqueId
      });

      this.emit('callInitiated', callInfo);
      return uniqueId;
    } catch (error) {
      this.activeCalls.delete(uniqueId);
      throw error;
    }
  }

  async hangupCall(uniqueId: string): Promise<void> {
    const callInfo = this.activeCalls.get(uniqueId);
    if (!callInfo) {
      throw new Error(`Call ${uniqueId} not found`);
    }

    try {
      await this.nativeBinding.hangupCall({
        uniqueId,
        spanId: callInfo.spanId,
        channelId: callInfo.channelId
      });

      callInfo.state = 'hangup';
      callInfo.endTime = new Date();
      this.emit('callHangup', callInfo);
    } catch (error) {
      this.logger.error(`Failed to hangup call ${uniqueId}`, error);
      throw error;
    }
  }

  async sendDTMF(uniqueId: string, digits: string): Promise<void> {
    const callInfo = this.activeCalls.get(uniqueId);
    if (!callInfo) {
      throw new Error(`Call ${uniqueId} not found`);
    }

    try {
      await this.nativeBinding.sendDTMF({
        uniqueId,
        spanId: callInfo.spanId,
        channelId: callInfo.channelId,
        digits
      });

      this.emit('dtmfSent', { uniqueId, digits });
    } catch (error) {
      this.logger.error(`Failed to send DTMF for call ${uniqueId}`, error);
      throw error;
    }
  }

  // Event handlers
  private handleIncomingCall(event: any): void {
    const uniqueId = this.generateUniqueCallId();
    const callInfo: FreeTDMCallInfo = {
      uniqueId,
      spanId: event.spanId,
      channelId: event.channelId,
      direction: 'inbound',
      callerNumber: event.callerNumber,
      calledNumber: event.calledNumber,
      state: 'ringing',
      startTime: new Date()
    };

    this.activeCalls.set(uniqueId, callInfo);
    this.emit('incomingCall', callInfo);
  }

  private handleCallAnswered(event: any): void {
    const callInfo = this.findCallBySpanChannel(event.spanId, event.channelId);
    if (callInfo) {
      callInfo.state = 'answered';
      callInfo.answerTime = new Date();
      this.emit('callAnswered', callInfo);
    }
  }

  private handleCallHangup(event: any): void {
    const callInfo = this.findCallBySpanChannel(event.spanId, event.channelId);
    if (callInfo) {
      callInfo.state = 'hangup';
      callInfo.endTime = new Date();
      this.activeCalls.delete(callInfo.uniqueId);
      this.emit('callHangup', callInfo);
    }
  }

  private handleDTMF(event: any): void {
    const callInfo = this.findCallBySpanChannel(event.spanId, event.channelId);
    if (callInfo) {
      this.emit('dtmfReceived', {
        uniqueId: callInfo.uniqueId,
        digit: event.digit
      });
    }
  }

  private handleAlarm(event: any): void {
    const spanStatus = this.spanStatus.get(event.spanId);
    if (spanStatus) {
      spanStatus.alarms.push(event.alarmType);
      spanStatus.state = 'alarm';
      this.emit('alarm', {
        spanId: event.spanId,
        alarmType: event.alarmType,
        severity: event.severity,
        message: event.message
      });
    }
  }

  // Utility methods
  private findCallBySpanChannel(spanId: number, channelId: number): FreeTDMCallInfo | undefined {
    for (const callInfo of this.activeCalls.values()) {
      if (callInfo.spanId === spanId && callInfo.channelId === channelId) {
        return callInfo;
      }
    }
    return undefined;
  }

  private generateUniqueCallId(): string {
    return `FTDM-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }

  private hangupAllCalls(): void {
    const callIds = Array.from(this.activeCalls.keys());
    for (const callId of callIds) {
      this.hangupCall(callId).catch(error => {
        this.logger.warn(`Failed to hangup call ${callId} during shutdown`, error);
      });
    }
  }

  private async stopSpans(): Promise<void> {
    for (const spanId of this.spans.keys()) {
      this.updateSpanStatus(spanId, 'down');
    }
  }

  private async shutdownFreeTDM(): Promise<void> {
    return new Promise((resolve) => {
      try {
        this.nativeBinding.shutdown();
        resolve();
      } catch (error) {
        this.logger.warn('Error during FreeTDM shutdown', error);
        resolve();
      }
    });
  }

  private updateSpanStatus(spanId: number, state: 'up' | 'down' | 'alarm'): void {
    const status = this.spanStatus.get(spanId);
    if (status) {
      status.state = state;
      this.emit('spanStatusUpdate', status);
    }
  }

  // Public API methods
  getSpanStatus(spanId: number): FreeTDMSpanStatus | undefined {
    return this.spanStatus.get(spanId);
  }

  getAllSpanStatuses(): FreeTDMSpanStatus[] {
    return Array.from(this.spanStatus.values());
  }

  getActiveCall(uniqueId: string): FreeTDMCallInfo | undefined {
    return this.activeCalls.get(uniqueId);
  }

  getAllActiveCalls(): FreeTDMCallInfo[] {
    return Array.from(this.activeCalls.values());
  }

  getCallsForSpan(spanId: number): FreeTDMCallInfo[] {
    return Array.from(this.activeCalls.values()).filter(call => call.spanId === spanId);
  }

  isRunning(): boolean {
    return this.isRunning;
  }

  getConfiguration(): FreeTDMConfig {
    return { ...this.config };
  }
}