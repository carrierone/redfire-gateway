import { EventEmitter } from 'events';
import { Logger } from '../utils/logger';

export enum FeatureGroupType {
  FGD = 'fgd',  // Feature Group D (Equal Access)
  FGB = 'fgb'   // Feature Group B (Access Tandem)
}

export enum ANIDeliveryMethod {
  WINK_START = 'wink_start',
  IMMEDIATE_START = 'immediate_start',
  DELAY_DIAL = 'delay_dial'
}

export enum SignalingType {
  SF = 'sf',           // Single Frequency
  E_AND_M = 'e_and_m', // E&M signaling
  WINK_START = 'wink_start',
  IMMEDIATE_START = 'immediate_start',
  DELAY_DIAL = 'delay_dial'
}

export interface FeatureGroupConfig {
  type: FeatureGroupType;
  signaling: SignalingType;
  aniDelivery: ANIDeliveryMethod;
  seizureTimeout: number;        // Time to wait for seizure ack
  winkDuration: number;         // Wink pulse duration (ms)
  winkTimeout: number;          // Time to wait for wink response
  digitTimeout: number;         // Inter-digit timeout
  kpDuration: number;           // KP (Key Pulse) duration
  stDuration: number;           // ST (Start) pulse duration
  mfToneDuration: number;       // MF tone duration
  interToneDelay: number;       // Delay between MF tones
  answerSupervision: boolean;   // Enable answer supervision
  disconnectSupervision: boolean; // Enable disconnect supervision
  spill: {
    enabled: boolean;
    maxDigits: number;
    timeout: number;
  };
  ani: {
    enabled: boolean;
    format: 'mf' | 'dtmf' | 'ss7';
    includeOLI: boolean;        // Originating Line Information
    includeCIC: boolean;        // Carrier Identification Code
  };
  billing: {
    enabled: boolean;
    collectRecords: boolean;
    ratingMethod: 'flat_rate' | 'usage_based' | 'distance_based';
  };
}

export interface MFSignaling {
  kp: number[];     // Key Pulse frequencies
  st: number[];     // Start frequencies  
  digits: { [key: string]: number[] }; // MF digit frequencies
}

export interface FeatureGroupCall {
  id: string;
  type: FeatureGroupType;
  direction: 'inbound' | 'outbound';
  state: 'idle' | 'seizing' | 'winking' | 'dialing' | 'proceeding' | 'answered' | 'releasing';
  channelId: number;
  ani?: string;           // Automatic Number Identification
  dnis?: string;          // Dialed Number Identification Service
  oli?: string;           // Originating Line Information  
  cic?: string;           // Carrier Identification Code
  calledNumber?: string;
  seizureTime?: Date;
  answerTime?: Date;
  releaseTime?: Date;
  billingRecords: BillingRecord[];
}

export interface BillingRecord {
  recordType: 'start' | 'answer' | 'end';
  timestamp: Date;
  duration?: number;      // Call duration in seconds
  digits?: string;        // Dialed digits
  ani?: string;
  charge?: number;        // Billing amount
  tariff?: string;        // Applied tariff
}

export class FeatureGroupHandler extends EventEmitter {
  private config: FeatureGroupConfig;
  private logger: Logger;
  private activeCalls: Map<string, FeatureGroupCall> = new Map();
  private mfSignaling: MFSignaling;
  private timers: Map<string, NodeJS.Timeout> = new Map();

  constructor(config: FeatureGroupConfig, logger: Logger) {
    super();
    this.config = config;
    this.logger = logger.child({ 
      component: 'feature-group',
      type: config.type
    });
    this.initializeMFSignaling();
  }

  private initializeMFSignaling(): void {
    // ITU-T Q.310-Q.332 MF signaling frequencies
    this.mfSignaling = {
      kp: [1100, 1700],      // Key Pulse
      st: [1500, 1700],      // Start signal
      digits: {
        '1': [700, 900],
        '2': [700, 1100],
        '3': [900, 1100],
        '4': [700, 1300],
        '5': [900, 1300],
        '6': [1100, 1300],
        '7': [700, 1500],
        '8': [900, 1500],
        '9': [1100, 1500],
        '0': [1300, 1500],
        'KP1': [1100, 1700],   // Same as KP
        'KP2': [1300, 1700],   // KP2 for operator assistance
        'ST': [1500, 1700],    // Start signal
        'ST2P': [900, 1700],   // ST2' for priority calls
        'ST3P': [1300, 1700]   // ST3' for emergency calls
      }
    };
  }

