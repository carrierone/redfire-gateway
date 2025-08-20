import { EventEmitter } from 'events';
import { Logger } from '../utils/logger';

export interface MobileNetworkConfig {
  enabled: boolean;
  networkType: '3g' | '4g' | 'volte' | 'vowifi';
  msc: {
    address: string;
    port: number;
    protocol: 'sip' | 'diameter' | 'map';
  };
  codec: {
    amr: {
      enabled: boolean;
      modes: ('4.75' | '5.15' | '5.90' | '6.70' | '7.40' | '7.95' | '10.2' | '12.2')[];
      modeSet: number;
      octetAlign: boolean;
      robustSorting: boolean;
    };
    amrWb: {
      enabled: boolean;
      modes: ('6.60' | '8.85' | '12.65' | '14.25' | '15.85' | '18.25' | '19.85' | '23.05' | '23.85')[];
      modeSet: number;
      octetAlign: boolean;
    };
    evs: {
      enabled: boolean;
      primaryMode: number;
      modes: string[];
      vbr: boolean;
    };
  };
  qos: {
    enabled: boolean;
    conversationalClass: number;
    maxBitrate: number;
    guaranteedBitrate: number;
    transferDelay: number;
    trafficHandlingPriority: number;
  };
  emergencyCall: {
    enabled: boolean;
    emergencyNumbers: string[];
    locationInfo: boolean;
  };
}

export interface MobileCallContext {
  callId: string;
  imsi?: string;
  msisdn: string;
  locationAreaCode?: number;
  cellId?: number;
  networkType: '3g' | '4g' | 'volte' | 'vowifi';
  qosParameters?: QoSParameters;
  emergencyCall: boolean;
  roaming: boolean;
}

export interface QoSParameters {
  conversationalClass: number;
  maxBitrateUplink: number;
  maxBitrateDownlink: number;
  guaranteedBitrateUplink: number;
  guaranteedBitrateDownlink: number;
  transferDelay: number;
  trafficClass: number;
  handlingPriority: number;
  allocationRetention: number;
}

export interface AMRFrame {
  frameType: number;
  mode: number;
  quality: boolean;
  payload: Buffer;
}

export interface EVSFrame {
  frameType: number;
  cmr: number; // Codec Mode Request
  toc: number; // Table of Contents
  payload: Buffer;
}

export class MobileInterworkingHandler extends EventEmitter {
  private config: MobileNetworkConfig;
  private logger: Logger;
  private activeCalls: Map<string, MobileCallContext> = new Map();
  private codecTranscoder: MobileCodecTranscoder;

  constructor(config: MobileNetworkConfig, logger: Logger) {
    super();
    this.config = config;
    this.logger = logger.child({ component: 'mobile-interworking' });
    this.codecTranscoder = new MobileCodecTranscoder(config.codec, logger);
  }

  // Handle inbound mobile call
  async handleInboundMobileCall(context: MobileCallContext, sdp: string): Promise<string> {
    this.logger.info('Handling inbound mobile call', {
      callId: context.callId,
      msisdn: context.msisdn,
      networkType: context.networkType,
      emergencyCall: context.emergencyCall
    });

    this.activeCalls.set(context.callId, context);

    // Parse mobile SDP and adapt for TDM
    const adaptedSDP = this.adaptMobileSDPForTDM(sdp, context);

    // Handle emergency calls
    if (context.emergencyCall) {
      await this.handleEmergencyCall(context);
    }

    // Apply QoS if configured
    if (this.config.qos.enabled && context.qosParameters) {
      this.applyQoSParameters(context);
    }

    this.emit('mobileCallSetup', {
      context,
      adaptedSDP
    });

    return adaptedSDP;
  }

  // Handle outbound call to mobile network
  async handleOutboundToMobile(callId: string, tdmSDP: string, destinationMSISDN: string): Promise<string> {
    this.logger.info('Handling outbound call to mobile', {
      callId,
      destinationMSISDN
    });

    // Create mobile call context
    const context: MobileCallContext = {
      callId,
      msisdn: destinationMSISDN,
      networkType: this.config.networkType,
      emergencyCall: this.isEmergencyNumber(destinationMSISDN),
      roaming: false
    };

    this.activeCalls.set(callId, context);

    // Adapt TDM SDP for mobile network
    const mobileSDP = this.adaptTDMSDPForMobile(tdmSDP, context);

    this.emit('mobileCallOrigination', {
      context,
      mobileSDP
    });

    return mobileSDP;
  }

