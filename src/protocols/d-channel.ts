import { EventEmitter } from 'events';
import { Logger } from '../utils/logger';

export enum DChannelMode {
  FAS = 'fas',     // Facility Associated Signaling
  NFAS = 'nfas'    // Non-Facility Associated Signaling
}

export enum DChannelState {
  DOWN = 'down',
  ESTABLISHING = 'establishing',
  ESTABLISHED = 'established',
  MULTIPLE_FRAME_ESTABLISHED = 'multiple_frame_established',
  TEI_ASSIGNED = 'tei_assigned',
  AWAITING_ESTABLISHMENT = 'awaiting_establishment',
  AWAITING_RELEASE = 'awaiting_release'
}

export interface DChannelConfig {
  mode: DChannelMode;
  spanId: number;
  channelId: number;
  tei: number;          // Terminal Endpoint Identifier
  sapi: number;         // Service Access Point Identifier
  ces: number;          // Connection Endpoint Suffix (for NFAS)
  primaryInterface?: boolean;  // Primary interface in NFAS group
  backupInterface?: boolean;   // Backup interface in NFAS group
  interfaceGroup?: number;     // NFAS interface group ID
  maxRetransmissions: number;
  t200Timer: number;    // Acknowledgment timer
  t201Timer: number;    // TEI identity check timer
  t202Timer: number;    // TEI identity request timer
  t203Timer: number;    // Maximum time without frame exchange
  n200Counter: number;  // Maximum number of retransmissions
  n201Counter: number;  // Maximum number of octets in I field
}

export interface LAPDFrame {
  address: {
    sapi: number;
    cr: boolean;        // Command/Response bit
    ea0: boolean;       // Extended Address bit 0
    tei: number;
    ea1: boolean;       // Extended Address bit 1
  };
  control: {
    type: 'I' | 'S' | 'U';  // Information, Supervisory, Unnumbered
    ns?: number;        // N(S) - Send sequence number
    nr?: number;        // N(R) - Receive sequence number
    pf?: boolean;       // P/F bit
    supervisory?: 'RR' | 'RNR' | 'REJ';  // Supervisory frame types
    unnumbered?: 'SABME' | 'DM' | 'UI' | 'DISC' | 'UA' | 'FRMR' | 'XID';
  };
  information?: Buffer;
  fcs: number;          // Frame Check Sequence
}

export interface Q931Message {
  protocolDiscriminator: number;
  callReference: {
    length: number;
    flag: boolean;      // 0=message from originating side, 1=message to originating side
    value: number;
  };
  messageType: number;
  informationElements: Q931InformationElement[];
}

export interface Q931InformationElement {
  id: number;
  codingStandard?: number;
  informationTransferCapability?: number;
  transferMode?: number;
  transferRate?: number;
  data: Buffer;
}

export interface NFASGroup {
  groupId: number;
  primarySpan: number;
  backupSpans: number[];
  activeSpan: number;
  spans: Map<number, DChannelHandler>;
  state: 'inactive' | 'active' | 'switching';
}

export class DChannelHandler extends EventEmitter {
  private config: DChannelConfig;
  private logger: Logger;
  private state: DChannelState = DChannelState.DOWN;
  private isRunning = false;
  
  // LAPD state variables
  private vs = 0;       // Send state variable
  private vr = 0;       // Receive state variable
  private va = 0;       // Acknowledge state variable
  private retransmissionCount = 0;
  private ownBusy = false;
  private peerBusy = false;
  
  // Timers
  private t200Timer?: NodeJS.Timeout;
  private t201Timer?: NodeJS.Timeout;
  private t202Timer?: NodeJS.Timeout;
  private t203Timer?: NodeJS.Timeout;
  
  // Frame queues
  private transmitQueue: LAPDFrame[] = [];
  private acknowledgePending: LAPDFrame[] = [];
  
  constructor(config: DChannelConfig, logger: Logger) {
    super();
    this.config = config;
    this.logger = logger.child({ 
      component: 'dchan',
      span: config.spanId,
      channel: config.channelId,
      mode: config.mode
    });
  }

