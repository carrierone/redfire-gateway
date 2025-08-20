//! TDM over Ethernet (TDMoE) interface implementation

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use dashmap::DashMap;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Interval};
use tracing::{error, info, trace, warn};

use crate::{Error, Result};

const TDMOE_HEADER_SIZE: usize = 12;
const TDMOE_MAGIC: u16 = 0x7A7A;
const TDMOE_VERSION: u8 = 1;

/// TDMoE frame types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Voice = 0x00,
    Control = 0x01,
    LoopbackCommand = 0x02,
    LoopbackResponse = 0x03,
    Keepalive = 0x04,
}

impl From<u8> for FrameType {
    fn from(value: u8) -> Self {
        match value {
            0x00 => FrameType::Voice,
            0x01 => FrameType::Control,
            0x02 => FrameType::LoopbackCommand,
            0x03 => FrameType::LoopbackResponse,
            0x04 => FrameType::Keepalive,
            _ => FrameType::Voice, // Default fallback
        }
    }
}

/// TDMoE frame structure
#[derive(Debug, Clone)]
pub struct TdmoeFrame {
    pub magic: u16,
    pub version: u8,
    pub frame_type: FrameType,
    pub channel: u16,
    pub sequence: u32,
    pub timestamp: u32,
    pub payload: Bytes,
}

impl TdmoeFrame {
    pub fn new(frame_type: FrameType, channel: u16, payload: Bytes) -> Self {
        Self {
            magic: TDMOE_MAGIC,
            version: TDMOE_VERSION,
            frame_type,
            channel,
            sequence: 0,
            timestamp: 0,
            payload,
        }
    }

    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(TDMOE_HEADER_SIZE + self.payload.len());
        
        buf.put_u16(self.magic);
        buf.put_u8(self.version);
        buf.put_u8(self.frame_type as u8);
        buf.put_u16(self.channel);
        buf.put_u32(self.sequence);
        buf.put_u32(self.timestamp);
        buf.put(self.payload.clone());
        
        buf.freeze()
    }

    pub fn decode(mut data: Bytes) -> Result<Self> {
        if data.len() < TDMOE_HEADER_SIZE {
            return Err(Error::protocol("TDMoE frame too short"));
        }

        let magic = data.get_u16();
        if magic != TDMOE_MAGIC {
            return Err(Error::protocol("Invalid TDMoE magic number"));
        }

        let version = data.get_u8();
        if version != TDMOE_VERSION {
            return Err(Error::protocol("Unsupported TDMoE version"));
        }

        let frame_type = FrameType::from(data.get_u8());
        let channel = data.get_u16();
        let sequence = data.get_u32();
        let timestamp = data.get_u32();
        let payload = data;

        Ok(Self {
            magic,
            version,
            frame_type,
            channel,
            sequence,
            timestamp,
            payload,
        })
    }
}

/// Channel status information
#[derive(Debug, Clone)]
pub struct ChannelStatus {
    pub channel_id: u16,
    pub is_active: bool,
    pub last_seen: Instant,
    pub frame_count: u64,
    pub error_count: u64,
    pub loopback_active: bool,
}

impl ChannelStatus {
    pub fn new(channel_id: u16) -> Self {
        Self {
            channel_id,
            is_active: false,
            last_seen: Instant::now(),
            frame_count: 0,
            error_count: 0,
            loopback_active: false,
        }
    }
}

/// Remote loopback command types
#[derive(Debug, Clone, Copy)]
pub enum LoopbackCommand {
    Activate = 0x01,
    Deactivate = 0x02,
    Status = 0x03,
}

/// Remote loopback types
#[derive(Debug, Clone, Copy)]
pub enum LoopbackType {
    Line = 0x01,
    Payload = 0x02,
    Network = 0x03,
}

