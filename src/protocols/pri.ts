import { EventEmitter } from 'events';
import { PRIChannel } from '../types';

export interface PRIMessage {
  messageType: 'setup' | 'call_proceeding' | 'alerting' | 'connect' | 'disconnect' | 'release' | 'release_complete';
  callReference: number;
  callingNumber?: string;
  calledNumber?: string;
  bearerCapability?: string;
  cause?: number;
}

export class PRIEmulator extends EventEmitter {
  private channels: Map<number, PRIChannel> = new Map();
  private variant: 'ni2' | 'euro' | 'japan';
  private switchType: string;
  private isRunning = false;
  private callReferences: Map<number, number> = new Map();
  private nextCallRef = 1;

  constructor(variant: 'ni2' | 'euro' | 'japan' = 'ni2', switchType = 'dms100') {
    super();
    this.variant = variant;
    this.switchType = switchType;
    
    for (let i = 1; i <= 23; i++) {
      this.channels.set(i, {
        id: i,
        state: 'idle',
        direction: 'inbound'
      });
    }
  }

  async start(): Promise<void> {
    if (this.isRunning) {
      throw new Error('PRI emulator already running');
    }

    this.isRunning = true;
    this.emit('started');
  }

  async stop(): Promise<void> {
    if (!this.isRunning) {
      return;
    }

    this.isRunning = false;
    
    for (const channel of this.channels.values()) {
      if (channel.state !== 'idle') {
        channel.state = 'idle';
      }
    }
    
    this.callReferences.clear();
    this.emit('stopped');
  }

  sendSetup(callingNumber: string, calledNumber: string, bearerCapability = 'speech'): number | null {
    const channel = this.findAvailableChannel();
    if (!channel) {
      return null;
    }

    const callRef = this.allocateCallReference();
    this.callReferences.set(callRef, channel.id);
    
    channel.state = 'busy';
    channel.direction = 'outbound';

    const message: PRIMessage = {
      messageType: 'setup',
      callReference: callRef,
      callingNumber,
      calledNumber,
      bearerCapability
    };

    this.emit('messageOut', message, channel.id);
    return channel.id;
  }

  sendCallProceeding(callRef: number): void {
    const channelId = this.callReferences.get(callRef);
    if (!channelId) {
      throw new Error(`Invalid call reference: ${callRef}`);
    }

    const channel = this.channels.get(channelId);
    if (channel) {
      channel.state = 'busy';
    }

    const message: PRIMessage = {
      messageType: 'call_proceeding',
      callReference: callRef
    };

    this.emit('messageOut', message, channelId);
  }

  sendAlerting(callRef: number): void {
    const channelId = this.callReferences.get(callRef);
    if (!channelId) {
      throw new Error(`Invalid call reference: ${callRef}`);
    }

    const channel = this.channels.get(channelId);
    if (channel) {
      channel.state = 'ringing';
    }

    const message: PRIMessage = {
      messageType: 'alerting',
      callReference: callRef
    };

    this.emit('messageOut', message, channelId);
  }

  sendConnect(callRef: number): void {
    const channelId = this.callReferences.get(callRef);
    if (!channelId) {
      throw new Error(`Invalid call reference: ${callRef}`);
    }

    const channel = this.channels.get(channelId);
    if (channel) {
      channel.state = 'connected';
    }

    const message: PRIMessage = {
      messageType: 'connect',
      callReference: callRef
    };

    this.emit('messageOut', message, channelId);
  }

  sendDisconnect(callRef: number, cause = 16): void {
    const channelId = this.callReferences.get(callRef);
    if (!channelId) {
      throw new Error(`Invalid call reference: ${callRef}`);
    }

    const message: PRIMessage = {
      messageType: 'disconnect',
      callReference: callRef,
      cause
    };

    this.emit('messageOut', message, channelId);
  }

  sendRelease(callRef: number, cause = 16): void {
    const channelId = this.callReferences.get(callRef);
    if (!channelId) {
      throw new Error(`Invalid call reference: ${callRef}`);
    }

    const channel = this.channels.get(channelId);
    if (channel) {
      channel.state = 'idle';
    }

    const message: PRIMessage = {
      messageType: 'release',
      callReference: callRef,
      cause
    };

    this.callReferences.delete(callRef);
    this.emit('messageOut', message, channelId);
  }

  sendReleaseComplete(callRef: number, cause = 16): void {
    const channelId = this.callReferences.get(callRef);
    
    const message: PRIMessage = {
      messageType: 'release_complete',
      callReference: callRef,
      cause
    };

    if (channelId) {
      const channel = this.channels.get(channelId);
      if (channel) {
        channel.state = 'idle';
      }
      this.callReferences.delete(callRef);
      this.emit('messageOut', message, channelId);
    }
  }

  handleIncomingMessage(message: PRIMessage, channelId: number): void {
    const channel = this.channels.get(channelId);
    if (!channel) {
      throw new Error(`Invalid channel: ${channelId}`);
    }

    switch (message.messageType) {
      case 'setup':
        this.handleSetup(message, channel);
        break;
      case 'call_proceeding':
        this.handleCallProceeding(message, channel);
        break;
      case 'alerting':
        this.handleAlerting(message, channel);
        break;
      case 'connect':
        this.handleConnect(message, channel);
        break;
      case 'disconnect':
        this.handleDisconnect(message, channel);
        break;
      case 'release':
        this.handleRelease(message, channel);
        break;
      case 'release_complete':
        this.handleReleaseComplete(message, channel);
        break;
    }
  }

  private handleSetup(message: PRIMessage, channel: PRIChannel): void {
    channel.state = 'busy';
    channel.direction = 'inbound';
    this.callReferences.set(message.callReference, channel.id);
    this.emit('incomingCall', message, channel.id);
  }

  private handleCallProceeding(message: PRIMessage, channel: PRIChannel): void {
    channel.state = 'busy';
    this.emit('callProceeding', message, channel.id);
  }

  private handleAlerting(message: PRIMessage, channel: PRIChannel): void {
    channel.state = 'ringing';
    this.emit('alerting', message, channel.id);
  }

  private handleConnect(message: PRIMessage, channel: PRIChannel): void {
    channel.state = 'connected';
    this.emit('connected', message, channel.id);
  }

  private handleDisconnect(message: PRIMessage, channel: PRIChannel): void {
    this.emit('disconnect', message, channel.id);
  }

  private handleRelease(message: PRIMessage, channel: PRIChannel): void {
    channel.state = 'idle';
    this.callReferences.delete(message.callReference);
    this.emit('release', message, channel.id);
  }

  private handleReleaseComplete(message: PRIMessage, channel: PRIChannel): void {
    channel.state = 'idle';
    this.callReferences.delete(message.callReference);
    this.emit('releaseComplete', message, channel.id);
  }

  private findAvailableChannel(): PRIChannel | null {
    for (const channel of this.channels.values()) {
      if (channel.state === 'idle') {
        return channel;
      }
    }
    return null;
  }

  private allocateCallReference(): number {
    return this.nextCallRef++;
  }

  getChannelStatus(channelId: number): PRIChannel | undefined {
    return this.channels.get(channelId);
  }

  getAllChannels(): PRIChannel[] {
    return Array.from(this.channels.values());
  }

  getVariant(): string {
    return this.variant;
  }

  getSwitchType(): string {
    return this.switchType;
  }
}