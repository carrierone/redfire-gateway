//! RTP media relay service for B2BUA
//! 
//! This module provides comprehensive RTP media relay functionality
//! including packet forwarding, codec transcoding, and media processing.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use dashmap::DashMap;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use crate::protocols::rtp::{RtpPacket, RtpSession, RtpHandler, RtpEvent};
use crate::services::transcoding::{TranscodingService, CodecType, TranscodingEvent};
use crate::{Error, Result};

/// Media relay session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaRelaySession {
    pub id: String,
    pub call_id: String,
    pub leg_a_session_id: String,
    pub leg_b_session_id: String,
    pub leg_a_endpoint: MediaEndpoint,
    pub leg_b_endpoint: MediaEndpoint,
    pub relay_mode: RelayMode,
    pub transcoding_session_id: Option<String>,
    #[serde(skip, default = "Instant::now")]
    pub created_at: Instant,
    #[serde(skip, default = "Instant::now")]
    pub last_activity: Instant,
    pub stats: MediaRelayStats,
}

/// Media endpoint information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaEndpoint {
    pub rtp_port: u16,
    pub rtcp_port: u16,
    pub remote_address: Option<SocketAddr>,
    pub codec: CodecType,
    pub ssrc: u32,
    pub payload_type: u8,
    #[serde(skip, default)]
    pub last_packet_time: Option<Instant>,
}

/// Media relay mode
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RelayMode {
    /// Direct relay without modification
    Transparent,
    /// Relay with codec transcoding
    Transcoding,
    /// Relay with media processing (echo cancellation, noise reduction, etc.)
    Processing,
    /// Record media for compliance/debugging
    Recording,
}

/// Media relay statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaRelayStats {
    pub packets_relayed_a_to_b: u64,
    pub packets_relayed_b_to_a: u64,
    pub bytes_relayed_a_to_b: u64,
    pub bytes_relayed_b_to_a: u64,
    pub packets_dropped: u64,
    pub transcoding_errors: u32,
    pub jitter_buffer_size: u32,
    pub average_latency_ms: f64,
    pub packet_loss_rate: f64,
    pub codec_a: CodecType,
    pub codec_b: CodecType,
}

impl MediaRelayStats {
    pub fn new(codec_a: CodecType, codec_b: CodecType) -> Self {
        Self {
            packets_relayed_a_to_b: 0,
            packets_relayed_b_to_a: 0,
            bytes_relayed_a_to_b: 0,
            bytes_relayed_b_to_a: 0,
            packets_dropped: 0,
            transcoding_errors: 0,
            jitter_buffer_size: 0,
            average_latency_ms: 0.0,
            packet_loss_rate: 0.0,
            codec_a,
            codec_b,
        }
    }

    pub fn total_packets(&self) -> u64 {
        self.packets_relayed_a_to_b + self.packets_relayed_b_to_a
    }

    pub fn total_bytes(&self) -> u64 {
        self.bytes_relayed_a_to_b + self.bytes_relayed_b_to_a
    }
}

/// Media processing configuration
#[derive(Debug, Clone)]
pub struct MediaProcessingConfig {
    pub enable_echo_cancellation: bool,
    pub enable_noise_reduction: bool,
    pub enable_automatic_gain_control: bool,
    pub enable_dtmf_detection: bool,
    pub enable_silence_detection: bool,
    pub jitter_buffer_size: u32,
    pub packet_loss_concealment: bool,
}

impl Default for MediaProcessingConfig {
    fn default() -> Self {
        Self {
            enable_echo_cancellation: false,
            enable_noise_reduction: false,
            enable_automatic_gain_control: false,
            enable_dtmf_detection: true,
            enable_silence_detection: true,
            jitter_buffer_size: 50,
            packet_loss_concealment: true,
        }
    }
}

