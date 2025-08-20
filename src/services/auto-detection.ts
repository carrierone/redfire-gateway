import { EventEmitter } from 'events';
import { Logger } from '../utils/logger';
import { GatewayConfig } from '../types';
import * as fs from 'fs/promises';
import * as path from 'path';

export interface DetectionResult {
  spanId: number;
  interface: string;
  layer1: 'e1' | 't1';
  framing: string;
  lineCode: string;
  clockSource: string;
  switchType: string;
  protocol: 'pri' | 'cas' | 'r2' | 'mf' | 'ss7';
  variant: string;
  dChannels: number[];
  nfasGroup?: {
    enabled: boolean;
    primarySpan: number;
    backupSpans: number[];
  };
  confidence: number; // 0-100%
  detectedFeatures: string[];
  vendor?: string;
  model?: string;
}

export interface SignalingPattern {
  pattern: Buffer;
  description: string;
  protocol: string;
  switchType: string;
  variant: string;
  confidence: number;
}

export class AutoDetectionService extends EventEmitter {
  private logger: Logger;
  private isRunning = false;
  private detectionResults: Map<number, DetectionResult> = new Map();
  private signalingPatterns: SignalingPattern[] = [];
  private detectionTimeout = 30000; // 30 seconds
  private frameBuffer: Map<number, Buffer[]> = new Map();

  constructor(logger: Logger) {
    super();
    this.logger = logger.child({ component: 'auto-detection' });
    this.initializeSignalingPatterns();
  }

  private initializeSignalingPatterns(): void {
    // ITU-T Q.931 PRI patterns
    this.signalingPatterns.push(
      {
        pattern: Buffer.from([0x08, 0x02]), // Protocol Discriminator + Call Reference Length
        description: 'ITU-T Q.931 PRI',
        protocol: 'pri',
        switchType: 'euroISDN',
        variant: 'etsi',
        confidence: 95
      },
      // Nortel DMS-100 patterns
      {
        pattern: Buffer.from([0x08, 0x01]), // DMS-100 specific Q.931
        description: 'Nortel DMS-100 PRI',
        protocol: 'pri',
        switchType: 'dms100',
        variant: 'ni2',
        confidence: 90
      },
      // Lucent 5ESS patterns
      {
        pattern: Buffer.from([0x08, 0x02, 0x80]), // 5ESS Q.931 with facility
        description: 'Lucent 5ESS PRI',
        protocol: 'pri',
        switchType: '5ess',
        variant: 'ni2',
        confidence: 90
      },
      // Ericsson AXE patterns
      {
        pattern: Buffer.from([0x08, 0x02, 0x05]), // AXE SETUP message
        description: 'Ericsson AXE PRI',
        protocol: 'pri',
        switchType: 'axe',
        variant: 'etsi',
        confidence: 85
      },
      // Siemens EWSD patterns
      {
        pattern: Buffer.from([0x08, 0x01, 0x05]), // EWSD Q.931
        description: 'Siemens EWSD PRI',
        protocol: 'pri',
        switchType: 'ewsd',
        variant: 'etsi',
        confidence: 85
      },
      // Cisco patterns
      {
        pattern: Buffer.from([0x08, 0x02, 0x45]), // Cisco IOS PRI
        description: 'Cisco IOS PRI',
        protocol: 'pri',
        switchType: 'ios',
        variant: 'ni2',
        confidence: 80
      },
      // Note: R2 MFC patterns removed - D-channel protocols only
      // SS7 ISUP patterns
      {
        pattern: Buffer.from([0x05, 0x01]), // ISUP IAM
        description: 'SS7 ISUP',
        protocol: 'ss7',
        switchType: 'isup',
        variant: 'itu',
        confidence: 95
      }
    );
  }

