//! RTP (Real-time Transport Protocol) implementation

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use dashmap::DashMap;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, trace, warn};

use crate::config::PortRange;
use crate::{Error, Result};

/// RTP packet structure
#[derive(Debug, Clone)]
pub struct RtpPacket {
    pub version: u8,
    pub padding: bool,
    pub extension: bool,
    pub csrc_count: u8,
    pub marker: bool,
    pub payload_type: u8,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    pub csrc_list: Vec<u32>,
    pub payload: Bytes,
}

impl RtpPacket {
    pub fn new(payload_type: u8, sequence_number: u16, timestamp: u32, ssrc: u32) -> Self {
        Self {
            version: 2,
            padding: false,
            extension: false,
            csrc_count: 0,
            marker: false,
            payload_type,
            sequence_number,
            timestamp,
            ssrc,
            csrc_list: Vec::new(),
            payload: Bytes::new(),
        }
    }

    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(12 + (self.csrc_count as usize * 4) + self.payload.len());
        
        // First byte: V(2) + P(1) + X(1) + CC(4)
        let first_byte = (self.version << 6) 
            | (if self.padding { 1 << 5 } else { 0 })
            | (if self.extension { 1 << 4 } else { 0 })
            | self.csrc_count;
        buf.put_u8(first_byte);
        
        // Second byte: M(1) + PT(7)
        let second_byte = (if self.marker { 1 << 7 } else { 0 }) | self.payload_type;
        buf.put_u8(second_byte);
        
        buf.put_u16(self.sequence_number);
        buf.put_u32(self.timestamp);
        buf.put_u32(self.ssrc);
        
        // CSRC list
        for csrc in &self.csrc_list {
            buf.put_u32(*csrc);
        }
        
        // Payload
        buf.put(self.payload.clone());
        
        buf.freeze()
    }

    pub fn decode(mut data: Bytes) -> Result<Self> {
        if data.len() < 12 {
            return Err(Error::rtp("RTP packet too short"));
        }

        let first_byte = data.get_u8();
        let version = (first_byte >> 6) & 0x03;
        let padding = (first_byte & 0x20) != 0;
        let extension = (first_byte & 0x10) != 0;
        let csrc_count = first_byte & 0x0F;

        if version != 2 {
            return Err(Error::rtp("Invalid RTP version"));
        }

        let second_byte = data.get_u8();
        let marker = (second_byte & 0x80) != 0;
        let payload_type = second_byte & 0x7F;

        let sequence_number = data.get_u16();
        let timestamp = data.get_u32();
        let ssrc = data.get_u32();

        let mut csrc_list = Vec::new();
        for _ in 0..csrc_count {
            if data.remaining() < 4 {
                return Err(Error::rtp("Invalid CSRC list"));
            }
            csrc_list.push(data.get_u32());
        }

        // Handle extension header if present
        if extension {
            if data.remaining() < 4 {
                return Err(Error::rtp("Invalid extension header"));
            }
            let _extension_type = data.get_u16();
            let extension_length = data.get_u16() as usize * 4;
            
            if data.remaining() < extension_length {
                return Err(Error::rtp("Invalid extension length"));
            }
            
            // Skip extension data
            data.advance(extension_length);
        }

        // Handle padding if present
        let payload = if padding && !data.is_empty() {
            let padding_length = data[data.len() - 1] as usize;
            if padding_length > data.len() {
                return Err(Error::rtp("Invalid padding length"));
            }
            data.slice(0..data.len() - padding_length)
        } else {
            data
        };

        Ok(Self {
            version,
            padding,
            extension,
            csrc_count,
            marker,
            payload_type,
            sequence_number,
            timestamp,
            ssrc,
            csrc_list,
            payload,
        })
    }
}

/// RTCP packet types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtcpPacketType {
    SenderReport = 200,
    ReceiverReport = 201,
    SourceDescription = 202,
    Goodbye = 203,
    ApplicationDefined = 204,
}

