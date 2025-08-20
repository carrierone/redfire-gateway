import { EventEmitter } from 'events';
import { SIPMessage, CallSession } from '../types';

export class SIPHandler extends EventEmitter {
  private sessions: Map<string, CallSession> = new Map();
  private listenPort: number;
  private domain: string;
  private transport: 'udp' | 'tcp' | 'tls';
  private isRunning = false;

  constructor(port: number, domain: string, transport: 'udp' | 'tcp' | 'tls' = 'udp') {
    super();
    this.listenPort = port;
    this.domain = domain;
    this.transport = transport;
  }

  async start(): Promise<void> {
    if (this.isRunning) {
      throw new Error('SIP handler already running');
    }

    this.isRunning = true;
    this.emit('started');
  }

  async stop(): Promise<void> {
    if (!this.isRunning) {
      return;
    }

    this.isRunning = false;
    this.sessions.clear();
    this.emit('stopped');
  }

  createSession(callId: string, direction: 'inbound' | 'outbound'): CallSession {
    const session: CallSession = {
      id: callId,
      sipCallId: callId,
      state: 'setup',
      direction,
      startTime: new Date()
    };

    this.sessions.set(callId, session);
    this.emit('sessionCreated', session);
    return session;
  }

  updateSession(callId: string, updates: Partial<CallSession>): void {
    const session = this.sessions.get(callId);
    if (session) {
      Object.assign(session, updates);
      this.emit('sessionUpdated', session);
    }
  }

  terminateSession(callId: string): void {
    const session = this.sessions.get(callId);
    if (session) {
      session.state = 'disconnect';
      this.sessions.delete(callId);
      this.emit('sessionTerminated', session);
    }
  }

  sendInvite(toUri: string, fromUri: string, sdp?: string): string {
    const callId = this.generateCallId();
    const invite: SIPMessage = {
      method: 'INVITE',
      uri: toUri,
      headers: {
        'Call-ID': callId,
        'From': fromUri,
        'To': toUri,
        'Via': `SIP/2.0/${this.transport.toUpperCase()} ${this.domain}:${this.listenPort}`,
        'CSeq': '1 INVITE',
        'Content-Type': 'application/sdp'
      },
      body: sdp
    };

    this.createSession(callId, 'outbound');
    this.emit('messageOut', invite);
    return callId;
  }

  sendResponse(callId: string, statusCode: number, reasonPhrase: string, sdp?: string): void {
    const session = this.sessions.get(callId);
    if (!session) {
      throw new Error(`Session not found: ${callId}`);
    }

    const response: SIPMessage = {
      method: `${statusCode} ${reasonPhrase}`,
      uri: '',
      headers: {
        'Call-ID': callId,
        'CSeq': '1 INVITE'
      },
      body: sdp
    };

    if (statusCode >= 200 && statusCode < 300) {
      session.state = 'connected';
    } else if (statusCode >= 180 && statusCode < 200) {
      session.state = 'alerting';
    }

    this.emit('messageOut', response);
  }

  sendBye(callId: string): void {
    const session = this.sessions.get(callId);
    if (!session) {
      return;
    }

    const bye: SIPMessage = {
      method: 'BYE',
      uri: '',
      headers: {
        'Call-ID': callId,
        'CSeq': '2 BYE'
      }
    };

    this.terminateSession(callId);
    this.emit('messageOut', bye);
  }

  handleIncomingMessage(message: SIPMessage): void {
    const callId = message.headers['Call-ID'];
    
    if (message.method === 'INVITE') {
      this.handleInvite(message);
    } else if (message.method === 'BYE') {
      this.handleBye(callId);
    } else if (message.method.match(/^\d{3}/)) {
      this.handleResponse(message);
    }
  }

  private handleInvite(message: SIPMessage): void {
    const callId = message.headers['Call-ID'];
    const session = this.createSession(callId, 'inbound');
    
    if (message.body) {
      this.parseSDP(message.body, session);
    }
    
    this.emit('incomingCall', session, message);
  }