  async start(): Promise<void> {
    if (this.isRunning) {
      throw new Error('D-Channel already running');
    }

    this.isRunning = true;
    this.state = DChannelState.ESTABLISHING;
    
    this.logger.info('Starting D-Channel', {
      mode: this.config.mode,
      tei: this.config.tei,
      sapi: this.config.sapi
    });

    // Start Layer 2 establishment
    if (this.config.mode === DChannelMode.FAS) {
      await this.establishFAS();
    } else {
      await this.establishNFAS();
    }

    this.emit('started');
  }

  async stop(): Promise<void> {
    if (!this.isRunning) {
      return;
    }

    this.clearAllTimers();
    this.state = DChannelState.DOWN;
    this.isRunning = false;
    
    this.logger.info('D-Channel stopped');
    this.emit('stopped');
  }

  private async establishFAS(): Promise<void> {
    this.logger.debug('Establishing FAS D-Channel');
    
    // Send SABME (Set Asynchronous Balanced Mode Extended)
    const sabmeFrame = this.createUnnumberedFrame('SABME', true);
    await this.sendFrame(sabmeFrame);
    
    // Start T200 timer for acknowledgment
    this.startT200Timer();
    
    this.state = DChannelState.AWAITING_ESTABLISHMENT;
  }

  private async establishNFAS(): Promise<void> {
    this.logger.debug('Establishing NFAS D-Channel');
    
    if (this.config.primaryInterface) {
      // Primary interface establishment
      await this.establishAsPrimary();
    } else {
      // Backup interface - wait for primary to establish
      this.state = DChannelState.AWAITING_ESTABLISHMENT;
      this.startT203Timer(); // Supervise primary interface
    }
  }

  private async establishAsPrimary(): Promise<void> {
    this.logger.debug('Establishing as NFAS primary interface');
    
    // Send SABME with NFAS CES identification
    const sabmeFrame = this.createUnnumberedFrame('SABME', true);
    await this.sendFrame(sabmeFrame);
    
    this.startT200Timer();
    this.state = DChannelState.AWAITING_ESTABLISHMENT;
  }

  processReceivedFrame(frameData: Buffer): void {
    if (!this.isRunning) {
      return;
    }

    try {
      const frame = this.parseLAPDFrame(frameData);
      this.logger.trace('Received LAPD frame', {
        sapi: frame.address.sapi,
        tei: frame.address.tei,
        type: frame.control.type
      });

      // Validate frame addressing
      if (!this.validateFrameAddress(frame)) {
        this.logger.warn('Invalid frame address', frame.address);
        return;
      }

      switch (frame.control.type) {
        case 'I':
          this.processInformationFrame(frame);
          break;
        case 'S':
          this.processSupervisoryFrame(frame);
          break;
        case 'U':
          this.processUnnumberedFrame(frame);
          break;
        default:
          this.logger.warn('Unknown frame type', frame.control);
      }

      this.restartT203Timer();
    } catch (error) {
      this.logger.error('Error processing received frame', error);
    }
  }

  private processInformationFrame(frame: LAPDFrame): void {
    if (this.state !== DChannelState.ESTABLISHED && 
        this.state !== DChannelState.MULTIPLE_FRAME_ESTABLISHED) {
      this.logger.warn('Received I-frame in invalid state', { state: this.state });
      return;
    }

    // Check sequence numbers
    if (frame.control.ns !== this.vr) {
      this.logger.warn('Sequence number error', {
        expected: this.vr,
        received: frame.control.ns
      });
      this.sendRejectFrame();
      return;
    }

    // Update receive state variable
    this.vr = (this.vr + 1) % 128;
    
    // Process Q.931 message if present
    if (frame.information && frame.information.length > 0) {
      this.processQ931Message(frame.information);
    }

    // Send acknowledgment
    this.sendSupervisoryFrame('RR', false);
    
    // Update acknowledgment state
    this.updateAcknowledgmentState(frame.control.nr!);
  }

  private processSupervisoryFrame(frame: LAPDFrame): void {
    switch (frame.control.supervisory) {
      case 'RR':
        this.processSupervisoryRR(frame);
        break;
      case 'RNR':
        this.processSupervisoryRNR(frame);
        break;
      case 'REJ':
        this.processSupervisoryREJ(frame);
        break;
    }
  }