/// RTP stream statistics
#[derive(Debug, Clone)]
pub struct RtpStreamStats {
    pub ssrc: u32,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_lost: u32,
    pub jitter: f64,
    pub last_sequence: u16,
    pub last_timestamp: u32,
    pub last_packet_time: Instant,
    pub first_packet_time: Option<Instant>,
}

impl RtpStreamStats {
    pub fn new(ssrc: u32) -> Self {
        Self {
            ssrc,
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            packets_lost: 0,
            jitter: 0.0,
            last_sequence: 0,
            last_timestamp: 0,
            last_packet_time: Instant::now(),
            first_packet_time: None,
        }
    }

    pub fn update_received(&mut self, packet: &RtpPacket) {
        self.packets_received += 1;
        self.bytes_received += packet.payload.len() as u64;
        
        let now = Instant::now();
        
        if self.first_packet_time.is_none() {
            self.first_packet_time = Some(now);
        }

        // Calculate packet loss (simplified)
        if self.packets_received > 1 {
            let expected_seq = self.last_sequence.wrapping_add(1);
            if packet.sequence_number != expected_seq {
                // Handle sequence number wrap-around
                let diff = if packet.sequence_number > expected_seq {
                    packet.sequence_number - expected_seq
                } else {
                    (u16::MAX - expected_seq) + packet.sequence_number + 1
                };
                self.packets_lost += diff as u32;
            }
        }

        // Calculate jitter (simplified RFC 3550 formula)
        if self.packets_received > 1 {
            let arrival_diff = now.duration_since(self.last_packet_time).as_millis() as f64;
            let timestamp_diff = if packet.timestamp > self.last_timestamp {
                packet.timestamp - self.last_timestamp
            } else {
                (u32::MAX - self.last_timestamp) + packet.timestamp + 1
            } as f64;
            
            let d = arrival_diff - (timestamp_diff / 8.0); // Assuming 8kHz sampling
            self.jitter += (d.abs() - self.jitter) / 16.0; // Low-pass filter
        }

        self.last_sequence = packet.sequence_number;
        self.last_timestamp = packet.timestamp;
        self.last_packet_time = now;
    }

    pub fn update_sent(&mut self, packet: &RtpPacket) {
        self.packets_sent += 1;
        self.bytes_sent += packet.payload.len() as u64;
    }

    pub fn packet_loss_rate(&self) -> f64 {
        if self.packets_received == 0 {
            0.0
        } else {
            (self.packets_lost as f64) / ((self.packets_received + self.packets_lost as u64) as f64) * 100.0
        }
    }
}

/// RTP session information
#[derive(Debug, Clone)]
pub struct RtpSession {
    pub id: String,
    pub local_port: u16,
    pub remote_addr: Option<SocketAddr>,
    pub ssrc: u32,
    pub payload_type: u8,
    pub sequence_number: Arc<RwLock<u16>>,
    pub timestamp_base: u32,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub stats: RtpStreamStats,
}

impl RtpSession {
    pub fn new(id: String, local_port: u16, payload_type: u8) -> Self {
        let ssrc = rand::random::<u32>();
        let timestamp_base = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as u32;

        Self {
            id,
            local_port,
            remote_addr: None,
            ssrc,
            payload_type,
            sequence_number: Arc::new(RwLock::new(rand::random::<u16>())),
            timestamp_base,
            created_at: Instant::now(),
            last_activity: Instant::now(),
            stats: RtpStreamStats::new(ssrc),
        }
    }

    pub async fn next_sequence_number(&self) -> u16 {
        let mut seq = self.sequence_number.write().await;
        *seq = seq.wrapping_add(1);
        *seq
    }

    pub fn update_activity(&mut self) {
        self.last_activity = Instant::now();
    }
}

/// RTP events
#[derive(Debug, Clone)]
pub enum RtpEvent {
    PacketReceived {
        session_id: String,
        packet: RtpPacket,
        source: SocketAddr,
    },
    SessionTimeout {
        session_id: String,
    },
    StreamStatistics {
        session_id: String,
        stats: RtpStreamStats,
    },
    Error {
        message: String,
        session_id: Option<String>,
    },
}