  // Adapt mobile SDP for TDM interworking
  private adaptMobileSDPForTDM(mobileSDP: string, context: MobileCallContext): string {
    const lines = mobileSDP.split('\r\n');
    const adaptedLines: string[] = [];
    let hasAMR = false;
    let hasEVS = false;

    for (const line of lines) {
      if (line.startsWith('m=audio')) {
        // Replace mobile codecs with G.711
        const parts = line.split(' ');
        const port = parts[1];
        adaptedLines.push(`m=audio ${port} RTP/AVP 8 0 101`); // PCMA, PCMU, telephone-event
      } else if (line.startsWith('a=rtpmap:')) {
        // Handle codec mappings
        if (line.includes('AMR') || line.includes('AMR-WB')) {
          hasAMR = true;
          // Skip AMR, will be transcoded
          continue;
        } else if (line.includes('EVS')) {
          hasEVS = true;
          // Skip EVS, will be transcoded
          continue;
        } else if (line.includes('PCMA') || line.includes('PCMU') || line.includes('telephone-event')) {
          adaptedLines.push(line);
        }
      } else if (line.startsWith('a=fmtp:')) {
        // Handle format parameters
        if (line.includes('AMR') || line.includes('EVS')) {
          // Skip mobile codec parameters
          continue;
        } else {
          adaptedLines.push(line);
        }
      } else {
        adaptedLines.push(line);
      }
    }

    // Add standard G.711 mappings if not present
    if (!adaptedLines.some(line => line.includes('rtpmap:8'))) {
      adaptedLines.push('a=rtpmap:8 PCMA/8000');
    }
    if (!adaptedLines.some(line => line.includes('rtpmap:0'))) {
      adaptedLines.push('a=rtpmap:0 PCMU/8000');
    }
    if (!adaptedLines.some(line => line.includes('rtpmap:101'))) {
      adaptedLines.push('a=rtpmap:101 telephone-event/8000');
      adaptedLines.push('a=fmtp:101 0-15');
    }

    // Set up transcoding if mobile codecs detected
    if (hasAMR || hasEVS) {
      this.setupTranscoding(context.callId, hasAMR, hasEVS);
    }

    return adaptedLines.join('\r\n');
  }

  // Adapt TDM SDP for mobile network
  private adaptTDMSDPForMobile(tdmSDP: string, context: MobileCallContext): string {
    const lines = tdmSDP.split('\r\n');
    const adaptedLines: string[] = [];
    let audioPortFound = false;

    for (const line of lines) {
      if (line.startsWith('m=audio') && !audioPortFound) {
        // Add mobile codecs
        const parts = line.split(' ');
        const port = parts[1];
        let codecs = '';

        // Add AMR if enabled
        if (this.config.codec.amr.enabled) {
          codecs += ' 96'; // AMR
        }

        // Add AMR-WB if enabled
        if (this.config.codec.amrWb.enabled) {
          codecs += ' 97'; // AMR-WB
        }

        // Add EVS if enabled
        if (this.config.codec.evs.enabled) {
          codecs += ' 98'; // EVS
        }

        // Add G.711 as fallback
        codecs += ' 8 0'; // PCMA, PCMU

        // Add telephone-event
        codecs += ' 101';

        adaptedLines.push(`m=audio ${port} RTP/AVP${codecs}`);
        audioPortFound = true;
      } else if (line.startsWith('a=rtpmap:')) {
        // Keep existing mappings and add mobile codecs
        adaptedLines.push(line);
      } else {
        adaptedLines.push(line);
      }
    }

    // Add mobile codec mappings
    if (this.config.codec.amr.enabled) {
      adaptedLines.push('a=rtpmap:96 AMR/8000');
      adaptedLines.push(this.buildAMRFmtp());
    }

    if (this.config.codec.amrWb.enabled) {
      adaptedLines.push('a=rtpmap:97 AMR-WB/16000');
      adaptedLines.push(this.buildAMRWBFmtp());
    }

    if (this.config.codec.evs.enabled) {
      adaptedLines.push('a=rtpmap:98 EVS/16000');
      adaptedLines.push(this.buildEVSFmtp());
    }

    // Add QoS attributes for mobile
    if (this.config.qos.enabled) {
      adaptedLines.push(`a=curr:qos local none`);
      adaptedLines.push(`a=curr:qos remote none`);
      adaptedLines.push(`a=des:qos mandatory local sendrecv`);
      adaptedLines.push(`a=des:qos mandatory remote sendrecv`);
    }

    return adaptedLines.join('\r\n');
  }