  private processUnnumberedFrame(frame: LAPDFrame): void {
    switch (frame.control.unnumbered) {
      case 'SABME':
        this.processUnnumberedSABME(frame);
        break;
      case 'UA':
        this.processUnnumberedUA(frame);
        break;
      case 'DM':
        this.processUnnumberedDM(frame);
        break;
      case 'DISC':
        this.processUnnumberedDISC(frame);
        break;
      case 'FRMR':
        this.processUnnumberedFRMR(frame);
        break;
      case 'UI':
        this.processUnnumberedUI(frame);
        break;
      case 'XID':
        this.processUnnumberedXID(frame);
        break;
    }
  }

  private processUnnumberedSABME(frame: LAPDFrame): void {
    this.logger.debug('Received SABME frame');
    
    // Reset state variables
    this.vs = 0;
    this.vr = 0;
    this.va = 0;
    
    // Send UA response
    const uaFrame = this.createUnnumberedFrame('UA', false);
    this.sendFrame(uaFrame);
    
    this.state = DChannelState.ESTABLISHED;
    this.emit('established');
  }

  private processUnnumberedUA(frame: LAPDFrame): void {
    if (this.state === DChannelState.AWAITING_ESTABLISHMENT) {
      this.logger.debug('Received UA frame - establishment complete');
      
      this.clearT200Timer();
      this.state = DChannelState.ESTABLISHED;
      
      // Start T203 timer for supervision
      this.startT203Timer();
      
      this.emit('established');
    }
  }

  private processUnnumberedDISC(frame: LAPDFrame): void {
    this.logger.debug('Received DISC frame');
    
    // Send UA response
    const uaFrame = this.createUnnumberedFrame('UA', false);
    this.sendFrame(uaFrame);
    
    this.state = DChannelState.DOWN;
    this.emit('disconnected');
  }

  private processQ931Message(data: Buffer): void {
    try {
      const q931Message = this.parseQ931Message(data);
      
      this.logger.debug('Received Q.931 message', {
        messageType: q931Message.messageType,
        callReference: q931Message.callReference.value
      });

      this.emit('q931Message', q931Message);
    } catch (error) {
      this.logger.error('Error parsing Q.931 message', error);
    }
  }

  async sendQ931Message(message: Q931Message): Promise<void> {
    if (this.state !== DChannelState.ESTABLISHED && 
        this.state !== DChannelState.MULTIPLE_FRAME_ESTABLISHED) {
      throw new Error('D-Channel not established');
    }

    const messageData = this.buildQ931Message(message);
    const iframe = this.createInformationFrame(messageData);
    
    await this.sendFrame(iframe);
  }

  private async sendFrame(frame: LAPDFrame): Promise<void> {
    const frameData = this.buildLAPDFrame(frame);
    
    this.logger.trace('Sending LAPD frame', {
      sapi: frame.address.sapi,
      tei: frame.address.tei,
      type: frame.control.type
    });

    this.emit('frameOut', frameData);
    
    // Handle retransmission for I-frames and certain U-frames
    if ((frame.control.type === 'I') || 
        (frame.control.type === 'U' && frame.control.unnumbered === 'SABME')) {
      this.acknowledgePending.push(frame);
      this.startT200Timer();
    }
  }

  // Frame creation methods
  private createInformationFrame(information: Buffer): LAPDFrame {
    return {
      address: {
        sapi: this.config.sapi,
        cr: true,
        ea0: false,
        tei: this.config.tei,
        ea1: true
      },
      control: {
        type: 'I',
        ns: this.vs,
        nr: this.vr,
        pf: false
      },
      information,
      fcs: 0
    };
  }

  private createSupervisoryFrame(type: 'RR' | 'RNR' | 'REJ', pf: boolean): LAPDFrame {
    return {
      address: {
        sapi: this.config.sapi,
        cr: true,
        ea0: false,
        tei: this.config.tei,
        ea1: true
      },
      control: {
        type: 'S',
        supervisory: type,
        nr: this.vr,
        pf
      },
      fcs: 0
    };
  }

  private createUnnumberedFrame(type: 'SABME' | 'DM' | 'UI' | 'DISC' | 'UA' | 'FRMR' | 'XID', pf: boolean): LAPDFrame {
    return {
      address: {
        sapi: this.config.sapi,
        cr: true,
        ea0: false,
        tei: this.config.tei,
        ea1: true
      },
      control: {
        type: 'U',
        unnumbered: type,
        pf
      },
      fcs: 0
    };
  }