  // Initiate outbound Feature Group call
  async initiateOutboundCall(channelId: number, calledNumber: string, ani?: string): Promise<string> {
    const callId = this.generateCallId();
    
    const call: FeatureGroupCall = {
      id: callId,
      type: this.config.type,
      direction: 'outbound',
      state: 'seizing',
      channelId,
      ani,
      calledNumber,
      seizureTime: new Date(),
      billingRecords: []
    };

    this.activeCalls.set(callId, call);

    this.logger.info('Initiating Feature Group call', {
      callId,
      type: this.config.type,
      channelId,
      calledNumber,
      ani
    });

    // Start seizure procedure
    await this.performSeizure(call);

    return callId;
  }

  // Handle inbound Feature Group call
  async handleInboundCall(channelId: number, seizureSignal: Buffer): Promise<string> {
    const callId = this.generateCallId();
    
    const call: FeatureGroupCall = {
      id: callId,
      type: this.config.type,
      direction: 'inbound',
      state: 'seizing',
      channelId,
      seizureTime: new Date(),
      billingRecords: []
    };

    this.activeCalls.set(callId, call);

    this.logger.info('Handling inbound Feature Group call', {
      callId,
      type: this.config.type,
      channelId
    });

    // Process seizure and start digit collection
    await this.processInboundSeizure(call, seizureSignal);

    return callId;
  }

  // Perform outbound seizure
  private async performSeizure(call: FeatureGroupCall): Promise<void> {
    call.state = 'seizing';

    // Send seizure signal based on signaling type
    switch (this.config.signaling) {
      case SignalingType.WINK_START:
        await this.performWinkStartSeizure(call);
        break;
      case SignalingType.IMMEDIATE_START:
        await this.performImmediateStartSeizure(call);
        break;
      case SignalingType.DELAY_DIAL:
        await this.performDelayDialSeizure(call);
        break;
      default:
        throw new Error(`Unsupported signaling type: ${this.config.signaling}`);
    }
  }

  // Wink Start seizure procedure
  private async performWinkStartSeizure(call: FeatureGroupCall): Promise<void> {
    this.logger.debug('Performing Wink Start seizure', { callId: call.id });

    // Send off-hook (seizure)
    this.emit('seizureSignal', {
      callId: call.id,
      channelId: call.channelId,
      signalType: 'off_hook'
    });

    // Wait for wink response
    const winkTimer = setTimeout(() => {
      this.handleSeizureTimeout(call.id);
    }, this.config.winkTimeout);

    this.timers.set(`${call.id}-wink`, winkTimer);
  }

  // Process wink response
  async processWinkResponse(callId: string): Promise<void> {
    const call = this.activeCalls.get(callId);
    if (!call || call.state !== 'seizing') {
      return;
    }

    this.clearTimer(`${callId}-wink`);
    
    this.logger.debug('Received wink response', { callId });
    
    call.state = 'winking';

    // Wait for wink duration, then start dialing
    setTimeout(async () => {
      await this.startDialing(call);
    }, this.config.winkDuration);
  }

  // Start dialing procedure
  private async startDialing(call: FeatureGroupCall): Promise<void> {
    call.state = 'dialing';

    this.logger.debug('Starting MF dialing', { 
      callId: call.id,
      calledNumber: call.calledNumber 
    });

    // Send KP (Key Pulse)
    await this.sendMFTone(call.channelId, 'KP');

    // For Feature Group D, send ANI if available
    if (this.config.type === FeatureGroupType.FGD && call.ani && this.config.ani.enabled) {
      await this.sendANI(call);
    }

    // Send called number
    if (call.calledNumber) {
      await this.sendMFDigits(call.channelId, call.calledNumber);
    }

    // Send ST (Start signal)
    await this.sendMFTone(call.channelId, 'ST');

    call.state = 'proceeding';
    this.emit('dialingComplete', call);
  }