/// Events emitted by the TDMoE interface
#[derive(Debug, Clone)]
pub enum TdmoeEvent {
    FrameReceived {
        frame: TdmoeFrame,
        source: SocketAddr,
    },
    ChannelStateChanged {
        channel: u16,
        active: bool,
    },
    LoopbackResponse {
        channel: u16,
        success: bool,
        loopback_type: LoopbackType,
    },
    Error {
        error: String,
        channel: Option<u16>,
    },
}

/// TDMoE interface configuration
#[derive(Debug, Clone)]
pub struct TdmoeConfig {
    pub interface: String,
    pub bind_port: u16,
    pub remote_addr: Option<SocketAddr>,
    pub channels: u16,
    pub keepalive_interval: Duration,
    pub frame_timeout: Duration,
    pub max_retries: u8,
}

impl Default for TdmoeConfig {
    fn default() -> Self {
        Self {
            interface: "eth0".to_string(),
            bind_port: 2427, // Standard TDMoE port
            remote_addr: None,
            channels: 30,
            keepalive_interval: Duration::from_secs(30),
            frame_timeout: Duration::from_secs(5),
            max_retries: 3,
        }
    }
}

/// TDMoE interface implementation
pub struct TdmoeInterface {
    config: TdmoeConfig,
    socket: Arc<UdpSocket>,
    channels: Arc<DashMap<u16, ChannelStatus>>,
    sequence_counter: Arc<RwLock<u32>>,
    event_tx: mpsc::UnboundedSender<TdmoeEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<TdmoeEvent>>,
    #[allow(dead_code)]
    keepalive_interval: Option<Interval>,
}

