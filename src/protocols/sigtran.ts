import { EventEmitter } from 'events';
import { ISUPMessage } from '../types';

export interface SigtranConfig {
  localPointCode: number;
  remotePointCode: number;
  networkIndicator: number;
  serviceIndicator: number;
  variant: 'itu' | 'ansi' | 'etsi' | 'china' | 'japan';
  applicationServer: string;
  routingKey: number;
  trafficMode: 'override' | 'loadshare' | 'broadcast';
}

export interface ISUPCall {
  cic: number;
  state: 'idle' | 'outgoing_setup' | 'incoming_setup' | 'answered' | 'releasing';
  callingNumber?: string;
  calledNumber?: string;
  startTime?: Date;
}

export class SigtranISUPHandler extends EventEmitter {
  private config: SigtranConfig;
  private calls: Map<number, ISUPCall> = new Map();
  private isRunning = false;
  private availableCICs: Set<number> = new Set();

  constructor(config: SigtranConfig) {
    super();
    this.config = config;
    
    for (let cic = 1; cic <= 1000; cic++) {
      this.availableCICs.add(cic);
    }
  }

  async start(): Promise<void> {
    if (this.isRunning) {
      throw new Error('Sigtran ISUP handler already running');
    }

    this.isRunning = true;
    this.emit('started');
  }

  async stop(): Promise<void> {
    if (!this.isRunning) {
      return;
    }

    this.isRunning = false;
    this.calls.clear();
    this.emit('stopped');
  }

  sendIAM(callingNumber: string, calledNumber: string): number | null {
    const cic = this.allocateCIC();
    if (!cic) {
      return null;
    }

    const call: ISUPCall = {
      cic,
      state: 'outgoing_setup',
      callingNumber,
      calledNumber,
      startTime: new Date()
    };

    this.calls.set(cic, call);

    const message: ISUPMessage = {
      messageType: 0x01, // IAM
      cic,
      parameters: this.buildIAMParameters(callingNumber, calledNumber)
    };

    this.emit('messageOut', message);
    return cic;
  }

  sendACM(cic: number): void {
    const call = this.calls.get(cic);
    if (!call) {
      throw new Error(`Call not found for CIC: ${cic}`);
    }

    const message: ISUPMessage = {
      messageType: 0x06, // ACM
      cic,
      parameters: Buffer.alloc(0)
    };

    this.emit('messageOut', message);
  }

  sendANM(cic: number): void {
    const call = this.calls.get(cic);
    if (!call) {
      throw new Error(`Call not found for CIC: ${cic}`);
    }

    call.state = 'answered';

    const message: ISUPMessage = {
      messageType: 0x09, // ANM
      cic,
      parameters: Buffer.alloc(0)
    };

    this.emit('messageOut', message);
  }

  sendREL(cic: number, cause = 16): void {
    const call = this.calls.get(cic);
    if (!call) {
      return;
    }

    call.state = 'releasing';

    const message: ISUPMessage = {
      messageType: 0x0C, // REL
      cic,
      parameters: this.buildCauseParameter(cause)
    };

    this.emit('messageOut', message);
  }

  sendRLC(cic: number): void {
    const call = this.calls.get(cic);
    if (call) {
      this.calls.delete(cic);
      this.availableCICs.add(cic);
    }

    const message: ISUPMessage = {
      messageType: 0x10, // RLC
      cic,
      parameters: Buffer.alloc(0)
    };

    this.emit('messageOut', message);
  }

  handleIncomingMessage(message: ISUPMessage): void {
    switch (message.messageType) {
      case 0x01: // IAM
        this.handleIAM(message);
        break;
      case 0x06: // ACM
        this.handleACM(message);
        break;
      case 0x09: // ANM
        this.handleANM(message);
        break;
      case 0x0C: // REL
        this.handleREL(message);
        break;
      case 0x10: // RLC
        this.handleRLC(message);
        break;
      default:
        this.emit('unknownMessage', message);
    }
  }

  private handleIAM(message: ISUPMessage): void {
    const { callingNumber, calledNumber } = this.parseIAMParameters(message.parameters);
    
    const call: ISUPCall = {
      cic: message.cic,
      state: 'incoming_setup',
      callingNumber,
      calledNumber,
      startTime: new Date()
    };

    this.calls.set(message.cic, call);
    this.availableCICs.delete(message.cic);
    
    this.emit('incomingCall', call);
  }

  private handleACM(message: ISUPMessage): void {
    const call = this.calls.get(message.cic);
    if (call) {
      this.emit('callProgress', call);
    }
  }

  private handleANM(message: ISUPMessage): void {
    const call = this.calls.get(message.cic);
    if (call) {
      call.state = 'answered';
      this.emit('callAnswered', call);
    }
  }

  private handleREL(message: ISUPMessage): void {
    const call = this.calls.get(message.cic);
    if (call) {
      call.state = 'releasing';
      const cause = this.parseCauseParameter(message.parameters);
      this.emit('callReleased', call, cause);
      
      this.sendRLC(message.cic);
    }
  }