/// RTP handler implementation
pub struct RtpHandler {
    port_range: PortRange,
    sessions: Arc<DashMap<String, RtpSession>>,
    sockets: Arc<DashMap<u16, Arc<UdpSocket>>>,
    event_tx: mpsc::UnboundedSender<RtpEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<RtpEvent>>,
    next_port: Arc<RwLock<u16>>,
    is_running: bool,
}

impl RtpHandler {
    pub fn new(port_range: PortRange) -> Result<Self> {
        if port_range.min >= port_range.max {
            return Err(Error::parse("Invalid RTP port range"));
        }

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let min_port = port_range.min;
        Ok(Self {
            port_range,
            sessions: Arc::new(DashMap::new()),
            sockets: Arc::new(DashMap::new()),
            event_tx,
            event_rx: Some(event_rx),
            next_port: Arc::new(RwLock::new(min_port)),
            is_running: false,
        })
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<RtpEvent>> {
        self.event_rx.take()
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("Starting RTP handler");

        // Start session monitoring task
        let sessions_monitor = Arc::clone(&self.sessions);
        let event_tx_monitor = self.event_tx.clone();

        tokio::spawn(async move {
            Self::session_monitor_loop(sessions_monitor, event_tx_monitor).await;
        });

        // Start statistics reporting task
        let sessions_stats = Arc::clone(&self.sessions);
        let event_tx_stats = self.event_tx.clone();

        tokio::spawn(async move {
            Self::statistics_loop(sessions_stats, event_tx_stats).await;
        });

        self.is_running = true;
        info!("RTP handler started successfully");
        Ok(())
    }

    async fn session_monitor_loop(
        sessions: Arc<DashMap<String, RtpSession>>,
        event_tx: mpsc::UnboundedSender<RtpEvent>,
    ) {
        let mut monitor_interval = interval(Duration::from_secs(30));
        let timeout_duration = Duration::from_secs(300); // 5 minutes

        loop {
            monitor_interval.tick().await;
            let now = Instant::now();

            let timed_out_sessions: Vec<String> = sessions
                .iter()
                .filter(|entry| {
                    now.duration_since(entry.last_activity) > timeout_duration
                })
                .map(|entry| entry.id.clone())
                .collect();

            for session_id in timed_out_sessions {
                if let Some((_, _session)) = sessions.remove(&session_id) {
                    info!("RTP session timed out: {}", session_id);
                    let _ = event_tx.send(RtpEvent::SessionTimeout { session_id });
                }
            }
        }
    }

    async fn statistics_loop(
        sessions: Arc<DashMap<String, RtpSession>>,
        event_tx: mpsc::UnboundedSender<RtpEvent>,
    ) {
        let mut stats_interval = interval(Duration::from_secs(10));

        loop {
            stats_interval.tick().await;

            for session in sessions.iter() {
                let _ = event_tx.send(RtpEvent::StreamStatistics {
                    session_id: session.id.clone(),
                    stats: session.stats.clone(),
                });
            }
        }
    }

    async fn receive_loop(
        socket: Arc<UdpSocket>,
        port: u16,
        sessions: Arc<DashMap<String, RtpSession>>,
        event_tx: mpsc::UnboundedSender<RtpEvent>,
    ) {
        let mut buffer = vec![0u8; 2048];

        loop {
            match socket.recv_from(&mut buffer).await {
                Ok((size, source)) => {
                    let data = Bytes::copy_from_slice(&buffer[..size]);
                    
                    match RtpPacket::decode(data) {
                        Ok(packet) => {
                            trace!("Received RTP packet: SSRC={}, PT={}, Seq={}, TS={}",
                                packet.ssrc, packet.payload_type, packet.sequence_number, packet.timestamp);

                            // Find session by port and update statistics
                            let mut found_session = false;
                            for mut session in sessions.iter_mut() {
                                if session.local_port == port {
                                    session.update_activity();
                                    session.stats.update_received(&packet);
                                    
                                    // Update remote address if not set
                                    if session.remote_addr.is_none() {
                                        session.remote_addr = Some(source);
                                    }

                                    let _ = event_tx.send(RtpEvent::PacketReceived {
                                        session_id: session.id.clone(),
                                        packet,
                                        source,
                                    });
                                    
                                    found_session = true;
                                    break;
                                }
                            }

                            if !found_session {
                                debug!("Received RTP packet for unknown session on port {}", port);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to decode RTP packet from {}: {}", source, e);
                        }
                    }
                }
                Err(e) => {
                    error!("RTP receive error on port {}: {}", port, e);
                }
            }
        }
    }

    pub async fn create_session(&self, session_id: String, payload_type: u8) -> Result<RtpSession> {
        let port = self.allocate_port().await?;
        
        // Create and bind socket
        let bind_addr = format!("0.0.0.0:{}", port);
        let socket = UdpSocket::bind(&bind_addr).await
            .map_err(|e| Error::network(format!("Failed to bind RTP socket to {}: {}", bind_addr, e)))?;

        let socket = Arc::new(socket);
        self.sockets.insert(port, Arc::clone(&socket));

        // Start receiver task for this socket
        let socket_recv = Arc::clone(&socket);
        let sessions_recv = Arc::clone(&self.sessions);
        let event_tx_recv = self.event_tx.clone();

        tokio::spawn(async move {
            Self::receive_loop(socket_recv, port, sessions_recv, event_tx_recv).await;
        });

        let session = RtpSession::new(session_id.clone(), port, payload_type);
        self.sessions.insert(session_id, session.clone());

        info!("Created RTP session {} on port {}", session.id, port);
        Ok(session)
    }

    pub async fn send_packet(
        &self,
        session_id: &str,
        payload: Bytes,
        timestamp_offset: u32,
        marker: bool,
    ) -> Result<()> {
        let session = self.sessions.get(session_id)
            .ok_or_else(|| Error::rtp("RTP session not found"))?;

        let socket = self.sockets.get(&session.local_port)
            .ok_or_else(|| Error::rtp("RTP socket not found"))?;

        let remote_addr = session.remote_addr
            .ok_or_else(|| Error::rtp("Remote address not set"))?;

        let sequence_number = session.next_sequence_number().await;
        let timestamp = session.timestamp_base + timestamp_offset;

        let mut packet = RtpPacket::new(
            session.payload_type,
            sequence_number,
            timestamp,
            session.ssrc,
        );
        packet.marker = marker;
        packet.payload = payload;

        let encoded = packet.encode();
        socket.send_to(&encoded, remote_addr).await?;

        // Update statistics
        drop(session); // Release the reference to allow mutable access
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.update_activity();
            session.stats.update_sent(&packet);
        }

        trace!("Sent RTP packet: session={}, seq={}, ts={}, size={}",
            session_id, sequence_number, timestamp, encoded.len());

        Ok(())
    }

    pub async fn set_remote_address(&self, session_id: &str, remote_addr: SocketAddr) -> Result<()> {
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.remote_addr = Some(remote_addr);
            info!("Set remote address for RTP session {}: {}", session_id, remote_addr);
            Ok(())
        } else {
            Err(Error::rtp("RTP session not found"))
        }
    }