impl TdmoeInterface {
    pub async fn new(config: TdmoeConfig) -> Result<Self> {
        let bind_addr = format!("0.0.0.0:{}", config.bind_port);
        let socket = UdpSocket::bind(&bind_addr).await
            .map_err(|e| Error::network(format!("Failed to bind to {}: {}", bind_addr, e)))?;

        info!("TDMoE interface bound to {}", bind_addr);

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let channels = Arc::new(DashMap::new());

        // Initialize channel status for all configured channels
        for i in 1..=config.channels {
            channels.insert(i, ChannelStatus::new(i));
        }

        Ok(Self {
            config,
            socket: Arc::new(socket),
            channels,
            sequence_counter: Arc::new(RwLock::new(0)),
            event_tx,
            event_rx: Some(event_rx),
            keepalive_interval: None,
        })
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<TdmoeEvent>> {
        self.event_rx.take()
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("Starting TDMoE interface");

        // Start receiver task
        let socket_recv = Arc::clone(&self.socket);
        let channels_recv = Arc::clone(&self.channels);
        let event_tx_recv = self.event_tx.clone();
        
        tokio::spawn(async move {
            Self::receive_loop(socket_recv, channels_recv, event_tx_recv).await;
        });

        // Start keepalive task
        let mut keepalive_interval = interval(self.config.keepalive_interval);
        let socket_keepalive = Arc::clone(&self.socket);
        let config_keepalive = self.config.clone();
        let sequence_keepalive = Arc::clone(&self.sequence_counter);
        
        tokio::spawn(async move {
            loop {
                keepalive_interval.tick().await;
                if let Err(e) = Self::send_keepalive(
                    &socket_keepalive,
                    &config_keepalive,
                    &sequence_keepalive,
                ).await {
                    error!("Failed to send keepalive: {}", e);
                }
            }
        });

        // Start channel monitoring task
        let channels_monitor = Arc::clone(&self.channels);
        let event_tx_monitor = self.event_tx.clone();
        let frame_timeout = self.config.frame_timeout;
        
        tokio::spawn(async move {
            Self::channel_monitor_loop(channels_monitor, event_tx_monitor, frame_timeout).await;
        });

        info!("TDMoE interface started successfully");
        Ok(())
    }

    async fn receive_loop(
        socket: Arc<UdpSocket>,
        channels: Arc<DashMap<u16, ChannelStatus>>,
        event_tx: mpsc::UnboundedSender<TdmoeEvent>,
    ) {
        let mut buffer = vec![0u8; 2048];

        loop {
            match socket.recv_from(&mut buffer).await {
                Ok((size, source)) => {
                    let data = Bytes::copy_from_slice(&buffer[..size]);
                    
                    match TdmoeFrame::decode(data) {
                        Ok(frame) => {
                            trace!("Received TDMoE frame: channel={}, type={:?}, size={}",
                                frame.channel, frame.frame_type, frame.payload.len());

                            // Update channel statistics
                            if let Some(mut channel) = channels.get_mut(&frame.channel) {
                                channel.last_seen = Instant::now();
                                channel.frame_count += 1;
                                if !channel.is_active {
                                    channel.is_active = true;
                                    let _ = event_tx.send(TdmoeEvent::ChannelStateChanged {
                                        channel: frame.channel,
                                        active: true,
                                    });
                                }
                            }

                            // Handle different frame types
                            match frame.frame_type {
                                FrameType::LoopbackResponse => {
                                    Self::handle_loopback_response(&frame, &event_tx);
                                }
                                _ => {
                                    let _ = event_tx.send(TdmoeEvent::FrameReceived {
                                        frame,
                                        source,
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to decode TDMoE frame from {}: {}", source, e);
                            let _ = event_tx.send(TdmoeEvent::Error {
                                error: format!("Frame decode error: {}", e),
                                channel: None,
                            });
                        }
                    }
                }
                Err(e) => {
                    error!("UDP receive error: {}", e);
                    let _ = event_tx.send(TdmoeEvent::Error {
                        error: format!("UDP receive error: {}", e),
                        channel: None,
                    });
                }
            }
        }
    }

    fn handle_loopback_response(
        frame: &TdmoeFrame,
        event_tx: &mpsc::UnboundedSender<TdmoeEvent>,
    ) {
        if frame.payload.len() >= 2 {
            let success = frame.payload[0] != 0;
            let loopback_type = match frame.payload[1] {
                0x01 => LoopbackType::Line,
                0x02 => LoopbackType::Payload,
                0x03 => LoopbackType::Network,
                _ => LoopbackType::Line,
            };

            let _ = event_tx.send(TdmoeEvent::LoopbackResponse {
                channel: frame.channel,
                success,
                loopback_type,
            });
        }
    }

    async fn channel_monitor_loop(
        channels: Arc<DashMap<u16, ChannelStatus>>,
        event_tx: mpsc::UnboundedSender<TdmoeEvent>,
        timeout_duration: Duration,
    ) {
        let mut monitor_interval = interval(Duration::from_secs(5));

        loop {
            monitor_interval.tick().await;
            let now = Instant::now();

            for mut channel in channels.iter_mut() {
                if channel.is_active && now.duration_since(channel.last_seen) > timeout_duration {
                    warn!("Channel {} timed out", channel.channel_id);
                    channel.is_active = false;
                    let _ = event_tx.send(TdmoeEvent::ChannelStateChanged {
                        channel: channel.channel_id,
                        active: false,
                    });
                }
            }
        }
    }

    async fn send_keepalive(
        socket: &UdpSocket,
        config: &TdmoeConfig,
        sequence_counter: &RwLock<u32>,
    ) -> Result<()> {
        if let Some(remote_addr) = config.remote_addr {
            let mut seq = sequence_counter.write().await;
            *seq += 1;

            let frame = TdmoeFrame {
                magic: TDMOE_MAGIC,
                version: TDMOE_VERSION,
                frame_type: FrameType::Keepalive,
                channel: 0,
                sequence: *seq,
                timestamp: chrono::Utc::now().timestamp() as u32,
                payload: Bytes::new(),
            };

            let data = frame.encode();
            socket.send_to(&data, remote_addr).await?;
            trace!("Sent keepalive to {}", remote_addr);
        }
        Ok(())
    }

    pub async fn send_frame(&self, frame: TdmoeFrame, dest: Option<SocketAddr>) -> Result<()> {
        let target = dest.or(self.config.remote_addr)
            .ok_or_else(|| Error::network("No destination address specified"))?;

        let mut frame = frame;
        
        // Set sequence number
        {
            let mut seq = self.sequence_counter.write().await;
            *seq += 1;
            frame.sequence = *seq;
        }

        // Set timestamp
        frame.timestamp = chrono::Utc::now().timestamp() as u32;

        let data = frame.encode();
        self.socket.send_to(&data, target).await?;
        
        trace!("Sent TDMoE frame: channel={}, type={:?}, size={} to {}",
            frame.channel, frame.frame_type, frame.payload.len(), target);

        Ok(())
    }

    pub async fn send_voice_frame(&self, channel: u16, payload: Bytes) -> Result<()> {
        let frame = TdmoeFrame::new(FrameType::Voice, channel, payload);
        self.send_frame(frame, None).await
    }

    pub async fn send_loopback_command(
        &self,
        channel: u16,
        command: LoopbackCommand,
        loopback_type: LoopbackType,
    ) -> Result<()> {
        let mut payload = BytesMut::with_capacity(2);
        payload.put_u8(command as u8);
        payload.put_u8(loopback_type as u8);

        let frame = TdmoeFrame::new(FrameType::LoopbackCommand, channel, payload.freeze());
        self.send_frame(frame, None).await?;

        // Update channel loopback status
        if let Some(mut channel_status) = self.channels.get_mut(&channel) {
            channel_status.loopback_active = matches!(command, LoopbackCommand::Activate);
        }

        info!("Sent loopback command: channel={}, command={:?}, type={:?}",
            channel, command, loopback_type);

        Ok(())
    }

    pub fn get_channel_status(&self, channel: u16) -> Option<ChannelStatus> {
        self.channels.get(&channel).map(|status| status.clone())
    }

    pub fn get_all_channel_status(&self) -> HashMap<u16, ChannelStatus> {
        self.channels
            .iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect()
    }

    pub fn get_active_channels(&self) -> Vec<u16> {
        self.channels
            .iter()
            .filter(|entry| entry.value().is_active)
            .map(|entry| *entry.key())
            .collect()
    }

    pub async fn get_statistics(&self) -> TdmoeStatistics {
        let mut stats = TdmoeStatistics::default();

        for channel in self.channels.iter() {
            stats.total_channels += 1;
            if channel.is_active {
                stats.active_channels += 1;
            }
            stats.total_frames += channel.frame_count;
            stats.total_errors += channel.error_count;
            if channel.loopback_active {
                stats.loopback_active_channels += 1;
            }
        }

        stats
    }

    pub async fn stop(&self) -> Result<()> {
        info!("Stopping TDMoE interface");
        // The socket will be closed when dropped
        // Background tasks will be cancelled when their handles are dropped
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct TdmoeStatistics {
    pub total_channels: u32,
    pub active_channels: u32,
    pub total_frames: u64,
    pub total_errors: u64,
    pub loopback_active_channels: u32,
    pub last_keepalive: Option<Instant>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tdmoe_frame_encoding() {
        let payload = Bytes::from("test payload");
        let frame = TdmoeFrame::new(FrameType::Voice, 1, payload.clone());
        
        let encoded = frame.encode();
        let decoded = TdmoeFrame::decode(encoded).unwrap();
        
        assert_eq!(decoded.frame_type, FrameType::Voice);
        assert_eq!(decoded.channel, 1);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_invalid_magic_number() {
        let mut buf = BytesMut::new();
        buf.put_u16(0x1234); // Invalid magic
        buf.put_u8(1);
        buf.put_u8(0);
        buf.put_u16(1);
        buf.put_u32(0);
        buf.put_u32(0);
        
        let result = TdmoeFrame::decode(buf.freeze());
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tdmoe_interface_creation() {
        let config = TdmoeConfig {
            bind_port: 0, // Use ephemeral port for testing
            ..Default::default()
        };
        
        let interface = TdmoeInterface::new(config).await;
        assert!(interface.is_ok());
    }
}