  private handleRLC(message: ISUPMessage): void {
    const call = this.calls.get(message.cic);
    if (call) {
      this.calls.delete(message.cic);
      this.availableCICs.add(message.cic);
      this.emit('callCleared', call);
    }
  }

  private allocateCIC(): number | null {
    const availableArray = Array.from(this.availableCICs);
    if (availableArray.length === 0) {
      return null;
    }

    const cic = availableArray[0];
    this.availableCICs.delete(cic);
    return cic;
  }

  private buildIAMParameters(callingNumber: string, calledNumber: string): Buffer {
    const params = Buffer.alloc(64);
    let offset = 0;

    offset += this.writeCalledPartyNumber(params, offset, calledNumber);
    offset += this.writeCallingPartyNumber(params, offset, callingNumber);
    offset += this.writeNatureOfConnectionIndicators(params, offset);
    
    return params.slice(0, offset);
  }

  private writeCalledPartyNumber(buffer: Buffer, offset: number, number: string): number {
    buffer.writeUInt8(0x04, offset++); // Parameter tag
    const length = Math.ceil(number.length / 2) + 1;
    buffer.writeUInt8(length, offset++); // Length
    buffer.writeUInt8(0x83, offset++); // Nature of address + numbering plan
    
    for (let i = 0; i < number.length; i += 2) {
      const digit1 = parseInt(number[i], 10);
      const digit2 = i + 1 < number.length ? parseInt(number[i + 1], 10) : 0;
      buffer.writeUInt8((digit2 << 4) | digit1, offset++);
    }
    
    return offset - (offset - length - 2);
  }

  private writeCallingPartyNumber(buffer: Buffer, offset: number, number: string): number {
    buffer.writeUInt8(0x0A, offset++); // Parameter tag
    const length = Math.ceil(number.length / 2) + 2;
    buffer.writeUInt8(length, offset++); // Length
    buffer.writeUInt8(0x83, offset++); // Nature of address + numbering plan
    buffer.writeUInt8(0x00, offset++); // Screening and presentation
    
    for (let i = 0; i < number.length; i += 2) {
      const digit1 = parseInt(number[i], 10);
      const digit2 = i + 1 < number.length ? parseInt(number[i + 1], 10) : 0;
      buffer.writeUInt8((digit2 << 4) | digit1, offset++);
    }
    
    return offset - (offset - length - 2);
  }

  private writeNatureOfConnectionIndicators(buffer: Buffer, offset: number): number {
    buffer.writeUInt8(0x06, offset++); // Parameter tag
    buffer.writeUInt8(0x01, offset++); // Length
    buffer.writeUInt8(0x00, offset++); // Satellite indicator + continuity check + echo control
    return 3;
  }

  private buildCauseParameter(cause: number): Buffer {
    const buffer = Buffer.alloc(4);
    buffer.writeUInt8(0x12, 0); // Cause parameter tag
    buffer.writeUInt8(0x02, 1); // Length
    buffer.writeUInt8(0x80, 2); // Location + coding standard
    buffer.writeUInt8(0x80 | cause, 3); // Cause value
    return buffer;
  }

  private parseIAMParameters(parameters: Buffer): { callingNumber?: string; calledNumber?: string } {
    let offset = 0;
    let calledNumber: string | undefined;
    let callingNumber: string | undefined;

    while (offset < parameters.length) {
      const tag = parameters.readUInt8(offset++);
      const length = parameters.readUInt8(offset++);
      
      if (tag === 0x04) { // Called party number
        calledNumber = this.parseDigits(parameters.slice(offset + 1, offset + length));
      } else if (tag === 0x0A) { // Calling party number
        callingNumber = this.parseDigits(parameters.slice(offset + 2, offset + length));
      }
      
      offset += length;
    }

    return { callingNumber, calledNumber };
  }

  private parseDigits(buffer: Buffer): string {
    let digits = '';
    for (let i = 0; i < buffer.length; i++) {
      const byte = buffer.readUInt8(i);
      const digit1 = byte & 0x0F;
      const digit2 = (byte >> 4) & 0x0F;
      
      digits += digit1.toString();
      if (digit2 !== 0) {
        digits += digit2.toString();
      }
    }
    return digits;
  }

  private parseCauseParameter(parameters: Buffer): number {
    let offset = 0;
    while (offset < parameters.length) {
      const tag = parameters.readUInt8(offset++);
      const length = parameters.readUInt8(offset++);
      
      if (tag === 0x12) { // Cause parameter
        return parameters.readUInt8(offset + 1) & 0x7F;
      }
      
      offset += length;
    }
    return 16; // Default cause: Normal call clearing
  }

  getCall(cic: number): ISUPCall | undefined {
    return this.calls.get(cic);
  }

  getAllCalls(): ISUPCall[] {
    return Array.from(this.calls.values());
  }

  getAvailableCICs(): number[] {
    return Array.from(this.availableCICs);
  }

  getConfig(): SigtranConfig {
    return { ...this.config };
  }
}