  async startDetection(interfaces: string[]): Promise<DetectionResult[]> {
    if (this.isRunning) {
      throw new Error('Detection already in progress');
    }

    this.isRunning = true;
    this.detectionResults.clear();
    this.frameBuffer.clear();

    this.logger.info('Starting auto-detection', { interfaces });

    const results: DetectionResult[] = [];

    for (const interfaceName of interfaces) {
      try {
        const result = await this.detectInterface(interfaceName);
        if (result) {
          results.push(result);
          this.detectionResults.set(result.spanId, result);
        }
      } catch (error) {
        this.logger.error(`Detection failed for ${interfaceName}`, error);
      }
    }

    this.isRunning = false;
    this.emit('detectionComplete', results);

    return results;
  }

  private async detectInterface(interfaceName: string): Promise<DetectionResult | null> {
    this.logger.info(`Detecting interface ${interfaceName}`);

    // Step 1: Detect Layer 1 properties
    const layer1Result = await this.detectLayer1(interfaceName);
    if (!layer1Result) {
      return null;
    }

    // Step 2: Detect D-channel(s)
    const dChannels = await this.detectDChannels(interfaceName, layer1Result.layer1);
    
    // Step 3: Analyze signaling on D-channels
    const signalingResult = await this.analyzeSignaling(interfaceName, dChannels);

    // Step 4: Detect NFAS configuration if applicable
    const nfasResult = await this.detectNFAS(interfaceName, dChannels);

    // Step 5: Determine switch type and vendor
    const switchDetection = await this.detectSwitchType(interfaceName, signalingResult);

    const result: DetectionResult = {
      spanId: this.extractSpanId(interfaceName),
      interface: interfaceName,
      layer1: layer1Result.layer1,
      framing: layer1Result.framing,
      lineCode: layer1Result.lineCode,
      clockSource: layer1Result.clockSource,
      switchType: switchDetection.switchType,
      protocol: signalingResult.protocol,
      variant: signalingResult.variant,
      dChannels,
      nfasGroup: nfasResult,
      confidence: Math.min(layer1Result.confidence, signalingResult.confidence),
      detectedFeatures: [
        ...layer1Result.features,
        ...signalingResult.features,
        ...(nfasResult ? ['nfas'] : [])
      ],
      vendor: switchDetection.vendor,
      model: switchDetection.model
    };

    this.logger.info('Detection completed for interface', {
      interface: interfaceName,
      result: {
        layer1: result.layer1,
        protocol: result.protocol,
        switchType: result.switchType,
        confidence: result.confidence
      }
    });

    return result;
  }

  private async detectLayer1(interfaceName: string): Promise<any> {
    this.logger.debug(`Detecting Layer 1 for ${interfaceName}`);

    // Simulate reading from actual hardware/interface
    // In reality, this would interface with the TDM hardware
    const layer1Info = await this.probePhysicalLayer(interfaceName);

    // Analyze timing and framing
    const frameAnalysis = this.analyzeFrameStructure(layer1Info.frames);
    const timingAnalysis = this.analyzeClockTiming(layer1Info.timing);

    let layer1: 'e1' | 't1';
    let framing: string;
    let lineCode: string;
    let confidence = 0;
    const features: string[] = [];

    // Detect E1 vs T1 based on frame structure
    if (frameAnalysis.bitsPerFrame === 256 && frameAnalysis.framesPerSecond === 8000) {
      layer1 = 'e1';
      confidence += 30;
      features.push('e1_detected');

      // E1 framing detection
      if (frameAnalysis.crcBits === 4) {
        framing = 'crc4';
        confidence += 20;
        features.push('crc4_framing');
      } else {
        framing = 'no-crc4';
        confidence += 15;
      }

      // E1 line coding
      if (frameAnalysis.violations > 0) {
        lineCode = 'hdb3';
        confidence += 20;
        features.push('hdb3_encoding');
      } else {
        lineCode = 'ami';
        confidence += 10;
      }

    } else if (frameAnalysis.bitsPerFrame === 193 && frameAnalysis.framesPerSecond === 8000) {
      layer1 = 't1';
      confidence += 30;
      features.push('t1_detected');

      // T1 framing detection
      if (frameAnalysis.esfPattern) {
        framing = 'esf';
        confidence += 20;
        features.push('esf_framing');
      } else {
        framing = 'd4';
        confidence += 15;
      }

      // T1 line coding
      if (frameAnalysis.violations > 0) {
        lineCode = 'b8zs';
        confidence += 20;
        features.push('b8zs_encoding');
      } else {
        lineCode = 'ami';
        confidence += 10;
      }
    } else {
      throw new Error(`Unknown frame structure: ${frameAnalysis.bitsPerFrame} bits/frame`);
    }

    // Clock source detection
    let clockSource: string;
    if (timingAnalysis.jitter < 0.05) {
      clockSource = 'external';
      confidence += 15;
      features.push('external_clock');
    } else if (timingAnalysis.stability > 0.9) {
      clockSource = 'recovered';
      confidence += 10;
      features.push('recovered_clock');
    } else {
      clockSource = 'internal';
      confidence += 5;
    }

    return {
      layer1,
      framing,
      lineCode,
      clockSource,
      confidence,
      features
    };
  }