  // Send ANI (Automatic Number Identification)
  private async sendANI(call: FeatureGroupCall): Promise<void> {
    if (!call.ani) return;

    this.logger.debug('Sending ANI', { 
      callId: call.id,
      ani: call.ani 
    });

    // Format ANI based on Feature Group type
    let aniString = call.ani;

    if (this.config.type === FeatureGroupType.FGD) {
      // FGD format: II + 10-digit ANI
      // II = Information Indicator (00 for normal, 02 for operator, etc.)
      aniString = '00' + call.ani.replace(/[^0-9]/g, '');
    }

    // Send ANI digits
    await this.sendMFDigits(call.channelId, aniString);

    // Add OLI (Originating Line Information) if enabled
    if (this.config.ani.includeOLI) {
      const oli = this.determineOLI(call);
      await this.sendMFDigits(call.channelId, oli);
    }

    // Add CIC (Carrier Identification Code) if enabled
    if (this.config.ani.includeCIC && call.cic) {
      await this.sendMFDigits(call.channelId, call.cic);
    }
  }

  // Determine Originating Line Information
  private determineOLI(call: FeatureGroupCall): string {
    // OLI codes (simplified):
    // 00 = Regular line
    // 01 = Multiparty line
    // 02 = ANI failure
    // 06 = Hotel/motel
    // 07 = Coin phone
    // 08 = Cellular
    // 27 = Restricted line
    // 29 = Prison phone
    // 68 = InterLATA restricted
    // 78 = Cellular blocking
    
    // Default to regular line
    return '00';
  }

  // Send MF digits
  private async sendMFDigits(channelId: number, digits: string): Promise<void> {
    for (const digit of digits) {
      await this.sendMFTone(channelId, digit);
      await this.delay(this.config.interToneDelay);
    }
  }

  // Send individual MF tone
  private async sendMFTone(channelId: number, tone: string): Promise<void> {
    const frequencies = this.mfSignaling.digits[tone];
    if (!frequencies) {
      this.logger.warn('Unknown MF tone', { tone });
      return;
    }

    let duration = this.config.mfToneDuration;
    
    // Special durations for control tones
    if (tone === 'KP' || tone === 'KP1' || tone === 'KP2') {
      duration = this.config.kpDuration;
    } else if (tone.startsWith('ST')) {
      duration = this.config.stDuration;
    }

    this.emit('mfTone', {
      channelId,
      frequencies,
      duration,
      tone
    });

    this.logger.trace('Sent MF tone', {
      channelId,
      tone,
      frequencies,
      duration
    });

    await this.delay(duration);
  }

  // Process inbound seizure
  private async processInboundSeizure(call: FeatureGroupCall, seizureSignal: Buffer): Promise<void> {
    this.logger.debug('Processing inbound seizure', { callId: call.id });

    // Send wink response for Wink Start
    if (this.config.signaling === SignalingType.WINK_START) {
      await this.sendWinkResponse(call.channelId);
    }

    // Start digit collection
    call.state = 'dialing';
    this.startInboundDigitCollection(call);
  }

  // Send wink response
  private async sendWinkResponse(channelId: number): Promise<void> {
    this.emit('winkResponse', {
      channelId,
      duration: this.config.winkDuration
    });

    await this.delay(this.config.winkDuration);
  }

  // Start collecting inbound digits
  private startInboundDigitCollection(call: FeatureGroupCall): void {
    this.logger.debug('Starting digit collection', { callId: call.id });

    // Set up digit timeout
    const digitTimer = setTimeout(() => {
      this.handleDigitTimeout(call.id);
    }, this.config.digitTimeout);

    this.timers.set(`${call.id}-digits`, digitTimer);

    this.emit('startDigitCollection', {
      callId: call.id,
      channelId: call.channelId,
      expectedFormat: 'mf'
    });
  }