  // Frame parsing and building
  private parseLAPDFrame(data: Buffer): LAPDFrame {
    if (data.length < 4) {
      throw new Error('LAPD frame too short');
    }

    let offset = 0;
    
    // Parse address field
    const addr1 = data[offset++];
    const addr2 = data[offset++];
    
    const address = {
      sapi: (addr1 >> 2) & 0x3F,
      cr: (addr1 & 0x02) !== 0,
      ea0: (addr1 & 0x01) !== 0,
      tei: (addr2 >> 1) & 0x7F,
      ea1: (addr2 & 0x01) !== 0
    };

    // Parse control field
    const ctrl = data[offset++];
    let control: any = {};

    if ((ctrl & 0x01) === 0) {
      // I-frame
      control.type = 'I';
      control.ns = (ctrl >> 1) & 0x7F;
      control.pf = (data[offset] & 0x01) !== 0;
      control.nr = (data[offset++] >> 1) & 0x7F;
    } else if ((ctrl & 0x03) === 0x01) {
      // S-frame
      control.type = 'S';
      const sType = (ctrl >> 2) & 0x03;
      control.supervisory = ['RR', 'RNR', 'REJ', 'SREJ'][sType];
      control.pf = (data[offset] & 0x01) !== 0;
      control.nr = (data[offset++] >> 1) & 0x7F;
    } else {
      // U-frame
      control.type = 'U';
      control.pf = (ctrl & 0x10) !== 0;
      
      const uType = ctrl & 0xEF;
      const unnumberedTypes: { [key: number]: string } = {
        0x6F: 'SABME',
        0x0F: 'DM',
        0x03: 'UI',
        0x43: 'DISC',
        0x63: 'UA',
        0x87: 'FRMR',
        0xAF: 'XID'
      };
      control.unnumbered = unnumberedTypes[uType];
    }

    // Parse information field
    let information: Buffer | undefined;
    if (offset < data.length - 2) {
      information = data.slice(offset, data.length - 2);
    }

    // Parse FCS
    const fcs = data.readUInt16LE(data.length - 2);

    return {
      address,
      control,
      information,
      fcs
    };
  }

  private buildLAPDFrame(frame: LAPDFrame): Buffer {
    let length = 4; // Address(2) + Control(1-2) + FCS(2) minimum
    
    if (frame.control.type === 'I' || frame.control.type === 'S') {
      length = 5; // Extended control field
    }
    
    if (frame.information) {
      length += frame.information.length;
    }

    const buffer = Buffer.alloc(length);
    let offset = 0;

    // Build address field
    buffer[offset++] = (frame.address.sapi << 2) | 
                      (frame.address.cr ? 0x02 : 0) | 
                      (frame.address.ea0 ? 0x01 : 0);
    
    buffer[offset++] = (frame.address.tei << 1) | 
                      (frame.address.ea1 ? 0x01 : 0);

    // Build control field
    if (frame.control.type === 'I') {
      buffer[offset++] = (frame.control.ns! << 1);
      buffer[offset++] = (frame.control.nr! << 1) | (frame.control.pf ? 0x01 : 0);
    } else if (frame.control.type === 'S') {
      const sTypes: { [key: string]: number } = {
        'RR': 0, 'RNR': 1, 'REJ': 2, 'SREJ': 3
      };
      buffer[offset++] = 0x01 | (sTypes[frame.control.supervisory!] << 2);
      buffer[offset++] = (frame.control.nr! << 1) | (frame.control.pf ? 0x01 : 0);
    } else {
      const uTypes: { [key: string]: number } = {
        'SABME': 0x6F, 'DM': 0x0F, 'UI': 0x03, 'DISC': 0x43,
        'UA': 0x63, 'FRMR': 0x87, 'XID': 0xAF
      };
      let ctrl = uTypes[frame.control.unnumbered!] || 0x03;
      if (frame.control.pf) {
        ctrl |= 0x10;
      }
      buffer[offset++] = ctrl;
    }

    // Copy information field
    if (frame.information) {
      frame.information.copy(buffer, offset);
      offset += frame.information.length;
    }

    // Calculate and write FCS
    const fcs = this.calculateFCS(buffer.slice(0, offset));
    buffer.writeUInt16LE(fcs, offset);

    return buffer;
  }