  private async detectDChannels(interfaceName: string, layer1: 'e1' | 't1'): Promise<number[]> {
    const dChannels: number[] = [];

    // Standard D-channel positions
    const standardDChannels = layer1 === 'e1' ? [16] : [24];

    for (const channel of standardDChannels) {
      if (await this.isChannelSignaling(interfaceName, channel)) {
        dChannels.push(channel);
        this.logger.debug(`D-channel detected on channel ${channel}`);
      }
    }

    // Check for non-standard D-channel positions
    const allChannels = layer1 === 'e1' ? 
      Array.from({length: 31}, (_, i) => i + 1).filter(c => c !== 16) :
      Array.from({length: 24}, (_, i) => i + 1).filter(c => c !== 24);

    for (const channel of allChannels) {
      if (await this.isChannelSignaling(interfaceName, channel)) {
        dChannels.push(channel);
        this.logger.debug(`Non-standard D-channel detected on channel ${channel}`);
      }
    }

    return dChannels.sort((a, b) => a - b);
  }

  private async isChannelSignaling(interfaceName: string, channel: number): Promise<boolean> {
    // Simulate reading channel data
    const channelData = await this.readChannelData(interfaceName, channel, 1000); // 1 second
    
    // Look for LAPD frames (starts with address field)
    const lapdFrames = this.findLAPDFrames(channelData);
    
    // Check for Q.931 messages within LAPD frames
    const q931Messages = this.findQ931Messages(lapdFrames);

    return lapdFrames.length > 0 || q931Messages.length > 0;
  }

  private async analyzeSignaling(interfaceName: string, dChannels: number[]): Promise<any> {
    let bestMatch: SignalingPattern | null = null;
    let protocol = 'pri';
    let variant = 'unknown';
    let confidence = 0;
    const features: string[] = [];

    for (const channel of dChannels) {
      const channelData = await this.readChannelData(interfaceName, channel, 5000); // 5 seconds
      const lapdFrames = this.findLAPDFrames(channelData);
      const q931Messages = this.findQ931Messages(lapdFrames);

      // Analyze message patterns
      for (const message of q931Messages) {
        for (const pattern of this.signalingPatterns) {
          if (this.matchesPattern(message, pattern.pattern)) {
            if (!bestMatch || pattern.confidence > bestMatch.confidence) {
              bestMatch = pattern;
            }
          }
        }
      }

      // Analyze Information Elements for vendor-specific features
      const ieAnalysis = this.analyzeInformationElements(q931Messages);
      features.push(...ieAnalysis.features);

      // Look for proprietary extensions
      const proprietaryFeatures = this.detectProprietaryFeatures(q931Messages);
      features.push(...proprietaryFeatures);
    }

    if (bestMatch) {
      protocol = bestMatch.protocol;
      variant = bestMatch.variant;
      confidence = bestMatch.confidence;
      features.push(`${bestMatch.switchType}_detected`);
    }

    return {
      protocol,
      variant,
      confidence,
      features
    };
  }