  private handleBye(callId: string): void {
    this.terminateSession(callId);
    this.sendResponse(callId, 200, 'OK');
  }

  private handleResponse(message: SIPMessage): void {
    const callId = message.headers['Call-ID'];
    const statusCode = parseInt(message.method.split(' ')[0]);
    
    if (statusCode >= 200 && statusCode < 300) {
      this.updateSession(callId, { state: 'connected' });
    } else if (statusCode >= 400) {
      this.terminateSession(callId);
    }
    
    this.emit('response', callId, statusCode, message);
  }

  private parseSDP(sdp: string, session: CallSession): void {
    const lines = sdp.split('\r\n');
    for (const line of lines) {
      if (line.startsWith('m=audio')) {
        const parts = line.split(' ');
        if (parts.length >= 2) {
          const port = parseInt(parts[1]);
          if (!isNaN(port)) {
            session.rtpSession = {
              localPort: 0,
              remoteAddress: '',
              remotePort: port,
              payloadType: this.getPreferredPayloadType(parts.slice(3))
            };
          }
        }
      }
    }
  }

  private getPreferredPayloadType(payloadTypes: string[]): number {
    // Check for supported codecs in order of preference
    for (const pt of payloadTypes) {
      const ptNum = parseInt(pt);
      if (ptNum === 0) return 0;  // G.711 μ-law (PCMU)
      if (ptNum === 8) return 8;  // G.711 A-law (PCMA)
    }
    return parseInt(payloadTypes[0]) || 0;
  }

  generateSDP(session: CallSession, trunkType: 'e1' | 't1', codecConfig: any): string {
    let codecs = '';
    let rtpmaps = '';
    
    // Determine codecs based on trunk type and configuration
    if (trunkType === 'e1') {
      // E1 gateways prefer G.711 A-law
      if (codecConfig.allowedCodecs.includes('g711a')) {
        codecs += ' 8';
        rtpmaps += 'a=rtpmap:8 PCMA/8000\r\n';
      }
      if (codecConfig.allowedCodecs.includes('g711u')) {
        codecs += ' 0';
        rtpmaps += 'a=rtpmap:0 PCMU/8000\r\n';
      }
    } else {
      // T1 gateways prefer G.711 μ-law
      if (codecConfig.allowedCodecs.includes('g711u')) {
        codecs += ' 0';
        rtpmaps += 'a=rtpmap:0 PCMU/8000\r\n';
      }
      if (codecConfig.allowedCodecs.includes('g711a')) {
        codecs += ' 8';
        rtpmaps += 'a=rtpmap:8 PCMA/8000\r\n';
      }
    }

    // Add clear channel support if configured
    if (codecConfig.allowedCodecs.includes('clear-channel') && 
        codecConfig.clearChannelConfig?.enabled) {
      codecs += ' 97';
      rtpmaps += `a=rtpmap:97 clearmode/8000\r\n`;
      rtpmaps += `a=fmtp:97 vbd=yes\r\n`;
    }

    // Add DTMF support
    codecs += ` ${codecConfig.dtmf.payloadType}`;
    rtpmaps += `a=rtpmap:${codecConfig.dtmf.payloadType} telephone-event/8000\r\n`;
    rtpmaps += `a=fmtp:${codecConfig.dtmf.payloadType} 0-15\r\n`;

    const port = session.rtpSession?.localPort || 10000;
    
    return `v=0\r\n` +
           `o=redfire ${Date.now()} ${Date.now()} IN IP4 127.0.0.1\r\n` +
           `s=Call\r\n` +
           `c=IN IP4 127.0.0.1\r\n` +
           `t=0 0\r\n` +
           `m=audio ${port} RTP/AVP${codecs}\r\n` +
           rtpmaps +
           `a=sendrecv\r\n`;
  }

  private generateCallId(): string {
    return `${Date.now()}-${Math.random().toString(36).substr(2, 9)}@${this.domain}`;
  }

  getSession(callId: string): CallSession | undefined {
    return this.sessions.get(callId);
  }

  getAllSessions(): CallSession[] {
    return Array.from(this.sessions.values());
  }
}