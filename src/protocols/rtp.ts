import { EventEmitter } from 'events';
import { createSocket, Socket } from 'dgram';
import { RTPPacket, RTPSession } from '../types';

export class RTPHandler extends EventEmitter {
  private sessions: Map<string, RTPSessionHandler> = new Map();
  private portRange: { min: number; max: number };
  private allocatedPorts: Set<number> = new Set();

  constructor(portRange: { min: number; max: number }) {
    super();
    this.portRange = portRange;
  }

  createSession(sessionId: string, localPort?: number): RTPSessionHandler {
    if (this.sessions.has(sessionId)) {
      throw new Error(`RTP session already exists: ${sessionId}`);
    }

    const port = localPort || this.allocatePort();
    if (!port) {
      throw new Error('No available ports for RTP session');
    }

    const session = new RTPSessionHandler(sessionId, port);
    this.sessions.set(sessionId, session);
    this.allocatedPorts.add(port);

    session.on('packet', (packet: RTPPacket) => {
      this.emit('packet', sessionId, packet);
    });

    session.on('closed', () => {
      this.sessions.delete(sessionId);
      this.allocatedPorts.delete(port);
    });

    return session;
  }

  getSession(sessionId: string): RTPSessionHandler | undefined {
    return this.sessions.get(sessionId);
  }

  terminateSession(sessionId: string): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.close();
    }
  }

  private allocatePort(): number | null {
    for (let port = this.portRange.min; port <= this.portRange.max; port += 2) {
      if (!this.allocatedPorts.has(port)) {
        return port;
      }
    }
    return null;
  }

  getAllSessions(): RTPSessionHandler[] {
    return Array.from(this.sessions.values());
  }
}

export class RTPSessionHandler extends EventEmitter {
  private socket: Socket;
  private sessionId: string;
  private localPort: number;
  private remoteAddress?: string;
  private remotePort?: number;
  private ssrc: number;
  private sequenceNumber = 0;
  private isActive = false;

  constructor(sessionId: string, localPort: number) {
    super();
    this.sessionId = sessionId;
    this.localPort = localPort;
    this.ssrc = Math.floor(Math.random() * 0xFFFFFFFF);
    this.socket = createSocket('udp4');
    
    this.setupSocket();
  }

  private setupSocket(): void {
    this.socket.on('message', (msg: Buffer, rinfo) => {
      try {
        const packet = this.parseRTPPacket(msg);
        this.emit('packet', packet);
      } catch (error) {
        this.emit('error', error);
      }
    });

    this.socket.on('error', (error) => {
      this.emit('error', error);
    });

    this.socket.bind(this.localPort);
  }

  setRemoteEndpoint(address: string, port: number): void {
    this.remoteAddress = address;
    this.remotePort = port;
  }

  sendPacket(payload: Buffer, payloadType: number, timestamp: number): void {
    if (!this.remoteAddress || !this.remotePort) {
      throw new Error('Remote endpoint not set');
    }

    const packet: RTPPacket = {
      version: 2,
      padding: false,
      extension: false,
      csrcCount: 0,
      marker: false,
      payloadType,
      sequenceNumber: this.sequenceNumber++,
      timestamp,
      ssrc: this.ssrc,
      payload
    };

    const buffer = this.buildRTPPacket(packet);
    this.socket.send(buffer, this.remotePort, this.remoteAddress);
  }

  private parseRTPPacket(buffer: Buffer): RTPPacket {
    if (buffer.length < 12) {
      throw new Error('Invalid RTP packet: too short');
    }

    const firstByte = buffer.readUInt8(0);
    const version = (firstByte >> 6) & 0x03;
    const padding = ((firstByte >> 5) & 0x01) === 1;
    const extension = ((firstByte >> 4) & 0x01) === 1;
    const csrcCount = firstByte & 0x0F;

    const secondByte = buffer.readUInt8(1);
    const marker = ((secondByte >> 7) & 0x01) === 1;
    const payloadType = secondByte & 0x7F;

    const sequenceNumber = buffer.readUInt16BE(2);
    const timestamp = buffer.readUInt32BE(4);
    const ssrc = buffer.readUInt32BE(8);

    const headerLength = 12 + (csrcCount * 4);
    const payload = buffer.slice(headerLength);

    return {
      version,
      padding,
      extension,
      csrcCount,
      marker,
      payloadType,
      sequenceNumber,
      timestamp,
      ssrc,
      payload
    };
  }

  private buildRTPPacket(packet: RTPPacket): Buffer {
    const headerLength = 12;
    const buffer = Buffer.allocUnsafe(headerLength + packet.payload.length);

    const firstByte = (packet.version << 6) | 
                     (packet.padding ? 0x20 : 0) |
                     (packet.extension ? 0x10 : 0) |
                     packet.csrcCount;
    buffer.writeUInt8(firstByte, 0);

    const secondByte = (packet.marker ? 0x80 : 0) | packet.payloadType;
    buffer.writeUInt8(secondByte, 1);

    buffer.writeUInt16BE(packet.sequenceNumber, 2);
    buffer.writeUInt32BE(packet.timestamp, 4);
    buffer.writeUInt32BE(packet.ssrc, 8);

    packet.payload.copy(buffer, headerLength);

    return buffer;
  }

  start(): void {
    this.isActive = true;
    this.emit('started');
  }

  stop(): void {
    this.isActive = false;
    this.emit('stopped');
  }

  close(): void {
    this.isActive = false;
    this.socket.close();
    this.emit('closed');
  }

  getLocalPort(): number {
    return this.localPort;
  }

  getRemoteEndpoint(): { address?: string; port?: number } {
    return {
      address: this.remoteAddress,
      port: this.remotePort
    };
  }

  isSessionActive(): boolean {
    return this.isActive;
  }
}