  private async detectNFAS(interfaceName: string, dChannels: number[]): Promise<any> {
    if (dChannels.length <= 1) {
      return null; // NFAS requires multiple D-channels
    }

    this.logger.debug('Analyzing for NFAS configuration', { dChannels });

    // Look for NFAS-specific messages and CES values
    let primaryChannel: number | null = null;
    const backupChannels: number[] = [];

    for (const channel of dChannels) {
      const channelData = await this.readChannelData(interfaceName, channel, 3000);
      const lapdFrames = this.findLAPDFrames(channelData);
      
      // Analyze LAPD frames for NFAS indicators
      for (const frame of lapdFrames) {
        const cesValue = this.extractCESFromFrame(frame);
        if (cesValue !== null) {
          if (cesValue === 0 && primaryChannel === null) {
            primaryChannel = channel;
          } else if (cesValue > 0) {
            backupChannels.push(channel);
          }
        }
      }
    }

    if (primaryChannel !== null && backupChannels.length > 0) {
      return {
        enabled: true,
        primarySpan: this.extractSpanId(interfaceName),
        backupSpans: backupChannels.map(ch => this.extractSpanId(interfaceName))
      };
    }

    return null;
  }

  private async detectSwitchType(interfaceName: string, signalingResult: any): Promise<any> {
    let switchType = 'unknown';
    let vendor = 'unknown';
    let model = 'unknown';

    // Switch type detection based on signaling characteristics
    if (signalingResult.features.includes('dms100_detected')) {
      switchType = 'dms100';
      vendor = 'Nortel';
      model = 'DMS-100';
    } else if (signalingResult.features.includes('5ess_detected')) {
      switchType = '5ess';
      vendor = 'Lucent';
      model = '5ESS';
    } else if (signalingResult.features.includes('axe_detected')) {
      switchType = 'axe';
      vendor = 'Ericsson';
      model = 'AXE';
    } else if (signalingResult.features.includes('ewsd_detected')) {
      switchType = 'ewsd';
      vendor = 'Siemens';
      model = 'EWSD';
    } else if (signalingResult.features.includes('ios_detected')) {
      switchType = 'ios';
      vendor = 'Cisco';
      model = 'IOS';
    } else if (signalingResult.protocol === 'pri') {
      switchType = 'euroISDN'; // Default for ETSI PRI
      vendor = 'Generic';
      model = 'ETSI';
    }

    return { switchType, vendor, model };
  }

