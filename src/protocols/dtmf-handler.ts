import { EventEmitter } from 'events';
import { Logger } from '../utils/logger';
import { DTMFConfig } from '../types';

export interface DTMFEvent {
  digit: string;
  duration: number;
  volume: number;
  method: 'rfc2833' | 'sip-info' | 'inband';
  timestamp: Date;
  sessionId: string;
}

export interface RFC2833Packet {
  event: number;        // DTMF event code (0-15)
  endOfEvent: boolean;  // E bit
  reserved: boolean;    // R bit
  volume: number;       // Volume (0-63, where 0 is loudest)
  duration: number;     // Duration in timestamp units
}

export interface SIPInfoDTMF {
  signal: string;       // DTMF digit
  duration?: number;    // Duration in ms
}

export interface InbandDTMF {
  digit: string;
  lowFreq: number;
  highFreq: number;
  amplitude: number;
  duration: number;
}

export class DTMFHandler extends EventEmitter {
  private config: DTMFConfig;
  private logger: Logger;
  private activeTones: Map<string, NodeJS.Timeout> = new Map();
  private dtmfMatrix: Map<string, { low: number; high: number }> = new Map();
  private sequenceNumbers: Map<string, number> = new Map();

  constructor(config: DTMFConfig, logger: Logger) {
    super();
    this.config = config;
    this.logger = logger.child({ component: 'dtmf-handler' });
    this.initializeDTMFMatrix();
  }

  private initializeDTMFMatrix(): void {
    // ITU-T Q.23 DTMF frequency matrix
    this.dtmfMatrix.set('1', { low: 697, high: 1209 });
    this.dtmfMatrix.set('2', { low: 697, high: 1336 });
    this.dtmfMatrix.set('3', { low: 697, high: 1477 });
    this.dtmfMatrix.set('A', { low: 697, high: 1633 });
    this.dtmfMatrix.set('4', { low: 770, high: 1209 });
    this.dtmfMatrix.set('5', { low: 770, high: 1336 });
    this.dtmfMatrix.set('6', { low: 770, high: 1477 });
    this.dtmfMatrix.set('B', { low: 770, high: 1633 });
    this.dtmfMatrix.set('7', { low: 852, high: 1209 });
    this.dtmfMatrix.set('8', { low: 852, high: 1336 });
    this.dtmfMatrix.set('9', { low: 852, high: 1477 });
    this.dtmfMatrix.set('C', { low: 852, high: 1633 });
    this.dtmfMatrix.set('*', { low: 941, high: 1209 });
    this.dtmfMatrix.set('0', { low: 941, high: 1336 });
    this.dtmfMatrix.set('#', { low: 941, high: 1477 });
    this.dtmfMatrix.set('D', { low: 941, high: 1633 });
  }

  // Send DTMF using configured method
  async sendDTMF(sessionId: string, digits: string, method?: 'rfc2833' | 'sip-info' | 'inband'): Promise<void> {
    const dtmfMethod = method || this.config.method;
    
    this.logger.debug('Sending DTMF', {
      sessionId,
      digits,
      method: dtmfMethod
    });

    for (let i = 0; i < digits.length; i++) {
      const digit = digits[i].toUpperCase();
      
      if (!this.isValidDTMFDigit(digit)) {
        this.logger.warn('Invalid DTMF digit', { digit });
        continue;
      }

      // Send the digit
      switch (dtmfMethod) {
        case 'rfc2833':
          await this.sendRFC2833DTMF(sessionId, digit);
          break;
        case 'sip-info':
          await this.sendSIPInfoDTMF(sessionId, digit);
          break;
        case 'inband':
          await this.sendInbandDTMF(sessionId, digit);
          break;
        case 'auto':
          await this.sendAutoDTMF(sessionId, digit);
          break;
      }

      // Wait for inter-digit delay
      if (i < digits.length - 1) {
        await this.delay(this.config.interDigitDelay);
      }
    }
  }

