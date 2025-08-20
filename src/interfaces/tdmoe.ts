import { EventEmitter } from 'events';
import { TDMoEFrame } from '../types';

export class TDMoEInterface extends EventEmitter {
  private channels: Map<number, boolean> = new Map();
  private isRunning = false;
  private interfaceName: string;
  private channelCount: number;

  constructor(interfaceName: string, channelCount: number = 24) {
    super();
    this.interfaceName = interfaceName;
    this.channelCount = channelCount;
    
    for (let i = 1; i <= channelCount; i++) {
      this.channels.set(i, false);
    }
  }

  async start(): Promise<void> {
    if (this.isRunning) {
      throw new Error('TDMoE interface already running');
    }

    this.isRunning = true;
    this.emit('started');
  }

  async stop(): Promise<void> {
    if (!this.isRunning) {
      return;
    }

    this.isRunning = false;
    this.emit('stopped');
  }

  allocateChannel(): number | null {
    for (const [channel, inUse] of this.channels) {
      if (!inUse) {
        this.channels.set(channel, true);
        return channel;
      }
    }
    return null;
  }

  releaseChannel(channel: number): void {
    if (this.channels.has(channel)) {
      this.channels.set(channel, false);
      this.emit('channelReleased', channel);
    }
  }

  sendFrame(frame: TDMoEFrame): void {
    if (!this.isRunning) {
      throw new Error('TDMoE interface not running');
    }

    if (!this.channels.get(frame.channel)) {
      throw new Error(`Channel ${frame.channel} not allocated`);
    }

    this.emit('frameOut', frame);
  }

  sendRemoteLoopCommand(channel: number, command: 'activate' | 'deactivate', loopType: 'line' | 'payload' | 'network'): void {
    if (!this.isRunning) {
      throw new Error('TDMoE interface not running');
    }

    if (!this.channels.get(channel)) {
      throw new Error(`Channel ${channel} not allocated`);
    }

    // Construct remote loop command frame
    const commandCode = this.getLoopCommandCode(command, loopType);
    const commandFrame = this.buildRemoteLoopFrame(channel, commandCode);

    this.emit('remoteLoopCommand', {
      channel,
      command,
      loopType,
      frame: commandFrame
    });

    this.emit('frameOut', {
      channel,
      data: commandFrame,
      timestamp: Date.now()
    });
  }

  private getLoopCommandCode(command: 'activate' | 'deactivate', loopType: string): number {
    // ITU-T G.703/G.704 remote loop commands
    const codes: { [key: string]: number } = {
      'activate_line': 0x01,      // Activate line loopback
      'deactivate_line': 0x02,    // Deactivate line loopback
      'activate_payload': 0x03,   // Activate payload loopback
      'deactivate_payload': 0x04, // Deactivate payload loopback
      'activate_network': 0x05,   // Activate network loopback
      'deactivate_network': 0x06  // Deactivate network loopback
    };

    const key = `${command}_${loopType}`;
    return codes[key] || 0x00;
  }

  private buildRemoteLoopFrame(channel: number, commandCode: number): Buffer {
    // Build ITU-T G.704 compliant remote loop command frame
    const frame = Buffer.alloc(32); // Standard frame size
    
    // Frame header
    frame[0] = 0x7E; // Flag
    frame[1] = 0x7E; // Flag
    
    // Channel identification
    frame[2] = (channel >> 8) & 0xFF;
    frame[3] = channel & 0xFF;
    
    // Command type
    frame[4] = 0x10; // Remote loop command type
    
    // Command code
    frame[5] = commandCode;
    
    // Fill remainder with pattern
    for (let i = 6; i < 30; i++) {
      frame[i] = 0x55; // Alternating pattern
    }
    
    // CRC (simplified)
    const crc = this.calculateCRC(frame.slice(0, 30));
    frame[30] = (crc >> 8) & 0xFF;
    frame[31] = crc & 0xFF;
    
    return frame;
  }

  private calculateCRC(data: Buffer): number {
    // Simplified CRC-16 calculation
    let crc = 0xFFFF;
    
    for (let i = 0; i < data.length; i++) {
      crc ^= data[i] << 8;
      for (let j = 0; j < 8; j++) {
        if (crc & 0x8000) {
          crc = (crc << 1) ^ 0x1021;
        } else {
          crc <<= 1;
        }
        crc &= 0xFFFF;
      }
    }
    
    return crc;
  }

  handleRemoteLoopResponse(channel: number, data: Buffer): void {
    // Parse remote loop response
    if (data.length < 6) {
      this.emit('remoteLoopError', channel, 'Invalid response frame');
      return;
    }

    const responseType = data[4];
    const responseCode = data[5];

    if (responseType === 0x11) { // Remote loop response
      const success = (responseCode & 0x80) === 0; // MSB indicates success/failure
      const loopType = this.parseLoopType(responseCode & 0x7F);
      
      this.emit('remoteLoopResponse', {
        channel,
        success,
        loopType,
        responseCode
      });
    }
  }

  private parseLoopType(code: number): string {
    const types: { [key: number]: string } = {
      0x01: 'line',
      0x02: 'line',
      0x03: 'payload',
      0x04: 'payload',
      0x05: 'network',
      0x06: 'network'
    };
    return types[code] || 'unknown';
  }

  private simulateIncomingFrame(channel: number, data: Buffer): void {
    const frame: TDMoEFrame = {
      channel,
      data,
      timestamp: Date.now()
    };
    
    this.emit('frameIn', frame);
  }

  getChannelStatus(): Map<number, boolean> {
    return new Map(this.channels);
  }

  isChannelAllocated(channel: number): boolean {
    return this.channels.get(channel) || false;
  }
}