    pub fn get_session(&self, session_id: &str) -> Option<RtpSession> {
        self.sessions.get(session_id).map(|session| session.clone())
    }

    pub fn get_session_statistics(&self, session_id: &str) -> Option<RtpStreamStats> {
        self.sessions.get(session_id).map(|session| session.stats.clone())
    }

    pub fn get_all_sessions(&self) -> Vec<RtpSession> {
        self.sessions.iter().map(|entry| entry.value().clone()).collect()
    }

    pub async fn destroy_session(&self, session_id: &str) -> Result<()> {
        if let Some((_, session)) = self.sessions.remove(session_id) {
            // Remove and close socket
            if let Some((_, socket)) = self.sockets.remove(&session.local_port) {
                drop(socket); // Socket will be closed when dropped
            }

            info!("Destroyed RTP session: {}", session_id);
            Ok(())
        } else {
            Err(Error::rtp("RTP session not found"))
        }
    }

    async fn allocate_port(&self) -> Result<u16> {
        let mut next_port = self.next_port.write().await;
        let start_port = *next_port;

        loop {
            let port = *next_port;
            
            // Check if port is already in use
            if !self.sockets.contains_key(&port) {
                *next_port = if port >= self.port_range.max {
                    self.port_range.min
                } else {
                    port + 1
                };
                return Ok(port);
            }

            *next_port = if port >= self.port_range.max {
                self.port_range.min
            } else {
                port + 1
            };

            // Avoid infinite loop
            if *next_port == start_port {
                return Err(Error::rtp("No available RTP ports"));
            }
        }
    }