  // RFC2833 DTMF (RTP telephone-event)
  private async sendRFC2833DTMF(sessionId: string, digit: string): Promise<void> {
    const eventCode = this.getEventCode(digit);
    const duration = Math.floor(this.config.duration * 8); // Convert ms to RTP timestamp units (8kHz)
    
    // Create RFC2833 packets
    const packets = this.createRFC2833Packets(eventCode, duration);
    
    for (let i = 0; i < packets.length; i++) {
      const packet = packets[i];
      const isLast = i === packets.length - 1;
      
      // Set end-of-event bit for last packet
      if (isLast && this.config.endOfEvent) {
        packet.endOfEvent = true;
      }

      const rtpPayload = this.buildRFC2833Payload(packet);
      
      this.emit('rfc2833Packet', {
        sessionId,
        payloadType: this.config.payloadType,
        payload: rtpPayload,
        sequenceNumber: this.getNextSequenceNumber(sessionId),
        timestamp: Date.now()
      });

      // Send redundant packets for reliability
      for (let r = 0; r < this.config.redundancy; r++) {
        this.emit('rfc2833Packet', {
          sessionId,
          payloadType: this.config.payloadType,
          payload: rtpPayload,
          sequenceNumber: this.getNextSequenceNumber(sessionId),
          timestamp: Date.now()
        });
      }

      // Small delay between packets
      await this.delay(20);
    }

    this.emitDTMFEvent(sessionId, digit, 'rfc2833');
  }

  // SIP INFO DTMF
  private async sendSIPInfoDTMF(sessionId: string, digit: string): Promise<void> {
    const sipInfo: SIPInfoDTMF = {
      signal: digit,
      duration: this.config.duration
    };

    let body = '';
    
    if (this.config.sipInfoContentType === 'application/dtmf-relay') {
      body = `Signal=${digit}\r\nDuration=${this.config.duration}`;
    } else if (this.config.sipInfoContentType === 'application/dtmf') {
      body = digit;
    } else {
      // Custom format
      body = JSON.stringify(sipInfo);
    }

    this.emit('sipInfoMessage', {
      sessionId,
      method: 'INFO',
      headers: {
        'Content-Type': this.config.sipInfoContentType,
        'Content-Length': body.length.toString()
      },
      body
    });

    this.emitDTMFEvent(sessionId, digit, 'sip-info');
  }

  // Inband DTMF (audio tones)
  private async sendInbandDTMF(sessionId: string, digit: string): Promise<void> {
    const frequencies = this.dtmfMatrix.get(digit);
    if (!frequencies) {
      throw new Error(`Invalid DTMF digit: ${digit}`);
    }

    const inbandDTMF: InbandDTMF = {
      digit,
      lowFreq: frequencies.low,
      highFreq: frequencies.high,
      amplitude: this.volumeToAmplitude(this.config.volume),
      duration: this.config.duration
    };

    // Generate inband DTMF tone
    const audioSamples = this.generateDTMFTone(inbandDTMF);
    
    this.emit('inbandAudio', {
      sessionId,
      samples: audioSamples,
      sampleRate: 8000,
      channels: 1,
      duration: this.config.duration
    });

    this.emitDTMFEvent(sessionId, digit, 'inband');
  }

  // Auto DTMF (choose best method based on peer capabilities)
  private async sendAutoDTMF(sessionId: string, digit: string): Promise<void> {
    // Check peer capabilities and choose best method
    // This would be determined during SDP negotiation
    const peerSupportsRFC2833 = this.checkPeerRFC2833Support(sessionId);
    const peerSupportsSIPInfo = this.checkPeerSIPInfoSupport(sessionId);

    if (peerSupportsRFC2833) {
      await this.sendRFC2833DTMF(sessionId, digit);
    } else if (peerSupportsSIPInfo) {
      await this.sendSIPInfoDTMF(sessionId, digit);
    } else {
      await this.sendInbandDTMF(sessionId, digit);
    }
  }

  // Process received DTMF
  processReceivedRFC2833(sessionId: string, payload: Buffer): void {
    try {
      const packet = this.parseRFC2833Payload(payload);
      const digit = this.getDigitFromEventCode(packet.event);
      
      this.logger.debug('Received RFC2833 DTMF', {
        sessionId,
        digit,
        event: packet.event,
        endOfEvent: packet.endOfEvent,
        volume: packet.volume,
        duration: packet.duration
      });

      if (packet.endOfEvent) {
        this.emitDTMFEvent(sessionId, digit, 'rfc2833');
      }
    } catch (error) {
      this.logger.error('Error processing RFC2833 packet', error);
    }
  }