  // Build AMR fmtp line
  private buildAMRFmtp(): string {
    const modes = this.config.codec.amr.modes.join(',');
    let fmtp = `a=fmtp:96 mode-set=${this.config.codec.amr.modeSet}`;
    
    if (this.config.codec.amr.octetAlign) {
      fmtp += ';octet-align=1';
    }
    
    if (this.config.codec.amr.robustSorting) {
      fmtp += ';robust-sorting=1';
    }
    
    return fmtp;
  }

  // Build AMR-WB fmtp line
  private buildAMRWBFmtp(): string {
    let fmtp = `a=fmtp:97 mode-set=${this.config.codec.amrWb.modeSet}`;
    
    if (this.config.codec.amrWb.octetAlign) {
      fmtp += ';octet-align=1';
    }
    
    return fmtp;
  }

  // Build EVS fmtp line
  private buildEVSFmtp(): string {
    const modes = this.config.codec.evs.modes.join('-');
    let fmtp = `a=fmtp:98 br=${modes}`;
    
    if (this.config.codec.evs.vbr) {
      fmtp += ';vbr=1';
    }
    
    return fmtp;
  }

  // Setup transcoding between mobile and TDM codecs
  private setupTranscoding(callId: string, hasAMR: boolean, hasEVS: boolean): void {
    this.logger.debug('Setting up transcoding', {
      callId,
      hasAMR,
      hasEVS
    });

    const transcodingConfig = {
      inputCodecs: [],
      outputCodecs: ['g711a', 'g711u']
    };

    if (hasAMR) {
      transcodingConfig.inputCodecs.push('amr', 'amr-wb');
    }

    if (hasEVS) {
      transcodingConfig.inputCodecs.push('evs');
    }

    this.emit('setupTranscoding', {
      callId,
      config: transcodingConfig
    });
  }

  // Handle emergency calls
  private async handleEmergencyCall(context: MobileCallContext): Promise<void> {
    this.logger.warn('Emergency call detected', {
      callId: context.callId,
      msisdn: context.msisdn,
      networkType: context.networkType
    });

    // Priority handling for emergency calls
    context.qosParameters = {
      conversationalClass: 1, // Highest priority
      maxBitrateUplink: 64000,
      maxBitrateDownlink: 64000,
      guaranteedBitrateUplink: 64000,
      guaranteedBitrateDownlink: 64000,
      transferDelay: 100, // Low delay
      trafficClass: 1, // Conversational
      handlingPriority: 1, // Highest
      allocationRetention: 1 // Highest retention
    };

    this.emit('emergencyCall', {
      context,
      priority: 'highest'
    });
  }

  // Apply QoS parameters
  private applyQoSParameters(context: MobileCallContext): void {
    if (!context.qosParameters) return;

    this.logger.debug('Applying QoS parameters', {
      callId: context.callId,
      qos: context.qosParameters
    });

    this.emit('qosApplication', {
      callId: context.callId,
      qos: context.qosParameters
    });
  }

  // Check if number is emergency
  private isEmergencyNumber(msisdn: string): boolean {
    return this.config.emergencyCall.emergencyNumbers.includes(msisdn) ||
           ['112', '911', '999', '000'].includes(msisdn);
  }

  // Process mobile codec frames
  processAMRFrame(callId: string, frame: AMRFrame): Buffer {
    return this.codecTranscoder.transcodeAMRToG711(frame);
  }

  processEVSFrame(callId: string, frame: EVSFrame): Buffer {
    return this.codecTranscoder.transcodeEVSToG711(frame);
  }

