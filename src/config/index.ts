import { GatewayConfig } from '../types';

export const defaultConfig: GatewayConfig = {
  tdmoe: {
    interface: 'eth0',
    channels: 24
  },
  e1: {
    interface: 'span1',
    framing: 'crc4',
    lineCode: 'hdb3',
    clockSource: 'external',
    timeSlots: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31],
    channelAssociated: false
  },
  t1: {
    interface: 'span1',
    framing: 'esf',
    lineCode: 'b8zs',
    clockSource: 'external',
    timeSlots: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24],
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
    variant: 'etsi',
    layer1: 'e1',
    timeSlots: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31],
    switchType: 'euroISDN',
    networkSpecific: false,
    pointToPoint: false
  },
  sigtran: {
    enabled: true,
    pointCodes: {
      local: 1,
      remote: 2
    },
    variant: 'itu'
  },
  freetdm: {
    enabled: false,
    configFile: '/etc/freetdm.conf',
    spans: [
      {
        spanId: 1,
        name: 'span1',
        trunk_type: 'e1',
        d_channel: 16,
        channels: [
          { id: 1, type: 'bchan', enabled: true, signaling: 'pri' },
          { id: 2, type: 'bchan', enabled: true, signaling: 'pri' },
          { id: 16, type: 'dchan', enabled: true, signaling: 'pri' }
        ]
      }
    ]
  },
  trunk: {
    type: 'voice',
    signaling: 'pri',
    codec: {
      allowedCodecs: ['g711a'], // Default to G.711 A-law for E1
      preferredCodec: 'g711a',
      dtmf: {
        method: 'rfc2833',
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
        protocol: 'v110'
      }
    }
  },
  nfas: {
    enabled: false,
    groups: [],
    switchoverTimeout: 5000,
    heartbeatInterval: 30000,
    maxSwitchoverAttempts: 3
  }
};

export class ConfigManager {
  private config: GatewayConfig;

  constructor(config?: Partial<GatewayConfig>) {
    this.config = { ...defaultConfig, ...config };
  }

  getConfig(): GatewayConfig {
    return { ...this.config };
  }

  updateConfig(updates: Partial<GatewayConfig>): void {
    this.config = { ...this.config, ...updates };
  }

  getTDMoEConfig() {
    return this.config.tdmoe;
  }

  getSIPConfig() {
    return this.config.sip;
  }

  getRTPConfig() {
    return this.config.rtp;
  }

  getPRIConfig() {
    return this.config.pri;
  }

  getSigtranConfig() {
    return this.config.sigtran;
  }
}