  processReceivedSIPInfo(sessionId: string, contentType: string, body: string): void {
    try {
      let digit = '';
      let duration = this.config.duration;

      if (contentType === 'application/dtmf-relay') {
        const lines = body.split('\r\n');
        for (const line of lines) {
          if (line.startsWith('Signal=')) {
            digit = line.split('=')[1];
          } else if (line.startsWith('Duration=')) {
            duration = parseInt(line.split('=')[1]);
          }
        }
      } else if (contentType === 'application/dtmf') {
        digit = body.trim();
      } else {
        // Try to parse as JSON
        const parsed = JSON.parse(body);
        digit = parsed.signal || parsed.digit;
        duration = parsed.duration || duration;
      }

      if (this.isValidDTMFDigit(digit)) {
        this.logger.debug('Received SIP INFO DTMF', {
          sessionId,
          digit,
          duration,
          contentType
        });

        this.emitDTMFEvent(sessionId, digit, 'sip-info');
      }
    } catch (error) {
      this.logger.error('Error processing SIP INFO DTMF', error);
    }
  }

  processInbandAudio(sessionId: string, audioSamples: Float32Array): void {
    // Detect DTMF tones in audio samples using Goertzel algorithm
    const detectedDigits = this.detectInbandDTMF(audioSamples);
    
    for (const digit of detectedDigits) {
      this.logger.debug('Detected inband DTMF', {
        sessionId,
        digit
      });

      this.emitDTMFEvent(sessionId, digit, 'inband');
    }
  }

  // Helper methods
  private createRFC2833Packets(eventCode: number, totalDuration: number): RFC2833Packet[] {
    const packets: RFC2833Packet[] = [];
    const packetDuration = 160; // 20ms at 8kHz
    const numPackets = Math.ceil(totalDuration / packetDuration);

    for (let i = 0; i < numPackets; i++) {
      const currentDuration = Math.min(packetDuration, totalDuration - (i * packetDuration));
      
      packets.push({
        event: eventCode,
        endOfEvent: false,
        reserved: false,
        volume: this.config.volume < 0 ? Math.abs(this.config.volume) : 10,
        duration: (i + 1) * packetDuration
      });
    }

    return packets;
  }

  private buildRFC2833Payload(packet: RFC2833Packet): Buffer {
    const payload = Buffer.alloc(4);
    
    // Event + E + R + Volume
    payload[0] = packet.event;
    payload[1] = (packet.endOfEvent ? 0x80 : 0) | 
                 (packet.reserved ? 0x40 : 0) | 
                 (packet.volume & 0x3F);
    
    // Duration (16-bit big-endian)
    payload.writeUInt16BE(packet.duration, 2);
    
    return payload;
  }

  private parseRFC2833Payload(payload: Buffer): RFC2833Packet {
    if (payload.length < 4) {
      throw new Error('Invalid RFC2833 payload length');
    }

    return {
      event: payload[0],
      endOfEvent: (payload[1] & 0x80) !== 0,
      reserved: (payload[1] & 0x40) !== 0,
      volume: payload[1] & 0x3F,
      duration: payload.readUInt16BE(2)
    };
  }

  private generateDTMFTone(dtmf: InbandDTMF): Float32Array {
    const sampleRate = 8000;
    const samples = Math.floor(dtmf.duration * sampleRate / 1000);
    const audioSamples = new Float32Array(samples);

    for (let i = 0; i < samples; i++) {
      const t = i / sampleRate;
      const lowTone = Math.sin(2 * Math.PI * dtmf.lowFreq * t);
      const highTone = Math.sin(2 * Math.PI * dtmf.highFreq * t);
      
      // Combine tones with equal amplitude
      audioSamples[i] = (lowTone + highTone) * dtmf.amplitude * 0.5;
    }

    return audioSamples;
  }

  private detectInbandDTMF(audioSamples: Float32Array): string[] {
    const detectedDigits: string[] = [];
    const sampleRate = 8000;
    const windowSize = 160; // 20ms window
    
    // Use Goertzel algorithm to detect DTMF frequencies
    for (let i = 0; i < audioSamples.length - windowSize; i += windowSize) {
      const window = audioSamples.slice(i, i + windowSize);
      const digit = this.goertzelDTMFDetection(window, sampleRate);
      
      if (digit) {
        detectedDigits.push(digit);
      }
    }

    return detectedDigits;
  }