  async generateConfiguration(results: DetectionResult[]): Promise<GatewayConfig> {
    this.logger.info('Generating configuration from detection results');

    if (results.length === 0) {
      throw new Error('No detection results available');
    }

    // Use the first (or highest confidence) result as primary
    const primaryResult = results.reduce((best, current) => 
      current.confidence > best.confidence ? current : best
    );

    // Generate codec configuration based on detected layer1
    const codecConfig = {
      allowedCodecs: primaryResult.layer1 === 'e1' ? ['g711a'] : ['g711u'],
      preferredCodec: primaryResult.layer1 === 'e1' ? 'g711a' : 'g711u',
      dtmf: {
        method: 'rfc2833' as const,
        payloadType: 101,
        duration: 100,
        volume: -10,
        interDigitDelay: 50,
        sipInfoContentType: 'application/dtmf-relay',
        inbandFrequencies: {
          lowFreq: [697, 770, 852, 941],
          highFreq: [1209, 1336, 1477, 1633]
        },
        redundancy: 3,
        endOfEvent: true
      },
      clearChannelConfig: {
        enabled: false,
        dataRate: 64000,
        protocol: 'v110' as const
      }
    };

    // Generate time slots (exclude D-channels)
    const generateTimeSlots = (layer1: 'e1' | 't1', dChannels: number[]) => {
      const maxSlots = layer1 === 'e1' ? 31 : 24;
      return Array.from({length: maxSlots}, (_, i) => i + 1)
        .filter(slot => !dChannels.includes(slot));
    };

    const config: GatewayConfig = {
      tdmoe: {
        interface: 'eth0',
        channels: primaryResult.layer1 === 'e1' ? 30 : 23
      },
      e1: primaryResult.layer1 === 'e1' ? {
        interface: primaryResult.interface,
        framing: primaryResult.framing as any,
        lineCode: primaryResult.lineCode as any,
        clockSource: primaryResult.clockSource as any,
        timeSlots: generateTimeSlots('e1', primaryResult.dChannels),
        channelAssociated: false
      } : {
        interface: 'span1',
        framing: 'crc4',
        lineCode: 'hdb3',
        clockSource: 'external',
        timeSlots: [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31],
        channelAssociated: false
      },
      t1: primaryResult.layer1 === 't1' ? {
        interface: primaryResult.interface,
        framing: primaryResult.framing as any,
        lineCode: primaryResult.lineCode as any,
        clockSource: primaryResult.clockSource as any,
        timeSlots: generateTimeSlots('t1', primaryResult.dChannels),
        channelAssociated: false
      } : {
        interface: 'span1',
        framing: 'esf',
        lineCode: 'b8zs',
        clockSource: 'external',
        timeSlots: [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24],
        channelAssociated: false
      },
      sip: {
        listenPort: 5060,
        domain: 'redfire-gateway.local',
        transport: 'udp'
      },
      rtp: {
        portRange: {
          min: 10000,
          max: 20000
        }
      },
      pri: {
        variant: primaryResult.variant as any,
        layer1: primaryResult.layer1,
        timeSlots: generateTimeSlots(primaryResult.layer1, primaryResult.dChannels),
        switchType: primaryResult.switchType,
        networkSpecific: primaryResult.vendor !== 'Generic',
        pointToPoint: false
      },
      sigtran: {
        enabled: primaryResult.protocol === 'ss7',
        pointCodes: {
          local: 1,
          remote: 2
        },
        variant: primaryResult.variant as any
      },
      freetdm: {
        enabled: false,
        configFile: '/etc/freetdm.conf',
        spans: [{
          spanId: primaryResult.spanId,
          name: `span${primaryResult.spanId}`,
          trunk_type: primaryResult.layer1,
          d_channel: primaryResult.dChannels[0],
          channels: generateTimeSlots(primaryResult.layer1, primaryResult.dChannels)
            .map(id => ({
              id,
              type: 'bchan' as const,
              enabled: true,
              signaling: primaryResult.protocol
            }))
            .concat(primaryResult.dChannels.map(id => ({
              id,
              type: 'dchan' as const,
              enabled: true,
              signaling: primaryResult.protocol
            })))
        }]
      },
      trunk: {
        type: 'voice',
        signaling: primaryResult.protocol as any,
        codec: codecConfig
      },
      nfas: primaryResult.nfasGroup ? {
        enabled: true,
        groups: [{
          groupId: 1,
          primarySpan: primaryResult.nfasGroup.primarySpan,
          backupSpans: primaryResult.nfasGroup.backupSpans,
          loadBalancing: false,
          ces: 1
        }],
        switchoverTimeout: 5000,
        heartbeatInterval: 30000,
        maxSwitchoverAttempts: 3
      } : {
        enabled: false,
        groups: [],
        switchoverTimeout: 5000,
        heartbeatInterval: 30000,
        maxSwitchoverAttempts: 3
      }
    };

    return config;
  }