  private parseQ931Message(data: Buffer): Q931Message {
    if (data.length < 3) {
      throw new Error('Q.931 message too short');
    }

    let offset = 0;
    const protocolDiscriminator = data[offset++];
    
    // Parse call reference
    const callRefLength = data[offset++] & 0x0F;
    const callRefFlag = (data[offset] & 0x80) !== 0;
    let callRefValue = 0;
    
    for (let i = 0; i < callRefLength; i++) {
      callRefValue = (callRefValue << 8) | (data[offset++] & (i === 0 ? 0x7F : 0xFF));
    }

    const messageType = data[offset++];
    
    // Parse information elements
    const informationElements: Q931InformationElement[] = [];
    
    while (offset < data.length) {
      const ieId = data[offset++];
      let ieLength: number;
      
      if (ieId & 0x80) {
        // Single octet IE
        ieLength = 0;
        informationElements.push({
          id: ieId,
          data: Buffer.alloc(0)
        });
      } else {
        // Variable length IE
        ieLength = data[offset++];
        const ieData = data.slice(offset, offset + ieLength);
        offset += ieLength;
        
        informationElements.push({
          id: ieId,
          data: ieData
        });
      }
    }

    return {
      protocolDiscriminator,
      callReference: {
        length: callRefLength,
        flag: callRefFlag,
        value: callRefValue
      },
      messageType,
      informationElements
    };
  }

  private buildQ931Message(message: Q931Message): Buffer {
    let length = 3 + message.callReference.length; // PD + CRL + CRV + MT
    
    for (const ie of message.informationElements) {
      if (ie.id & 0x80) {
        length += 1; // Single octet IE
      } else {
        length += 2 + ie.data.length; // IE ID + Length + Data
      }
    }

    const buffer = Buffer.alloc(length);
    let offset = 0;

    // Protocol discriminator
    buffer[offset++] = message.protocolDiscriminator;
    
    // Call reference length and flag
    buffer[offset++] = message.callReference.length;
    
    // Call reference value
    let crv = message.callReference.value;
    if (message.callReference.flag) {
      crv |= 0x80;
    }
    
    for (let i = message.callReference.length - 1; i >= 0; i--) {
      buffer[offset++] = (crv >> (i * 8)) & 0xFF;
    }

    // Message type
    buffer[offset++] = message.messageType;
    
    // Information elements
    for (const ie of message.informationElements) {
      if (ie.id & 0x80) {
        // Single octet IE
        buffer[offset++] = ie.id;
      } else {
        // Variable length IE
        buffer[offset++] = ie.id;
        buffer[offset++] = ie.data.length;
        ie.data.copy(buffer, offset);
        offset += ie.data.length;
      }
    }

    return buffer;
  }

  private calculateFCS(data: Buffer): number {
    // Simplified FCS calculation (CRC-16)
    let fcs = 0xFFFF;
    
    for (let i = 0; i < data.length; i++) {
      fcs ^= data[i] << 8;
      for (let j = 0; j < 8; j++) {
        if (fcs & 0x8000) {
          fcs = (fcs << 1) ^ 0x1021;
        } else {
          fcs <<= 1;
        }
        fcs &= 0xFFFF;
      }
    }
    
    return fcs ^ 0xFFFF;
  }

  // Timer management
  private startT200Timer(): void {
    this.clearT200Timer();
    this.t200Timer = setTimeout(() => {
      this.handleT200Timeout();
    }, this.config.t200Timer);
  }

  private clearT200Timer(): void {
    if (this.t200Timer) {
      clearTimeout(this.t200Timer);
      this.t200Timer = undefined;
    }
  }

  private startT203Timer(): void {
    this.clearT203Timer();
    this.t203Timer = setTimeout(() => {
      this.handleT203Timeout();
    }, this.config.t203Timer);
  }

  private clearT203Timer(): void {
    if (this.t203Timer) {
      clearTimeout(this.t203Timer);
      this.t203Timer = undefined;
    }
  }

  private restartT203Timer(): void {
    this.startT203Timer();
  }