  private goertzelDTMFDetection(samples: Float32Array, sampleRate: number): string | null {
    const dtmfFreqs = [697, 770, 852, 941, 1209, 1336, 1477, 1633];
    const powers: number[] = [];
    
    // Calculate power for each DTMF frequency
    for (const freq of dtmfFreqs) {
      const k = Math.round(freq * samples.length / sampleRate);
      const omega = (2 * Math.PI * k) / samples.length;
      
      let q1 = 0, q2 = 0;
      
      for (let i = 0; i < samples.length; i++) {
        const q0 = 2 * Math.cos(omega) * q1 - q2 + samples[i];
        q2 = q1;
        q1 = q0;
      }
      
      const power = q1 * q1 + q2 * q2 - q1 * q2 * 2 * Math.cos(omega);
      powers.push(power);
    }

    // Find the two strongest frequencies
    const threshold = 1000000; // Adjust based on signal strength
    const lowFreqIndex = this.findMaxIndex(powers.slice(0, 4));
    const highFreqIndex = this.findMaxIndex(powers.slice(4, 8)) + 4;

    if (powers[lowFreqIndex] > threshold && powers[highFreqIndex] > threshold) {
      const lowFreq = dtmfFreqs[lowFreqIndex];
      const highFreq = dtmfFreqs[highFreqIndex];
      
      // Find digit corresponding to frequency pair
      for (const [digit, freqs] of this.dtmfMatrix) {
        if (freqs.low === lowFreq && freqs.high === highFreq) {
          return digit;
        }
      }
    }

    return null;
  }

  private findMaxIndex(array: number[]): number {
    let maxIndex = 0;
    let maxValue = array[0];
    
    for (let i = 1; i < array.length; i++) {
      if (array[i] > maxValue) {
        maxValue = array[i];
        maxIndex = i;
      }
    }
    
    return maxIndex;
  }

  private getEventCode(digit: string): number {
    const eventCodes: { [key: string]: number } = {
      '0': 0, '1': 1, '2': 2, '3': 3, '4': 4, '5': 5, '6': 6, '7': 7,
      '8': 8, '9': 9, '*': 10, '#': 11, 'A': 12, 'B': 13, 'C': 14, 'D': 15
    };
    return eventCodes[digit] || 0;
  }

  private getDigitFromEventCode(eventCode: number): string {
    const digits = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '*', '#', 'A', 'B', 'C', 'D'];
    return digits[eventCode] || '0';
  }

  private isValidDTMFDigit(digit: string): boolean {
    return /^[0-9A-D*#]$/.test(digit);
  }

  private volumeToAmplitude(volume: number): number {
    // Convert dBm0 to linear amplitude (0 dBm0 = 1.0)
    return Math.pow(10, volume / 20);
  }

  private checkPeerRFC2833Support(sessionId: string): boolean {
    // This would check SDP negotiation results
    // For now, assume support
    return true;
  }

  private checkPeerSIPInfoSupport(sessionId: string): boolean {
    // This would check Allow header or previous negotiations
    // For now, assume support
    return true;
  }

  private getNextSequenceNumber(sessionId: string): number {
    const current = this.sequenceNumbers.get(sessionId) || 0;
    const next = (current + 1) % 65536;
    this.sequenceNumbers.set(sessionId, next);
    return next;
  }

  private emitDTMFEvent(sessionId: string, digit: string, method: 'rfc2833' | 'sip-info' | 'inband'): void {
    const event: DTMFEvent = {
      digit,
      duration: this.config.duration,
      volume: this.config.volume,
      method,
      timestamp: new Date(),
      sessionId
    };

    this.emit('dtmfReceived', event);
  }

  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  // Public API methods
  updateConfig(config: Partial<DTMFConfig>): void {
    Object.assign(this.config, config);
    this.logger.info('DTMF configuration updated', config);
  }

  getConfig(): DTMFConfig {
    return { ...this.config };
  }

  getSupportedMethods(): string[] {
    return ['rfc2833', 'sip-info', 'inband', 'auto'];
  }

  validateDTMFString(digits: string): boolean {
    return /^[0-9A-D*#]+$/.test(digits);
  }

  clearSession(sessionId: string): void {
    // Clear any active tones for this session
    const timeoutKey = `${sessionId}-tone`;
    const timeout = this.activeTones.get(timeoutKey);
    if (timeout) {
      clearTimeout(timeout);
      this.activeTones.delete(timeoutKey);
    }

    // Clear sequence numbers
    this.sequenceNumbers.delete(sessionId);
  }
}