/// Media relay events
#[derive(Debug, Clone)]
pub enum MediaRelayEvent {
    SessionStarted {
        session_id: String,
        call_id: String,
        leg_a_codec: CodecType,
        leg_b_codec: CodecType,
        relay_mode: RelayMode,
    },
    SessionEnded {
        session_id: String,
        stats: MediaRelayStats,
    },
    CodecMismatch {
        session_id: String,
        leg_a_codec: CodecType,
        leg_b_codec: CodecType,
        transcoding_enabled: bool,
    },
    PacketDropped {
        session_id: String,
        reason: String,
        direction: RelayDirection,
    },
    DtmfDetected {
        session_id: String,
        digit: char,
        duration_ms: u32,
        direction: RelayDirection,
    },
    SilenceDetected {
        session_id: String,
        duration_ms: u32,
        direction: RelayDirection,
    },
    QualityAlert {
        session_id: String,
        metric: String,
        value: f64,
        threshold: f64,
    },
    Error {
        session_id: Option<String>,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub enum RelayDirection {
    AToB,
    BToA,
}

/// Jitter buffer for packet reordering and delay compensation
#[derive(Debug)]
pub struct JitterBuffer {
    packets: HashMap<u16, (RtpPacket, Instant)>,
    expected_sequence: u16,
    max_size: usize,
    target_delay_ms: u64,
    created_at: Instant,
}

impl JitterBuffer {
    pub fn new(max_size: usize, target_delay_ms: u64) -> Self {
        Self {
            packets: HashMap::new(),
            expected_sequence: 0,
            max_size,
            target_delay_ms,
            created_at: Instant::now(),
        }
    }

    pub fn add_packet(&mut self, packet: RtpPacket) -> Vec<RtpPacket> {
        let arrival_time = Instant::now();
        let sequence = packet.sequence_number;

        // Add packet to buffer
        self.packets.insert(sequence, (packet, arrival_time));

        // Remove packets that are too old
        let max_age = Duration::from_millis(self.target_delay_ms * 2);
        self.packets.retain(|_, (_, time)| arrival_time.duration_since(*time) < max_age);

        // Limit buffer size
        if self.packets.len() > self.max_size {
            // Remove oldest packets
            let mut sorted: Vec<_> = self.packets.iter().map(|(k, v)| (*k, v.1)).collect();
            sorted.sort_by_key(|(_, time)| *time);
            let to_remove = sorted.len() - self.max_size;
            let keys_to_remove: Vec<u16> = sorted.iter().take(to_remove).map(|(seq, _)| *seq).collect();
            for seq in keys_to_remove {
                self.packets.remove(&seq);
            }
        }

        // Extract ready packets
        self.extract_ready_packets()
    }

    fn extract_ready_packets(&mut self) -> Vec<RtpPacket> {
        let mut ready_packets = Vec::new();
        let target_delay = Duration::from_millis(self.target_delay_ms);

        // Find packets that are ready to be played
        loop {
            if let Some((packet, arrival_time)) = self.packets.remove(&self.expected_sequence) {
                // Check if packet has been in buffer long enough
                if arrival_time.elapsed() >= target_delay {
                    ready_packets.push(packet);
                    self.expected_sequence = self.expected_sequence.wrapping_add(1);
                } else {
                    // Put packet back, not ready yet
                    self.packets.insert(self.expected_sequence, (packet, arrival_time));
                    break;
                }
            } else {
                break;
            }
        }

        ready_packets
    }

    pub fn get_buffer_size(&self) -> usize {
        self.packets.len()
    }
}

/// Media relay service
pub struct MediaRelayService {
    relay_sessions: Arc<DashMap<String, MediaRelaySession>>,
    jitter_buffers: Arc<DashMap<String, RwLock<JitterBuffer>>>,
    rtp_handler: Arc<RwLock<RtpHandler>>,
    transcoding_service: Arc<RwLock<TranscodingService>>,
    processing_config: MediaProcessingConfig,
    event_tx: mpsc::UnboundedSender<MediaRelayEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<MediaRelayEvent>>,
    rtp_event_rx: Option<mpsc::UnboundedReceiver<RtpEvent>>,
    transcoding_event_rx: Option<mpsc::UnboundedReceiver<TranscodingEvent>>,
    is_running: bool,
}

impl MediaRelayService {
    pub fn new(
        rtp_handler: Arc<RwLock<RtpHandler>>,
        transcoding_service: Arc<RwLock<TranscodingService>>,
        processing_config: MediaProcessingConfig,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Self {
            relay_sessions: Arc::new(DashMap::new()),
            jitter_buffers: Arc::new(DashMap::new()),
            rtp_handler,
            transcoding_service,
            processing_config,
            event_tx,
            event_rx: Some(event_rx),
            rtp_event_rx: None,
            transcoding_event_rx: None,
            is_running: false,
        }
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<MediaRelayEvent>> {
        self.event_rx.take()
    }

    pub fn set_rtp_event_receiver(&mut self, rx: mpsc::UnboundedReceiver<RtpEvent>) {
        self.rtp_event_rx = Some(rx);
    }

    pub fn set_transcoding_event_receiver(&mut self, rx: mpsc::UnboundedReceiver<TranscodingEvent>) {
        self.transcoding_event_rx = Some(rx);
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("Starting media relay service");

        // Start RTP event processing
        if let Some(rtp_rx) = self.rtp_event_rx.take() {
            let relay_sessions_rtp = Arc::clone(&self.relay_sessions);
            let jitter_buffers_rtp = Arc::clone(&self.jitter_buffers);
            let event_tx_rtp = self.event_tx.clone();
            let transcoding_service_rtp = Arc::clone(&self.transcoding_service);
            let processing_config_rtp = self.processing_config.clone();

            tokio::spawn(async move {
                Self::process_rtp_events(
                    rtp_rx,
                    relay_sessions_rtp,
                    jitter_buffers_rtp,
                    event_tx_rtp,
                    transcoding_service_rtp,
                    processing_config_rtp,
                ).await;
            });
        }

        // Start transcoding event processing
        if let Some(transcoding_rx) = self.transcoding_event_rx.take() {
            let relay_sessions_transcoding = Arc::clone(&self.relay_sessions);
            let event_tx_transcoding = self.event_tx.clone();

            tokio::spawn(async move {
                Self::process_transcoding_events(
                    transcoding_rx,
                    relay_sessions_transcoding,
                    event_tx_transcoding,
                ).await;
            });
        }

        // Start statistics monitoring
        let relay_sessions_stats = Arc::clone(&self.relay_sessions);
        let event_tx_stats = self.event_tx.clone();

        tokio::spawn(async move {
            Self::statistics_monitor_loop(relay_sessions_stats, event_tx_stats).await;
        });

        // Start session cleanup
        let relay_sessions_cleanup = Arc::clone(&self.relay_sessions);
        let jitter_buffers_cleanup = Arc::clone(&self.jitter_buffers);

        tokio::spawn(async move {
            Self::session_cleanup_loop(relay_sessions_cleanup, jitter_buffers_cleanup).await;
        });

        self.is_running = true;
        info!("Media relay service started successfully");
        Ok(())
    }

    async fn process_rtp_events(
        mut rtp_rx: mpsc::UnboundedReceiver<RtpEvent>,
        relay_sessions: Arc<DashMap<String, MediaRelaySession>>,
        jitter_buffers: Arc<DashMap<String, RwLock<JitterBuffer>>>,
        event_tx: mpsc::UnboundedSender<MediaRelayEvent>,
        transcoding_service: Arc<RwLock<TranscodingService>>,
        processing_config: MediaProcessingConfig,
    ) {
        while let Some(event) = rtp_rx.recv().await {
            match event {
                RtpEvent::PacketReceived { session_id, packet, source: _ } => {
                    if let Err(e) = Self::handle_rtp_packet(
                        session_id,
                        packet,
                        &relay_sessions,
                        &jitter_buffers,
                        &event_tx,
                        &transcoding_service,
                        &processing_config,
                    ).await {
                        error!("Failed to handle RTP packet: {}", e);
                    }
                }
                _ => {
                    trace!("Unhandled RTP event: {:?}", event);
                }
            }
        }
    }

    async fn handle_rtp_packet(
        session_id: String,
        packet: RtpPacket,
        relay_sessions: &Arc<DashMap<String, MediaRelaySession>>,
        jitter_buffers: &Arc<DashMap<String, RwLock<JitterBuffer>>>,
        event_tx: &mpsc::UnboundedSender<MediaRelayEvent>,
        transcoding_service: &Arc<RwLock<TranscodingService>>,
        processing_config: &MediaProcessingConfig,
    ) -> Result<()> {
        // Find relay session that owns this RTP session
        let mut relay_session: Option<MediaRelaySession> = None;
        let mut relay_direction: Option<RelayDirection> = None;

        for session_entry in relay_sessions.iter() {
            let session = session_entry.value();
            if session.leg_a_session_id == session_id {
                relay_session = Some(session.clone());
                relay_direction = Some(RelayDirection::AToB);
                break;
            } else if session.leg_b_session_id == session_id {
                relay_session = Some(session.clone());
                relay_direction = Some(RelayDirection::BToA);
                break;
            }
        }

        let (relay_session, direction) = match (relay_session, relay_direction) {
            (Some(session), Some(dir)) => (session, dir),
            _ => return Ok(()), // No relay session found
        };

        // Apply media processing if enabled
        let processed_packet = Self::apply_media_processing(
            packet,
            &relay_session,
            &direction,
            processing_config,
            event_tx,
        ).await?;

        // Apply jitter buffering
        let ready_packets = if processing_config.jitter_buffer_size > 0 {
            let jitter_buffer_key = format!("{}_{:?}", relay_session.id, direction);
            
            if !jitter_buffers.contains_key(&jitter_buffer_key) {
                let buffer = JitterBuffer::new(
                    processing_config.jitter_buffer_size as usize,
                    20, // 20ms target delay
                );
                jitter_buffers.insert(jitter_buffer_key.clone(), RwLock::new(buffer));
            }

            if let Some(buffer_lock) = jitter_buffers.get(&jitter_buffer_key) {
                let mut buffer = buffer_lock.write().await;
                buffer.add_packet(processed_packet)
            } else {
                vec![processed_packet]
            }
        } else {
            vec![processed_packet]
        };

        // Relay packets
        for packet_to_relay in ready_packets {
            Self::relay_packet(
                packet_to_relay,
                &relay_session,
                &direction,
                transcoding_service,
                relay_sessions,
                event_tx,
            ).await?;
        }

        Ok(())
    }

    async fn apply_media_processing(
        mut packet: RtpPacket,
        relay_session: &MediaRelaySession,
        direction: &RelayDirection,
        config: &MediaProcessingConfig,
        event_tx: &mpsc::UnboundedSender<MediaRelayEvent>,
    ) -> Result<RtpPacket> {
        // DTMF detection
        if config.enable_dtmf_detection {
            if let Some((digit, duration)) = Self::detect_dtmf(&packet) {
                let _ = event_tx.send(MediaRelayEvent::DtmfDetected {
                    session_id: relay_session.id.clone(),
                    digit,
                    duration_ms: duration,
                    direction: direction.clone(),
                });
            }
        }

        // Silence detection
        if config.enable_silence_detection {
            if Self::is_silence(&packet) {
                let _ = event_tx.send(MediaRelayEvent::SilenceDetected {
                    session_id: relay_session.id.clone(),
                    duration_ms: 20, // Assuming 20ms packet
                    direction: direction.clone(),
                });
            }
        }

        // Audio processing (simplified - real implementation would use DSP libraries)
        if config.enable_noise_reduction {
            packet = Self::apply_noise_reduction(packet);
        }

        if config.enable_automatic_gain_control {
            packet = Self::apply_agc(packet);
        }

        Ok(packet)
    }

    fn detect_dtmf(packet: &RtpPacket) -> Option<(char, u32)> {
        // Simplified DTMF detection
        // Real implementation would analyze audio frequencies
        if packet.payload_type == 101 && packet.payload.len() >= 4 {
            // RFC 2833 DTMF event
            let event = packet.payload[0];
            let duration = u16::from_be_bytes([packet.payload[2], packet.payload[3]]) as u32;
            
            let digit = match event {
                0..=9 => Some((b'0' + event) as char),
                10 => Some('*'),
                11 => Some('#'),
                12 => Some('A'),
                13 => Some('B'),
                14 => Some('C'),
                15 => Some('D'),
                _ => None,
            };

            if let Some(d) = digit {
                return Some((d, duration));
            }
        }
        None
    }

    fn is_silence(packet: &RtpPacket) -> bool {
        // Simplified silence detection
        // Real implementation would analyze audio energy levels
        if packet.payload.len() < 10 {
            return true;
        }

        let energy: u32 = packet.payload.iter()
            .map(|&b| (b as u32).pow(2))
            .sum();
        
        let average_energy = energy / packet.payload.len() as u32;
        average_energy < 100 // Threshold for silence
    }

    fn apply_noise_reduction(mut packet: RtpPacket) -> RtpPacket {
        // Simplified noise reduction
        // Real implementation would use sophisticated DSP algorithms
        let mut payload_vec = packet.payload.to_vec();
        for byte in payload_vec.iter_mut() {
            if *byte < 10 {
                *byte = 0; // Zero out low-level noise
            }
        }
        packet.payload = payload_vec.into();
        packet
    }

    fn apply_agc(mut packet: RtpPacket) -> RtpPacket {
        // Simplified automatic gain control
        // Real implementation would normalize audio levels
        let max_value = packet.payload.iter().max().copied().unwrap_or(0);
        if max_value > 0 && max_value < 128 {
            let gain = 128 / max_value.max(1);
            let mut payload_vec = packet.payload.to_vec();
            for byte in payload_vec.iter_mut() {
                *byte = (*byte).saturating_mul(gain);
            }
            packet.payload = payload_vec.into();
        }
        packet
    }

    async fn relay_packet(
        packet: RtpPacket,
        relay_session: &MediaRelaySession,
        direction: &RelayDirection,
        transcoding_service: &Arc<RwLock<TranscodingService>>,
        relay_sessions: &Arc<DashMap<String, MediaRelaySession>>,
        event_tx: &mpsc::UnboundedSender<MediaRelayEvent>,
    ) -> Result<()> {
        let target_session_id = match direction {
            RelayDirection::AToB => &relay_session.leg_b_session_id,
            RelayDirection::BToA => &relay_session.leg_a_session_id,
        };

        let mut final_packet = packet.clone();

        // Apply transcoding if needed
        if relay_session.relay_mode == RelayMode::Transcoding {
            if let Some(transcoding_session_id) = &relay_session.transcoding_session_id {
                let transcoding = transcoding_service.read().await;
                match transcoding.transcode_packet(
                    transcoding_session_id,
                    &packet.payload,
                    packet.timestamp,
                ).await {
                    Ok(transcoded_payload) => {
                        final_packet.payload = transcoded_payload.into();
                    }
                    Err(e) => {
                        error!("Transcoding failed: {}", e);
                        
                        // Update error count
                        if let Some(mut session) = relay_sessions.get_mut(&relay_session.id) {
                            session.stats.transcoding_errors += 1;
                        }

                        return Err(e);
                    }
                }
            }
        }

        // Update statistics
        if let Some(mut session) = relay_sessions.get_mut(&relay_session.id) {
            session.last_activity = Instant::now();
            
            match direction {
                RelayDirection::AToB => {
                    session.stats.packets_relayed_a_to_b += 1;
                    session.stats.bytes_relayed_a_to_b += final_packet.payload.len() as u64;
                }
                RelayDirection::BToA => {
                    session.stats.packets_relayed_b_to_a += 1;
                    session.stats.bytes_relayed_b_to_a += final_packet.payload.len() as u64;
                }
            }
        }

        // Forward packet (in real implementation, this would send via RTP handler)
        trace!("Relayed RTP packet: {} -> {} ({} bytes)",
            relay_session.id, target_session_id, final_packet.payload.len());

        Ok(())
    }

    async fn process_transcoding_events(
        mut transcoding_rx: mpsc::UnboundedReceiver<TranscodingEvent>,
        relay_sessions: Arc<DashMap<String, MediaRelaySession>>,
        event_tx: mpsc::UnboundedSender<MediaRelayEvent>,
    ) {
        while let Some(event) = transcoding_rx.recv().await {
            match event {
                TranscodingEvent::SessionCompleted { session_id, stats: _ } => {
                    // Find relay session using this transcoding session
                    for mut session_entry in relay_sessions.iter_mut() {
                        let session = session_entry.value_mut();
                        if session.transcoding_session_id.as_ref() == Some(&session_id) {
                            session.transcoding_session_id = None;
                            break;
                        }
                    }
                }
                TranscodingEvent::Error { session_id: _, message } => {
                    let _ = event_tx.send(MediaRelayEvent::Error {
                        session_id: None,
                        message: format!("Transcoding error: {}", message),
                    });
                }
                _ => {
                    trace!("Unhandled transcoding event: {:?}", event);
                }
            }
        }
    }

    async fn statistics_monitor_loop(
        relay_sessions: Arc<DashMap<String, MediaRelaySession>>,
        event_tx: mpsc::UnboundedSender<MediaRelayEvent>,
    ) {
        let mut stats_interval = interval(Duration::from_secs(10));

        loop {
            stats_interval.tick().await;

            for session_entry in relay_sessions.iter() {
                let session = session_entry.value();
                
                // Check quality metrics
                if session.stats.packet_loss_rate > 5.0 {
                    let _ = event_tx.send(MediaRelayEvent::QualityAlert {
                        session_id: session.id.clone(),
                        metric: "Packet Loss Rate".to_string(),
                        value: session.stats.packet_loss_rate,
                        threshold: 5.0,
                    });
                }

                if session.stats.average_latency_ms > 150.0 {
                    let _ = event_tx.send(MediaRelayEvent::QualityAlert {
                        session_id: session.id.clone(),
                        metric: "Average Latency".to_string(),
                        value: session.stats.average_latency_ms,
                        threshold: 150.0,
                    });
                }
            }
        }
    }

    async fn session_cleanup_loop(
        relay_sessions: Arc<DashMap<String, MediaRelaySession>>,
        jitter_buffers: Arc<DashMap<String, RwLock<JitterBuffer>>>,
    ) {
        let mut cleanup_interval = interval(Duration::from_secs(60));
        let session_timeout = Duration::from_secs(300); // 5 minutes

        loop {
            cleanup_interval.tick().await;
            let now = Instant::now();

            // Find inactive sessions
            let inactive_sessions: Vec<String> = relay_sessions
                .iter()
                .filter(|entry| {
                    now.duration_since(entry.value().last_activity) > session_timeout
                })
                .map(|entry| entry.key().clone())
                .collect();

            // Clean up inactive sessions
            for session_id in inactive_sessions {
                if let Some((_, _session)) = relay_sessions.remove(&session_id) {
                    // Clean up associated jitter buffers
                    jitter_buffers.remove(&format!("{}_AToB", session_id));
                    jitter_buffers.remove(&format!("{}_BToA", session_id));
                    
                    info!("Cleaned up inactive media relay session: {}", session_id);
                }
            }
        }
    }

    // Public API methods
    pub async fn create_relay_session(
        &self,
        call_id: &str,
        leg_a_session_id: &str,
        leg_b_session_id: &str,
        leg_a_codec: CodecType,
        leg_b_codec: CodecType,
    ) -> Result<String> {
        let session_id = Uuid::new_v4().to_string();
        let relay_mode = if leg_a_codec != leg_b_codec {
            RelayMode::Transcoding
        } else {
            RelayMode::Transparent
        };

        // Create transcoding session if needed
        let transcoding_session_id = if relay_mode == RelayMode::Transcoding {
            let transcoding = self.transcoding_service.read().await;
            let transcode_id = transcoding.create_transcoding_session(
                call_id,
                leg_a_codec.clone(),
                leg_b_codec.clone(),
                8000, // Default sample rate
                8000,
            ).await?;
            Some(transcode_id)
        } else {
            None
        };

        let session = MediaRelaySession {
            id: session_id.clone(),
            call_id: call_id.to_string(),
            leg_a_session_id: leg_a_session_id.to_string(),
            leg_b_session_id: leg_b_session_id.to_string(),
            leg_a_endpoint: MediaEndpoint {
                rtp_port: 0, // Will be set later
                rtcp_port: 0,
                remote_address: None,
                codec: leg_a_codec.clone(),
                ssrc: 0,
                payload_type: 0,
                last_packet_time: None,
            },
            leg_b_endpoint: MediaEndpoint {
                rtp_port: 0,
                rtcp_port: 0,
                remote_address: None,
                codec: leg_b_codec.clone(),
                ssrc: 0,
                payload_type: 0,
                last_packet_time: None,
            },
            relay_mode: relay_mode.clone(),
            transcoding_session_id,
            created_at: Instant::now(),
            last_activity: Instant::now(),
            stats: MediaRelayStats::new(leg_a_codec.clone(), leg_b_codec.clone()),
        };

        self.relay_sessions.insert(session_id.clone(), session);

        // Emit session started event
        let _ = self.event_tx.send(MediaRelayEvent::SessionStarted {
            session_id: session_id.clone(),
            call_id: call_id.to_string(),
            leg_a_codec,
            leg_b_codec,
            relay_mode,
        });

        info!("Created media relay session: {} for call {}", session_id, call_id);
        Ok(session_id)
    }

    pub async fn destroy_relay_session(&self, session_id: &str) -> Result<()> {
        if let Some((_, session)) = self.relay_sessions.remove(session_id) {
            // Destroy transcoding session if exists
            if let Some(transcoding_session_id) = &session.transcoding_session_id {
                let transcoding = self.transcoding_service.read().await;
                let _ = transcoding.destroy_transcoding_session(transcoding_session_id).await;
            }

            // Clean up jitter buffers
            self.jitter_buffers.remove(&format!("{}_AToB", session_id));
            self.jitter_buffers.remove(&format!("{}_BToA", session_id));

            // Emit session ended event
            let _ = self.event_tx.send(MediaRelayEvent::SessionEnded {
                session_id: session_id.to_string(),
                stats: session.stats.clone(),
            });

            info!("Destroyed media relay session: {}", session_id);
        }

        Ok(())
    }

    pub fn get_relay_session(&self, session_id: &str) -> Option<MediaRelaySession> {
        self.relay_sessions.get(session_id).map(|entry| entry.value().clone())
    }

    pub fn get_active_sessions(&self) -> Vec<MediaRelaySession> {
        self.relay_sessions.iter().map(|entry| entry.value().clone()).collect()
    }

    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping media relay service");

        // Destroy all active sessions
        let session_ids: Vec<String> = self.relay_sessions.iter().map(|entry| entry.key().clone()).collect();
        for session_id in session_ids {
            let _ = self.destroy_relay_session(&session_id).await;
        }

        self.is_running = false;
        info!("Media relay service stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PortRange;
    use crate::protocols::rtp::RtpHandler;
    use crate::services::transcoding::{TranscodingService, TranscodingBackend};

    #[tokio::test]
    async fn test_jitter_buffer() {
        let mut buffer = JitterBuffer::new(10, 20);
        
        let packet1 = RtpPacket::new(0, 100, 8000, 12345);
        let packet2 = RtpPacket::new(0, 102, 8160, 12345);
        let packet3 = RtpPacket::new(0, 101, 8080, 12345);

        // Add packets out of order
        buffer.add_packet(packet1);
        buffer.add_packet(packet2);
        let ready = buffer.add_packet(packet3);

        assert!(ready.len() > 0);
    }

    #[tokio::test]
    async fn test_media_relay_service_creation() {
        let rtp_config = PortRange { min: 10000, max: 10100 };
        let rtp_handler = Arc::new(RwLock::new(
            RtpHandler::new(rtp_config).unwrap()
        ));
        
        let transcoding_service = Arc::new(RwLock::new(
            TranscodingService::new(TranscodingBackend::Cpu)
        ));

        let service = MediaRelayService::new(
            rtp_handler,
            transcoding_service,
            MediaProcessingConfig::default(),
        );

        assert!(!service.is_running);
    }

    #[test]
    fn test_dtmf_detection() {
        let mut packet = RtpPacket::new(101, 1000, 8000, 12345);
        // RFC 2833 DTMF event payload: event=1, volume=10, duration=160
        packet.payload = vec![1, 10, 0, 160].into();

        let result = MediaRelayService::detect_dtmf(&packet);
        assert_eq!(result, Some(('1', 160)));
    }

    #[test]
    fn test_media_relay_stats() {
        let mut stats = MediaRelayStats::new(CodecType::G711u, CodecType::G711a);
        
        stats.packets_relayed_a_to_b = 100;
        stats.packets_relayed_b_to_a = 50;
        stats.bytes_relayed_a_to_b = 8000;
        stats.bytes_relayed_b_to_a = 4000;

        assert_eq!(stats.total_packets(), 150);
        assert_eq!(stats.total_bytes(), 12000);
    }
}