  // Process received MF digits
  processReceivedMFDigits(callId: string, digits: string): void {
    const call = this.activeCalls.get(callId);
    if (!call) return;

    this.clearTimer(`${callId}-digits`);

    this.logger.debug('Received MF digits', { 
      callId,
      digits: digits.replace(/[^0-9*#]/g, 'X') // Mask for privacy
    });

    // Parse digits based on Feature Group type
    if (this.config.type === FeatureGroupType.FGD) {
      this.parseFGDDigits(call, digits);
    } else {
      this.parseFGBDigits(call, digits);
    }

    call.state = 'proceeding';
    this.emit('digitsReceived', call);
  }

  // Parse Feature Group D digits
  private parseFGDDigits(call: FeatureGroupCall, digits: string): void {
    // FGD format: KP + ANI (12 digits) + Called Number + ST
    let offset = 0;

    // Skip KP
    if (digits.startsWith('KP')) {
      offset = 2;
    }

    // Extract ANI (12 digits: II + 10-digit number)
    if (digits.length >= offset + 12) {
      const aniPart = digits.substr(offset, 12);
      call.ani = aniPart.substr(2); // Remove II (Information Indicator)
      offset += 12;
    }

    // Extract called number (until ST)
    const stIndex = digits.indexOf('ST', offset);
    if (stIndex > offset) {
      call.calledNumber = digits.substring(offset, stIndex);
      call.dnis = call.calledNumber; // DNIS same as called number for FGD
    }
  }

  // Parse Feature Group B digits
  private parseFGBDigits(call: FeatureGroupCall, digits: string): void {
    // FGB format: KP + Called Number + ST (no ANI delivery)
    let offset = 0;

    // Skip KP
    if (digits.startsWith('KP')) {
      offset = 2;
    }

    // Extract called number (until ST)
    const stIndex = digits.indexOf('ST', offset);
    if (stIndex > offset) {
      call.calledNumber = digits.substring(offset, stIndex);
      call.dnis = call.calledNumber;
    }
  }

  // Handle answer supervision
  processAnswerSupervision(callId: string): void {
    const call = this.activeCalls.get(callId);
    if (!call) return;

    call.state = 'answered';
    call.answerTime = new Date();

    this.logger.info('Call answered', { 
      callId,
      answerTime: call.answerTime 
    });

    // Create billing record
    if (this.config.billing.enabled) {
      this.createBillingRecord(call, 'answer');
    }

    this.emit('callAnswered', call);
  }

  // Handle disconnect supervision
  processDisconnectSupervision(callId: string): void {
    const call = this.activeCalls.get(callId);
    if (!call) return;

    call.state = 'releasing';
    call.releaseTime = new Date();

    const duration = call.answerTime ? 
      Math.floor((call.releaseTime.getTime() - call.answerTime.getTime()) / 1000) : 0;

    this.logger.info('Call disconnected', { 
      callId,
      releaseTime: call.releaseTime,
      duration 
    });

    // Create final billing record
    if (this.config.billing.enabled) {
      this.createBillingRecord(call, 'end', duration);
    }

    this.clearCall(callId);
    this.emit('callDisconnected', call);
  }

  // Create billing record
  private createBillingRecord(call: FeatureGroupCall, recordType: 'start' | 'answer' | 'end', duration?: number): void {
    const record: BillingRecord = {
      recordType,
      timestamp: new Date(),
      ani: call.ani,
      digits: call.calledNumber
    };

    if (duration !== undefined) {
      record.duration = duration;
      record.charge = this.calculateCharge(call, duration);
      record.tariff = this.determineTariff(call);
    }

    call.billingRecords.push(record);

    this.emit('billingRecord', {
      callId: call.id,
      record
    });
  }

  // Calculate call charge
  private calculateCharge(call: FeatureGroupCall, duration: number): number {
    // Simplified billing calculation
    switch (this.config.billing.ratingMethod) {
      case 'flat_rate':
        return 0.10; // 10 cents flat rate
      case 'usage_based':
        return Math.ceil(duration / 60) * 0.05; // 5 cents per minute
      case 'distance_based':
        // Would use called number to determine distance
        return Math.ceil(duration / 60) * 0.08; // 8 cents per minute
      default:
        return 0;
    }
  }

  // Determine applicable tariff
  private determineTariff(call: FeatureGroupCall): string {
    if (call.calledNumber?.startsWith('1800') || call.calledNumber?.startsWith('1888')) {
      return 'toll_free';
    } else if (call.calledNumber?.startsWith('1900')) {
      return 'premium';
    } else if (call.calledNumber?.length === 11 && call.calledNumber.startsWith('1')) {
      return 'long_distance';
    } else {
      return 'local';
    }
  }

  // Handle timeouts
  private handleSeizureTimeout(callId: string): void {
    const call = this.activeCalls.get(callId);
    if (!call) return;

    this.logger.warn('Seizure timeout', { callId });
    
    call.state = 'releasing';
    this.clearCall(callId);
    this.emit('seizureTimeout', call);
  }

  private handleDigitTimeout(callId: string): void {
    const call = this.activeCalls.get(callId);
    if (!call) return;

    this.logger.warn('Digit collection timeout', { callId });
    
    call.state = 'releasing';
    this.clearCall(callId);
    this.emit('digitTimeout', call);
  }

  // Utility methods
  private clearTimer(key: string): void {
    const timer = this.timers.get(key);
    if (timer) {
      clearTimeout(timer);
      this.timers.delete(key);
    }
  }

  private clearCall(callId: string): void {
    // Clear all timers for this call
    for (const [key] of this.timers) {
      if (key.startsWith(callId)) {
        this.clearTimer(key);
      }
    }

    this.activeCalls.delete(callId);
  }

  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  private generateCallId(): string {
    return `FG${this.config.type.toUpperCase()}-${Date.now()}-${Math.random().toString(36).substr(2, 6)}`;
  }

  // Public API methods
  getCall(callId: string): FeatureGroupCall | undefined {
    return this.activeCalls.get(callId);
  }

  getAllCalls(): FeatureGroupCall[] {
    return Array.from(this.activeCalls.values());
  }

  getActiveCalls(): FeatureGroupCall[] {
    return this.getAllCalls().filter(call => 
      call.state !== 'idle' && call.state !== 'releasing'
    );
  }

  getBillingRecords(callId?: string): BillingRecord[] {
    if (callId) {
      const call = this.activeCalls.get(callId);
      return call ? call.billingRecords : [];
    }

    // Return all billing records
    const allRecords: BillingRecord[] = [];
    for (const call of this.activeCalls.values()) {
      allRecords.push(...call.billingRecords);
    }
    return allRecords;
  }

  getConfiguration(): FeatureGroupConfig {
    return { ...this.config };
  }

  updateConfiguration(config: Partial<FeatureGroupConfig>): void {
    Object.assign(this.config, config);
    this.logger.info('Feature Group configuration updated', config);
  }

  // Statistics
  getStatistics(): any {
    const calls = this.getAllCalls();
    return {
      totalCalls: calls.length,
      activeCalls: this.getActiveCalls().length,
      answeredCalls: calls.filter(c => c.answerTime).length,
      averageDuration: this.calculateAverageDuration(calls),
      totalRevenue: this.calculateTotalRevenue(calls),
      callsPerHour: this.calculateCallsPerHour(calls)
    };
  }

  private calculateAverageDuration(calls: FeatureGroupCall[]): number {
    const completedCalls = calls.filter(c => c.answerTime && c.releaseTime);
    if (completedCalls.length === 0) return 0;

    const totalDuration = completedCalls.reduce((sum, call) => {
      const duration = call.releaseTime!.getTime() - call.answerTime!.getTime();
      return sum + duration;
    }, 0);

    return Math.floor(totalDuration / completedCalls.length / 1000); // Average in seconds
  }

  private calculateTotalRevenue(calls: FeatureGroupCall[]): number {
    return calls.reduce((total, call) => {
      const revenue = call.billingRecords
        .filter(record => record.charge)
        .reduce((sum, record) => sum + (record.charge || 0), 0);
      return total + revenue;
    }, 0);
  }

  private calculateCallsPerHour(calls: FeatureGroupCall[]): number {
    if (calls.length === 0) return 0;

    const now = new Date();
    const oneHourAgo = new Date(now.getTime() - 60 * 60 * 1000);
    
    return calls.filter(call => 
      call.seizureTime && call.seizureTime > oneHourAgo
    ).length;
  }
}