  // Handle handover scenarios
  handleHandover(callId: string, newNetworkType: '3g' | '4g' | 'volte' | 'vowifi'): void {
    const context = this.activeCalls.get(callId);
    if (!context) return;

    this.logger.info('Handling network handover', {
      callId,
      from: context.networkType,
      to: newNetworkType
    });

    context.networkType = newNetworkType;

    // Adjust codecs and QoS for new network
    this.emit('handover', {
      callId,
      previousNetwork: context.networkType,
      newNetwork: newNetworkType,
      context
    });
  }

  // Update location information
  updateLocationInfo(callId: string, locationAreaCode: number, cellId: number): void {
    const context = this.activeCalls.get(callId);
    if (!context) return;

    context.locationAreaCode = locationAreaCode;
    context.cellId = cellId;

    this.emit('locationUpdate', {
      callId,
      locationAreaCode,
      cellId
    });
  }

  // Get call context
  getCallContext(callId: string): MobileCallContext | undefined {
    return this.activeCalls.get(callId);
  }

  // Clear call
  clearCall(callId: string): void {
    this.activeCalls.delete(callId);
    this.codecTranscoder.clearSession(callId);
  }

  // Get configuration
  getConfig(): MobileNetworkConfig {
    return { ...this.config };
  }

  // Update configuration
  updateConfig(config: Partial<MobileNetworkConfig>): void {
    Object.assign(this.config, config);
    this.logger.info('Mobile interworking configuration updated');
  }
}

// Mobile codec transcoder
class MobileCodecTranscoder {
  private logger: Logger;
  private config: any;

  constructor(config: any, logger: Logger) {
    this.config = config;
    this.logger = logger.child({ component: 'mobile-transcoder' });
  }

  transcodeAMRToG711(frame: AMRFrame): Buffer {
    // Simplified transcoding - in reality this would use a proper codec library
    this.logger.trace('Transcoding AMR to G.711', {
      frameType: frame.frameType,
      mode: frame.mode,
      quality: frame.quality
    });

    // Convert AMR frame to G.711 PCM samples
    // This is a placeholder - real implementation would use libamr or similar
    const g711Samples = this.amrToG711Conversion(frame.payload);
    
    return g711Samples;
  }

  transcodeEVSToG711(frame: EVSFrame): Buffer {
    // Simplified transcoding for EVS
    this.logger.trace('Transcoding EVS to G.711', {
      frameType: frame.frameType,
      cmr: frame.cmr,
      toc: frame.toc
    });

    // Convert EVS frame to G.711 PCM samples
    const g711Samples = this.evsToG711Conversion(frame.payload);
    
    return g711Samples;
  }

  transcodeG711ToAMR(g711Data: Buffer, mode: number): AMRFrame {
    // Convert G.711 to AMR
    const amrPayload = this.g711ToAmrConversion(g711Data, mode);
    
    return {
      frameType: 0,
      mode,
      quality: true,
      payload: amrPayload
    };
  }

  transcodeG711ToEVS(g711Data: Buffer, mode: number): EVSFrame {
    // Convert G.711 to EVS
    const evsPayload = this.g711ToEvsConversion(g711Data, mode);
    
    return {
      frameType: 0,
      cmr: mode,
      toc: 0,
      payload: evsPayload
    };
  }

  // Placeholder conversion methods - would use real codec libraries
  private amrToG711Conversion(amrData: Buffer): Buffer {
    // This would use libamr or similar codec library
    // For now, return silence (G.711 A-law silence = 0xD5)
    return Buffer.alloc(160, 0xD5);
  }

  private evsToG711Conversion(evsData: Buffer): Buffer {
    // This would use EVS codec library
    // For now, return silence
    return Buffer.alloc(160, 0xD5);
  }

  private g711ToAmrConversion(g711Data: Buffer, mode: number): Buffer {
    // This would encode G.711 PCM to AMR
    // Return placeholder AMR frame
    return Buffer.alloc(32, 0);
  }

  private g711ToEvsConversion(g711Data: Buffer, mode: number): Buffer {
    // This would encode G.711 PCM to EVS
    // Return placeholder EVS frame
    return Buffer.alloc(48, 0);
  }

  clearSession(callId: string): void {
    // Clear any session-specific transcoding state
    this.logger.debug('Clearing transcoding session', { callId });
  }
}