  private clearAllTimers(): void {
    this.clearT200Timer();
    this.clearT203Timer();
    
    if (this.t201Timer) {
      clearTimeout(this.t201Timer);
      this.t201Timer = undefined;
    }
    
    if (this.t202Timer) {
      clearTimeout(this.t202Timer);
      this.t202Timer = undefined;
    }
  }

  private handleT200Timeout(): void {
    this.logger.warn('T200 timeout - retransmission required');
    
    if (this.retransmissionCount >= this.config.maxRetransmissions) {
      this.logger.error('Maximum retransmissions reached');
      this.state = DChannelState.DOWN;
      this.emit('error', new Error('Layer 2 establishment failed'));
      return;
    }

    // Retransmit pending frames
    for (const frame of this.acknowledgePending) {
      this.sendFrame(frame);
    }
    
    this.retransmissionCount++;
    this.startT200Timer();
  }

  private handleT203Timeout(): void {
    this.logger.warn('T203 timeout - sending RR frame for supervision');
    
    const rrFrame = this.createSupervisoryFrame('RR', true);
    this.sendFrame(rrFrame);
    
    this.startT203Timer();
  }

  // Helper methods
  private validateFrameAddress(frame: LAPDFrame): boolean {
    return frame.address.sapi === this.config.sapi && 
           frame.address.tei === this.config.tei;
  }

  private sendSupervisoryFrame(type: 'RR' | 'RNR' | 'REJ', pf: boolean): void {
    const frame = this.createSupervisoryFrame(type, pf);
    this.sendFrame(frame);
  }

  private sendRejectFrame(): void {
    this.sendSupervisoryFrame('REJ', false);
  }

  private updateAcknowledgmentState(nr: number): void {
    // Remove acknowledged frames from pending queue
    this.acknowledgePending = this.acknowledgePending.filter(frame => {
      if (frame.control.type === 'I' && frame.control.ns! < nr) {
        return false; // Remove acknowledged frame
      }
      return true;
    });
    
    this.va = nr;
    
    if (this.acknowledgePending.length === 0) {
      this.clearT200Timer();
    }
  }

  private processSupervisoryRR(frame: LAPDFrame): void {
    this.peerBusy = false;
    this.updateAcknowledgmentState(frame.control.nr!);
  }

  private processSupervisoryRNR(frame: LAPDFrame): void {
    this.peerBusy = true;
    this.updateAcknowledgmentState(frame.control.nr!);
  }

  private processSupervisoryREJ(frame: LAPDFrame): void {
    this.peerBusy = false;
    this.updateAcknowledgmentState(frame.control.nr!);
    
    // Retransmit from N(R)
    this.vs = frame.control.nr!;
    // Retransmit all I-frames from N(R)
  }

  private processUnnumberedDM(frame: LAPDFrame): void {
    this.logger.debug('Received DM frame');
    this.state = DChannelState.DOWN;
    this.emit('disconnected');
  }

  private processUnnumberedFRMR(frame: LAPDFrame): void {
    this.logger.error('Received FRMR frame - frame reject');
    this.state = DChannelState.DOWN;
    this.emit('error', new Error('Frame rejected by peer'));
  }

  private processUnnumberedUI(frame: LAPDFrame): void {
    // Unnumbered Information frame - process information if present
    if (frame.information) {
      this.emit('uiFrame', frame.information);
    }
  }

  private processUnnumberedXID(frame: LAPDFrame): void {
    this.logger.debug('Received XID frame');
    // Process parameter negotiation if needed
    this.emit('xidFrame', frame.information);
  }

  // Public API
  getState(): DChannelState {
    return this.state;
  }

  getConfig(): DChannelConfig {
    return { ...this.config };
  }

  isEstablished(): boolean {
    return this.state === DChannelState.ESTABLISHED || 
           this.state === DChannelState.MULTIPLE_FRAME_ESTABLISHED;
  }

  getStatistics(): any {
    return {
      state: this.state,
      vs: this.vs,
      vr: this.vr,
      va: this.va,
      retransmissionCount: this.retransmissionCount,
      pendingFrames: this.acknowledgePending.length,
      ownBusy: this.ownBusy,
      peerBusy: this.peerBusy
    };
  }
}