    pub fn get_active_session_count(&self) -> usize {
        self.sessions.len()
    }

    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping RTP handler");
        
        // Clear all sessions and sockets
        self.sessions.clear();
        self.sockets.clear();
        
        self.is_running = false;
        info!("RTP handler stopped");
        Ok(())
    }
}

/// DTMF tone generation for RFC 2833
pub struct DtmfGenerator {
    pub sample_rate: u32,
}

impl DtmfGenerator {
    pub fn new(sample_rate: u32) -> Self {
        Self { sample_rate }
    }

    pub fn generate_tone(&self, digit: char, duration_ms: u32) -> Vec<i16> {
        let (low_freq, high_freq) = match digit {
            '1' => (697.0, 1209.0),
            '2' => (697.0, 1336.0),
            '3' => (697.0, 1477.0),
            'A' => (697.0, 1633.0),
            '4' => (770.0, 1209.0),
            '5' => (770.0, 1336.0),
            '6' => (770.0, 1477.0),
            'B' => (770.0, 1633.0),
            '7' => (852.0, 1209.0),
            '8' => (852.0, 1336.0),
            '9' => (852.0, 1477.0),
            'C' => (852.0, 1633.0),
            '*' => (941.0, 1209.0),
            '0' => (941.0, 1336.0),
            '#' => (941.0, 1477.0),
            'D' => (941.0, 1633.0),
            _ => return Vec::new(),
        };

        let sample_count = (self.sample_rate as f64 * duration_ms as f64 / 1000.0) as usize;
        let mut samples = Vec::with_capacity(sample_count);

        for i in 0..sample_count {
            let t = i as f64 / self.sample_rate as f64;
            let sample = (2.0 * std::f64::consts::PI * low_freq * t).sin() +
                        (2.0 * std::f64::consts::PI * high_freq * t).sin();
            samples.push((sample * 16383.0) as i16); // Scale to 16-bit
        }

        samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtp_packet_encoding() {
        let payload = Bytes::from("test payload");
        let mut packet = RtpPacket::new(0, 12345, 67890, 0x12345678);
        packet.payload = payload.clone();
        
        let encoded = packet.encode();
        let decoded = RtpPacket::decode(encoded).unwrap();
        
        assert_eq!(decoded.payload_type, 0);
        assert_eq!(decoded.sequence_number, 12345);
        assert_eq!(decoded.timestamp, 67890);
        assert_eq!(decoded.ssrc, 0x12345678);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_rtp_packet_with_marker() {
        let mut packet = RtpPacket::new(8, 1, 160, 0x11111111);
        packet.marker = true;
        
        let encoded = packet.encode();
        let decoded = RtpPacket::decode(encoded).unwrap();
        
        assert!(decoded.marker);
        assert_eq!(decoded.payload_type, 8);
    }

    #[tokio::test]
    async fn test_rtp_handler_creation() {
        let port_range = PortRange { min: 10000, max: 10100 };
        let handler = RtpHandler::new(port_range);
        assert!(handler.is_ok());
    }

    #[test]
    fn test_dtmf_generation() {
        let generator = DtmfGenerator::new(8000);
        let samples = generator.generate_tone('1', 100);
        assert!(!samples.is_empty());
        assert_eq!(samples.len(), 800); // 100ms at 8kHz
    }
}