  async saveGeneratedConfig(config: GatewayConfig, filename?: string): Promise<string> {
    const configDir = path.join(process.cwd(), 'config');
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const configFile = filename || `auto-detected-${timestamp}.json`;
    const fullPath = path.join(configDir, configFile);

    // Ensure config directory exists
    await fs.mkdir(configDir, { recursive: true });

    // Add metadata comments
    const configWithMetadata = {
      ...config,
      _metadata: {
        generatedBy: 'Redfire Gateway Auto-Detection',
        generatedAt: new Date().toISOString(),
        detectionResults: Array.from(this.detectionResults.values()),
        note: 'This configuration was automatically generated. Please review and adjust as needed.'
      }
    };

    await fs.writeFile(fullPath, JSON.stringify(configWithMetadata, null, 2));
    
    this.logger.info('Configuration saved', { file: fullPath });
    return fullPath;
  }

  // Helper methods for frame analysis
  private async probePhysicalLayer(interfaceName: string): Promise<any> {
    // Simulate hardware probing
    return {
      frames: Buffer.alloc(1000), // Simulated frame data
      timing: {
        jitter: Math.random() * 0.1,
        stability: 0.95 + Math.random() * 0.05
      }
    };
  }

  private analyzeFrameStructure(frames: Buffer): any {
    // Simplified frame analysis
    return {
      bitsPerFrame: 256, // E1 example
      framesPerSecond: 8000,
      crcBits: 4,
      violations: 10,
      esfPattern: false
    };
  }

  private analyzeClockTiming(timing: any): any {
    return timing;
  }

  private async readChannelData(interfaceName: string, channel: number, durationMs: number): Promise<Buffer> {
    // Simulate reading channel data
    await new Promise(resolve => setTimeout(resolve, Math.min(durationMs, 100)));
    return Buffer.alloc(durationMs / 10); // Simulated data
  }

  private findLAPDFrames(data: Buffer): Buffer[] {
    const frames: Buffer[] = [];
    // Simplified LAPD frame detection
    for (let i = 0; i < data.length - 4; i++) {
      if (data[i] === 0x7E) { // Flag pattern
        frames.push(data.slice(i, i + 32)); // Assume 32-byte frame
      }
    }
    return frames;
  }

  private findQ931Messages(lapdFrames: Buffer[]): Buffer[] {
    const messages: Buffer[] = [];
    for (const frame of lapdFrames) {
      if (frame.length > 8 && frame[6] === 0x08) { // Q.931 protocol discriminator
        messages.push(frame.slice(6)); // Extract Q.931 message
      }
    }
    return messages;
  }

  private matchesPattern(message: Buffer, pattern: Buffer): boolean {
    if (message.length < pattern.length) return false;
    
    for (let i = 0; i < pattern.length; i++) {
      if (message[i] !== pattern[i]) return false;
    }
    return true;
  }

  private analyzeInformationElements(messages: Buffer[]): any {
    const features: string[] = [];
    // Analyze IE patterns to detect vendor-specific features
    // This is a simplified implementation
    return { features };
  }

  private detectProprietaryFeatures(messages: Buffer[]): string[] {
    const features: string[] = [];
    // Detect proprietary information elements and features
    return features;
  }

  private extractCESFromFrame(frame: Buffer): number | null {
    // Extract Connection Endpoint Suffix from LAPD frame
    // This is protocol-specific and would require detailed implementation
    return null;
  }

  private extractSpanId(interfaceName: string): number {
    const match = interfaceName.match(/span(\d+)/);
    return match ? parseInt(match[1]) : 1;
  }

  // Public API
  getDetectionResults(): DetectionResult[] {
    return Array.from(this.detectionResults.values());
  }

  getDetectionResult(spanId: number): DetectionResult | undefined {
    return this.detectionResults.get(spanId);
  }

  isDetectionRunning(): boolean {
    return this.isRunning;
  }

  async runQuickDetection(interfaceName: string): Promise<DetectionResult | null> {
    this.logger.info(`Running quick detection on ${interfaceName}`);
    
    // Abbreviated detection for faster results
    this.detectionTimeout = 10000; // 10 seconds
    
    try {
      const result = await this.detectInterface(interfaceName);
      return result;
    } finally {
      this.detectionTimeout = 30000; // Reset to default
    }
  }
}