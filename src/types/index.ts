export interface TDMoEFrame {
  channel: number;
  data: Buffer;
  timestamp: number;
}

export interface SIPMessage {
  method: string;
  uri: string;
  headers: Record<string, string>;
  body?: string;
}

export interface RTPPacket {
  version: number;
  padding: boolean;
  extension: boolean;
  csrcCount: number;
  marker: boolean;
  payloadType: number;
  sequenceNumber: number;
  timestamp: number;
  ssrc: number;
  payload: Buffer;
}

export interface PRIChannel {
  id: number;
  state: 'idle' | 'busy' | 'ringing' | 'connected';
  direction: 'inbound' | 'outbound';
}

export interface ISUPMessage {
  messageType: number;
  cic: number;
  parameters: Buffer;
}

export interface CallSession {
  id: string;
  tdmChannel?: number;
  sipCallId?: string;
  rtpSession?: RTPSession;
  state: 'setup' | 'proceeding' | 'alerting' | 'connected' | 'disconnect';
  direction: 'inbound' | 'outbound';
  startTime: Date;
}

export interface RTPSession {
  localPort: number;
  remoteAddress: string;
  remotePort: number;
  payloadType: number;
}

export interface E1Config {
  interface: string;
  framing: 'crc4' | 'no-crc4' | 'cas';
  lineCode: 'hdb3' | 'ami';
  clockSource: 'internal' | 'external' | 'recovered';
  timeSlots: number[];
  channelAssociated: boolean;
}

export interface ETSIPRIConfig {
  variant: 'etsi' | 'ni2' | 'euro' | 'japan' | 'ansi' | '5ess' | 'dms100';
  layer1: 'e1' | 't1';
  timeSlots: number[];
  switchType: string;
  networkSpecific: boolean;
  pointToPoint: boolean;
}

export interface FreeTDMConfig {
  enabled: boolean;
  configFile: string;
  spans: FreeTDMSpan[];
}

export interface FreeTDMSpan {
  spanId: number;
  name: string;
  trunk_type: 'e1' | 't1' | 'fxo' | 'fxs';
  d_channel?: number;
  channels: FreeTDMChannel[];
}

export interface FreeTDMChannel {
  id: number;
  type: 'voice' | 'data' | 'dchan' | 'bchan';
  enabled: boolean;
  signaling?: string;
}

export interface DTMFConfig {
  method: 'rfc2833' | 'sip-info' | 'inband' | 'auto';
  payloadType: number;        // RFC2833 payload type (typically 101)
  duration: number;           // DTMF tone duration in ms
  volume: number;             // Volume level (-63 to 0 dBm0)
  interDigitDelay: number;    // Delay between digits in ms
  sipInfoContentType: string; // Content-Type for SIP INFO method
  inbandFrequencies: {        // Inband DTMF frequency matrix
    lowFreq: number[];        // Low frequency group [697, 770, 852, 941]
    highFreq: number[];       // High frequency group [1209, 1336, 1477, 1633]
  };
  redundancy: number;         // Number of redundant packets for RFC2833
  endOfEvent: boolean;        // Send end-of-event marker
}

export interface CodecConfig {
  allowedCodecs: ('g711u' | 'g711a' | 'clear-channel')[];
  preferredCodec: 'g711u' | 'g711a' | 'clear-channel';
  dtmf: DTMFConfig;
  clearChannelConfig?: {
    enabled: boolean;
    dataRate: number;
    protocol?: 'v110' | 'v120' | 'x75';
  };
}

export interface TrunkConfig {
  type: 'voice' | 'data' | 'mixed';
  signaling: 'cas' | 'pri' | 'r2' | 'sip';
  codec: CodecConfig;
}

export interface GatewayConfig {
  tdmoe: {
    interface: string;
    channels: number;
  };
  e1: E1Config;
  t1: {
    interface: string;
    framing: 'esf' | 'd4';
    lineCode: 'b8zs' | 'ami';
    clockSource: 'internal' | 'external' | 'recovered';
    timeSlots: number[];
    channelAssociated: boolean;
  };
  sip: {
    listenPort: number;
    domain: string;
    transport: 'udp' | 'tcp' | 'tls';
  };
  rtp: {
    portRange: {
      min: number;
      max: number;
    };
  };
  pri: ETSIPRIConfig;
  sigtran: {
    enabled: boolean;
    pointCodes: {
      local: number;
      remote: number;
    };
    variant: 'itu' | 'ansi' | 'china' | 'japan';
  };
  freetdm: FreeTDMConfig;
  trunk: TrunkConfig;
  nfas: {
    enabled: boolean;
    groups: NFASGroupConfig[];
    switchoverTimeout: number;
    heartbeatInterval: number;
    maxSwitchoverAttempts: number;
  };
}

export interface NFASGroupConfig {
  groupId: number;
  primarySpan: number;
  backupSpans: number[];
  loadBalancing: boolean;
  ces: number;
}