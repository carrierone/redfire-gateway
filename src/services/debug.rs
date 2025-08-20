//! Debug services for protocol analysis and troubleshooting

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info};

use crate::Result;

/// Debug configuration
#[derive(Debug, Clone)]
pub struct DebugConfig {
    pub sip_debug_enabled: bool,
    pub tdm_debug_enabled: bool,
    pub rtp_debug_enabled: bool,
    pub max_history_size: usize,
    pub capture_content: bool,
    pub filter_patterns: Vec<String>,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            sip_debug_enabled: false,
            tdm_debug_enabled: false,
            rtp_debug_enabled: false,
            max_history_size: 10000,
            capture_content: true,
            filter_patterns: Vec::new(),
        }
    }
}

/// Message direction
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageDirection {
    Incoming,
    Outgoing,
    Internal,
}

/// Protocol type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProtocolType {
    Sip,
    Q931,
    Lapd,
    Rtp,
    Rtcp,
}

/// Debug message entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugMessage {
    pub id: u64,
    pub timestamp: DateTime<Utc>,
    pub protocol: ProtocolType,
    pub direction: MessageDirection,
    pub span_id: Option<u32>,
    pub channel_id: Option<u8>,
    pub source: Option<SocketAddr>,
    pub destination: Option<SocketAddr>,
    pub call_reference: Option<u16>,
    pub call_id: Option<String>,
    pub message_type: String,
    pub content: Option<String>,
    pub raw_data: Option<Vec<u8>>,
    pub parsed_fields: HashMap<String, String>,
    pub size: usize,
}

/// SIP-specific debug information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SipDebugInfo {
    pub method: Option<String>,
    pub response_code: Option<u16>,
    pub response_phrase: Option<String>,
    pub from_header: Option<String>,
    pub to_header: Option<String>,
    pub via_headers: Vec<String>,
    pub contact_header: Option<String>,
    pub user_agent: Option<String>,
    pub content_type: Option<String>,
    pub content_length: usize,
    pub cseq: Option<String>,
}

/// Q.931 (D-channel) debug information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Q931DebugInfo {
    pub protocol_discriminator: u8,
    pub call_reference_flag: bool,
    pub call_reference_value: u16,
    pub message_type: u8,
    pub message_name: String,
    pub information_elements: Vec<InformationElementDebug>,
    pub cause_value: Option<u8>,
    pub progress_indicator: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InformationElementDebug {
    pub id: u8,
    pub name: String,
    pub length: usize,
    pub data: Vec<u8>,
    pub decoded_value: Option<String>,
}

/// LAPD debug information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LapdDebugInfo {
    pub address_field: u8,
    pub control_field: u8,
    pub frame_type: LapdFrameType,
    pub command_response: bool,
    pub poll_final: bool,
    pub sequence_numbers: Option<(u8, u8)>, // N(S), N(R)
    pub supervisory_function: Option<String>,
    pub unnumbered_function: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LapdFrameType {
    Information,
    Supervisory,
    Unnumbered,
}

/// RTP debug information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtpDebugInfo {
    pub version: u8,
    pub padding: bool,
    pub extension: bool,
    pub csrc_count: u8,
    pub marker: bool,
    pub payload_type: u8,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    pub payload_size: usize,
}

/// Debug events
#[derive(Debug, Clone)]
pub enum DebugEvent {
    MessageCaptured(DebugMessage),
    DebugModeChanged { protocol: ProtocolType, enabled: bool },
    FilterUpdated { patterns: Vec<String> },
    HistoryCleaned { messages_removed: usize },
}

/// B-channel status tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BChannelStatus {
    pub span_id: u32,
    pub channel_id: u8,
    pub state: BChannelState,
    pub call_reference: Option<u16>,
    pub call_id: Option<String>,
    pub caller_number: Option<String>,
    pub called_number: Option<String>,
    pub connect_time: Option<DateTime<Utc>>,
    pub disconnect_time: Option<DateTime<Utc>>,
    pub duration: Option<Duration>,
    pub codec: Option<String>,
    pub quality_metrics: ChannelQualityMetrics,
    pub last_activity: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BChannelState {
    Idle,
    Seized,
    Dialing,
    Proceeding,
    Alerting,
    Connected,
    Disconnecting,
    OutOfService,
    Maintenance,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelQualityMetrics {
    pub packet_loss_percent: f64,
    pub jitter_ms: f64,
    pub round_trip_time_ms: f64,
    pub mos_score: Option<f64>,
    pub error_count: u32,
}

/// Debug service for protocol analysis
pub struct DebugService {
    config: Arc<RwLock<DebugConfig>>,
    message_history: Arc<RwLock<VecDeque<DebugMessage>>>,
    channel_status: Arc<RwLock<HashMap<(u32, u8), BChannelStatus>>>,
    event_tx: mpsc::UnboundedSender<DebugEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<DebugEvent>>,
    message_counter: Arc<RwLock<u64>>,
    is_running: bool,
}

impl DebugService {
    pub fn new(config: DebugConfig) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        Self {
            config: Arc::new(RwLock::new(config)),
            message_history: Arc::new(RwLock::new(VecDeque::new())),
            channel_status: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            event_rx: Some(event_rx),
            message_counter: Arc::new(RwLock::new(0)),
            is_running: false,
        }
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<DebugEvent>> {
        self.event_rx.take()
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("Starting debug service");
        self.is_running = true;
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping debug service");
        self.is_running = false;
        Ok(())
    }

    /// Enable/disable SIP debugging
    pub async fn set_sip_debug(&self, enabled: bool) -> Result<()> {
        {
            let mut config = self.config.write().await;
            config.sip_debug_enabled = enabled;
        }

        let _ = self.event_tx.send(DebugEvent::DebugModeChanged {
            protocol: ProtocolType::Sip,
            enabled,
        });

        info!("SIP debug mode: {}", if enabled { "ENABLED" } else { "DISABLED" });
        Ok(())
    }

    /// Enable/disable TDM debugging
    pub async fn set_tdm_debug(&self, enabled: bool) -> Result<()> {
        {
            let mut config = self.config.write().await;
            config.tdm_debug_enabled = enabled;
        }

        let _ = self.event_tx.send(DebugEvent::DebugModeChanged {
            protocol: ProtocolType::Q931,
            enabled,
        });

        info!("TDM debug mode: {}", if enabled { "ENABLED" } else { "DISABLED" });
        Ok(())
    }

    /// Enable/disable RTP debugging
    pub async fn set_rtp_debug(&self, enabled: bool) -> Result<()> {
        {
            let mut config = self.config.write().await;
            config.rtp_debug_enabled = enabled;
        }

        let _ = self.event_tx.send(DebugEvent::DebugModeChanged {
            protocol: ProtocolType::Rtp,
            enabled,
        });

        info!("RTP debug mode: {}", if enabled { "ENABLED" } else { "DISABLED" });
        Ok(())
    }

    /// Capture SIP message for debugging
    pub async fn capture_sip_message(
        &self,
        direction: MessageDirection,
        source: SocketAddr,
        destination: SocketAddr,
        message: &str,
        call_id: Option<String>,
    ) -> Result<()> {
        let config = self.config.read().await;
        if !config.sip_debug_enabled {
            return Ok(());
        }

        let sip_info = self.parse_sip_message(message);
        let mut parsed_fields = HashMap::new();

        if let Some(ref method) = sip_info.method {
            parsed_fields.insert("Method".to_string(), method.clone());
        }
        if let Some(code) = sip_info.response_code {
            parsed_fields.insert("Response-Code".to_string(), code.to_string());
        }
        if let Some(ref from) = sip_info.from_header {
            parsed_fields.insert("From".to_string(), from.clone());
        }
        if let Some(ref to) = sip_info.to_header {
            parsed_fields.insert("To".to_string(), to.clone());
        }

        let debug_message = DebugMessage {
            id: self.next_message_id().await,
            timestamp: Utc::now(),
            protocol: ProtocolType::Sip,
            direction,
            span_id: None,
            channel_id: None,
            source: Some(source),
            destination: Some(destination),
            call_reference: None,
            call_id,
            message_type: sip_info.method
                .or_else(|| sip_info.response_code.map(|c| format!("{} {}", c, 
                    sip_info.response_phrase.as_deref().unwrap_or("Unknown"))))
                .unwrap_or_else(|| "Unknown".to_string()),
            content: if config.capture_content { Some(message.to_string()) } else { None },
            raw_data: None,
            parsed_fields,
            size: message.len(),
        };

        self.add_message_to_history(debug_message).await?;
        Ok(())
    }

    /// Capture Q.931 (D-channel) message for debugging
    pub async fn capture_q931_message(
        &self,
        direction: MessageDirection,
        span_id: u32,
        raw_data: &[u8],
    ) -> Result<()> {
        let config = self.config.read().await;
        if !config.tdm_debug_enabled {
            return Ok(());
        }

        let q931_info = self.parse_q931_message(raw_data);
        let mut parsed_fields = HashMap::new();

        parsed_fields.insert("Protocol-Discriminator".to_string(), 
                           format!("0x{:02X}", q931_info.protocol_discriminator));
        parsed_fields.insert("Call-Reference".to_string(), 
                           format!("0x{:04X}", q931_info.call_reference_value));
        parsed_fields.insert("Message-Type".to_string(), 
                           format!("0x{:02X}", q931_info.message_type));

        for ie in &q931_info.information_elements {
            parsed_fields.insert(format!("IE-{}", ie.name), 
                                ie.decoded_value.clone().unwrap_or_else(|| "Raw data".to_string()));
        }

        let debug_message = DebugMessage {
            id: self.next_message_id().await,
            timestamp: Utc::now(),
            protocol: ProtocolType::Q931,
            direction,
            span_id: Some(span_id),
            channel_id: None,
            source: None,
            destination: None,
            call_reference: Some(q931_info.call_reference_value),
            call_id: None,
            message_type: q931_info.message_name.clone(),
            content: None,
            raw_data: if config.capture_content { Some(raw_data.to_vec()) } else { None },
            parsed_fields,
            size: raw_data.len(),
        };

        self.add_message_to_history(debug_message).await?;
        Ok(())
    }

    /// Capture LAPD frame for debugging
    pub async fn capture_lapd_frame(
        &self,
        direction: MessageDirection,
        span_id: u32,
        raw_data: &[u8],
    ) -> Result<()> {
        let config = self.config.read().await;
        if !config.tdm_debug_enabled {
            return Ok(());
        }

        let lapd_info = self.parse_lapd_frame(raw_data);
        let mut parsed_fields = HashMap::new();

        parsed_fields.insert("Address".to_string(), format!("0x{:02X}", lapd_info.address_field));
        parsed_fields.insert("Control".to_string(), format!("0x{:02X}", lapd_info.control_field));
        parsed_fields.insert("Frame-Type".to_string(), format!("{:?}", lapd_info.frame_type));

        if let Some((ns, nr)) = lapd_info.sequence_numbers {
            parsed_fields.insert("N(S)".to_string(), ns.to_string());
            parsed_fields.insert("N(R)".to_string(), nr.to_string());
        }

        let frame_type_name = match lapd_info.frame_type {
            LapdFrameType::Information => "I-Frame",
            LapdFrameType::Supervisory => "S-Frame", 
            LapdFrameType::Unnumbered => "U-Frame",
        };

        let debug_message = DebugMessage {
            id: self.next_message_id().await,
            timestamp: Utc::now(),
            protocol: ProtocolType::Lapd,
            direction,
            span_id: Some(span_id),
            channel_id: None,
            source: None,
            destination: None,
            call_reference: None,
            call_id: None,
            message_type: frame_type_name.to_string(),
            content: None,
            raw_data: if config.capture_content { Some(raw_data.to_vec()) } else { None },
            parsed_fields,
            size: raw_data.len(),
        };

        self.add_message_to_history(debug_message).await?;
        Ok(())
    }

    /// Update B-channel status
    pub async fn update_channel_status(
        &self,
        span_id: u32,
        channel_id: u8,
        state: BChannelState,
        call_info: Option<(Option<String>, Option<String>, Option<String>)>, // call_id, caller, called
    ) -> Result<()> {
        let mut channels = self.channel_status.write().await;
        let key = (span_id, channel_id);
        
        let now = Utc::now();
        
        let status = channels.entry(key).or_insert_with(|| BChannelStatus {
            span_id,
            channel_id,
            state: BChannelState::Idle,
            call_reference: None,
            call_id: None,
            caller_number: None,
            called_number: None,
            connect_time: None,
            disconnect_time: None,
            duration: None,
            codec: None,
            quality_metrics: ChannelQualityMetrics {
                packet_loss_percent: 0.0,
                jitter_ms: 0.0,
                round_trip_time_ms: 0.0,
                mos_score: None,
                error_count: 0,
            },
            last_activity: now,
        });

        // Update state and handle state transitions
        let previous_state = status.state.clone();
        status.state = state.clone();
        status.last_activity = now;

        // Handle call information
        if let Some((call_id, caller, called)) = call_info {
            status.call_id = call_id;
            status.caller_number = caller;
            status.called_number = called;
        }

        // Handle state transitions
        match (&previous_state, &state) {
            (_, BChannelState::Connected) if previous_state != BChannelState::Connected => {
                status.connect_time = Some(now);
            },
            (BChannelState::Connected, _) if state != BChannelState::Connected => {
                status.disconnect_time = Some(now);
                if let Some(connect_time) = status.connect_time {
                    status.duration = Some((now - connect_time).to_std().unwrap_or(Duration::from_secs(0)));
                }
            },
            _ => {}
        }

        debug!("Channel {}/{} state changed: {:?} -> {:?}", 
               span_id, channel_id, previous_state, state);

        Ok(())
    }

    /// Get current B-channel status
    pub async fn get_channel_status(&self, span_id: Option<u32>) -> Vec<BChannelStatus> {
        let channels = self.channel_status.read().await;
        
        channels.values()
            .filter(|status| span_id.map_or(true, |s| status.span_id == s))
            .cloned()
            .collect()
    }

    /// Get debug message history
    pub async fn get_message_history(
        &self,
        protocol: Option<ProtocolType>,
        limit: Option<usize>,
    ) -> Vec<DebugMessage> {
        let history = self.message_history.read().await;
        
        history.iter()
            .rev() // Most recent first
            .filter(|msg| protocol.as_ref().map_or(true, |p| &msg.protocol == p))
            .take(limit.unwrap_or(usize::MAX))
            .cloned()
            .collect()
    }

    /// Clear debug message history
    pub async fn clear_history(&self) -> Result<usize> {
        let mut history = self.message_history.write().await;
        let count = history.len();
        history.clear();
        
        let _ = self.event_tx.send(DebugEvent::HistoryCleaned {
            messages_removed: count,
        });
        
        info!("Cleared {} debug messages from history", count);
        Ok(count)
    }

    /// Get debug statistics
    pub async fn get_debug_stats(&self) -> DebugStatistics {
        let history = self.message_history.read().await;
        let channels = self.channel_status.read().await;
        
        let mut stats = DebugStatistics {
            total_messages: history.len(),
            messages_by_protocol: HashMap::new(),
            active_channels: 0,
            channels_by_state: HashMap::new(),
            oldest_message: None,
            newest_message: None,
        };

        // Analyze message history
        for msg in history.iter() {
            *stats.messages_by_protocol.entry(msg.protocol.clone()).or_insert(0) += 1;
        }

        if let Some(oldest) = history.front() {
            stats.oldest_message = Some(oldest.timestamp);
        }
        if let Some(newest) = history.back() {
            stats.newest_message = Some(newest.timestamp);
        }

        // Analyze channel status
        for status in channels.values() {
            if status.state != BChannelState::Idle {
                stats.active_channels += 1;
            }
            *stats.channels_by_state.entry(status.state.clone()).or_insert(0) += 1;
        }

        stats
    }

    // Private helper methods

    async fn next_message_id(&self) -> u64 {
        let mut counter = self.message_counter.write().await;
        *counter += 1;
        *counter
    }

    async fn add_message_to_history(&self, message: DebugMessage) -> Result<()> {
        let mut history = self.message_history.write().await;
        let config = self.config.read().await;
        
        // Apply filters
        if !self.message_matches_filters(&message, &config.filter_patterns) {
            return Ok(());
        }

        history.push_back(message.clone());
        
        // Limit history size
        while history.len() > config.max_history_size {
            history.pop_front();
        }

        let _ = self.event_tx.send(DebugEvent::MessageCaptured(message));
        Ok(())
    }

    fn message_matches_filters(&self, message: &DebugMessage, patterns: &[String]) -> bool {
        if patterns.is_empty() {
            return true;
        }

        for pattern in patterns {
            if message.message_type.contains(pattern) ||
               message.call_id.as_ref().map_or(false, |id| id.contains(pattern)) ||
               message.parsed_fields.values().any(|value| value.contains(pattern)) {
                return true;
            }
        }

        false
    }

    fn parse_sip_message(&self, message: &str) -> SipDebugInfo {
        // Simplified SIP parsing - in reality would be more comprehensive
        let lines: Vec<&str> = message.lines().collect();
        let mut info = SipDebugInfo {
            method: None,
            response_code: None,
            response_phrase: None,
            from_header: None,
            to_header: None,
            via_headers: Vec::new(),
            contact_header: None,
            user_agent: None,
            content_type: None,
            content_length: 0,
            cseq: None,
        };

        if let Some(first_line) = lines.first() {
            if first_line.starts_with("SIP/2.0") {
                // Response
                let parts: Vec<&str> = first_line.split_whitespace().collect();
                if parts.len() >= 3 {
                    if let Ok(code) = parts[1].parse::<u16>() {
                        info.response_code = Some(code);
                        info.response_phrase = Some(parts[2..].join(" "));
                    }
                }
            } else {
                // Request
                let parts: Vec<&str> = first_line.split_whitespace().collect();
                if !parts.is_empty() {
                    info.method = Some(parts[0].to_string());
                }
            }
        }

        // Parse headers
        for line in &lines[1..] {
            if line.is_empty() {
                break; // End of headers
            }

            if let Some((header, value)) = line.split_once(':') {
                let header = header.trim().to_lowercase();
                let value = value.trim();

                match header.as_str() {
                    "from" | "f" => info.from_header = Some(value.to_string()),
                    "to" | "t" => info.to_header = Some(value.to_string()),
                    "via" | "v" => info.via_headers.push(value.to_string()),
                    "contact" | "m" => info.contact_header = Some(value.to_string()),
                    "user-agent" => info.user_agent = Some(value.to_string()),
                    "content-type" | "c" => info.content_type = Some(value.to_string()),
                    "content-length" | "l" => {
                        info.content_length = value.parse().unwrap_or(0);
                    },
                    "cseq" => info.cseq = Some(value.to_string()),
                    _ => {}
                }
            }
        }

        info
    }

    fn parse_q931_message(&self, data: &[u8]) -> Q931DebugInfo {
        // Simplified Q.931 parsing - in reality would be more comprehensive
        if data.len() < 4 {
            return Q931DebugInfo {
                protocol_discriminator: 0,
                call_reference_flag: false,
                call_reference_value: 0,
                message_type: 0,
                message_name: "Invalid".to_string(),
                information_elements: Vec::new(),
                cause_value: None,
                progress_indicator: None,
            };
        }

        let protocol_discriminator = data[0];
        let _call_ref_len = data[1];
        let call_reference_flag = (data[2] & 0x80) != 0;
        let call_reference_value = u16::from_be_bytes([data[2] & 0x7F, data[3]]);
        let message_type = data[4];

        let message_name = match message_type {
            0x05 => "SETUP",
            0x07 => "CONNECT",
            0x0F => "CONNECT ACK",
            0x01 => "ALERTING",
            0x02 => "CALL PROCEEDING",
            0x03 => "PROGRESS",
            0x45 => "DISCONNECT",
            0x4D => "RELEASE",
            0x5A => "RELEASE COMPLETE",
            _ => "UNKNOWN",
        }.to_string();

        // Parse information elements (simplified)
        let mut information_elements = Vec::new();
        let mut offset = 5;
        
        while offset + 1 < data.len() {
            let ie_id = data[offset];
            let ie_len = data[offset + 1] as usize;
            
            if offset + 2 + ie_len > data.len() {
                break;
            }

            let ie_data = data[offset + 2..offset + 2 + ie_len].to_vec();
            let ie_name = match ie_id {
                0x18 => "Channel Identification",
                0x1E => "Progress Indicator",
                0x08 => "Cause",
                0x6C => "Calling Party Number",
                0x70 => "Called Party Number",
                _ => "Unknown IE",
            };

            information_elements.push(InformationElementDebug {
                id: ie_id,
                name: ie_name.to_string(),
                length: ie_len,
                data: ie_data.clone(),
                decoded_value: self.decode_information_element(ie_id, &ie_data),
            });

            offset += 2 + ie_len;
        }

        Q931DebugInfo {
            protocol_discriminator,
            call_reference_flag,
            call_reference_value,
            message_type,
            message_name,
            information_elements,
            cause_value: None,
            progress_indicator: None,
        }
    }

    fn parse_lapd_frame(&self, data: &[u8]) -> LapdDebugInfo {
        // Simplified LAPD parsing
        if data.len() < 2 {
            return LapdDebugInfo {
                address_field: 0,
                control_field: 0,
                frame_type: LapdFrameType::Unnumbered,
                command_response: false,
                poll_final: false,
                sequence_numbers: None,
                supervisory_function: None,
                unnumbered_function: None,
            };
        }

        let address_field = data[0];
        let control_field = data[1];
        
        let command_response = (address_field & 0x02) != 0;
        
        let (frame_type, sequence_numbers, poll_final, supervisory_function, unnumbered_function) = 
            if (control_field & 0x01) == 0 {
                // I-frame
                let ns = (control_field >> 1) & 0x07;
                let nr = (control_field >> 5) & 0x07;
                let pf = (control_field & 0x10) != 0;
                (LapdFrameType::Information, Some((ns, nr)), pf, None, None)
            } else if (control_field & 0x03) == 0x01 {
                // S-frame
                let nr = (control_field >> 5) & 0x07;
                let pf = (control_field & 0x10) != 0;
                let ss = (control_field >> 2) & 0x03;
                let function = match ss {
                    0 => "RR (Receiver Ready)",
                    1 => "RNR (Receiver Not Ready)",
                    2 => "REJ (Reject)",
                    3 => "SREJ (Selective Reject)",
                    _ => "Unknown",
                };
                (LapdFrameType::Supervisory, Some((0, nr)), pf, Some(function.to_string()), None)
            } else {
                // U-frame
                let pf = (control_field & 0x10) != 0;
                let function = "UI (Unnumbered Information)"; // Simplified
                (LapdFrameType::Unnumbered, None, pf, None, Some(function.to_string()))
            };

        LapdDebugInfo {
            address_field,
            control_field,
            frame_type,
            command_response,
            poll_final,
            sequence_numbers,
            supervisory_function,
            unnumbered_function,
        }
    }

    fn decode_information_element(&self, ie_id: u8, data: &[u8]) -> Option<String> {
        match ie_id {
            0x6C | 0x70 => {
                // Calling/Called party number - simplified decoding
                if !data.is_empty() {
                    let number_digits: String = data[1..].iter()
                        .map(|&b| char::from_digit((b & 0x0F) as u32, 10).unwrap_or('?'))
                        .collect();
                    Some(number_digits)
                } else {
                    None
                }
            },
            0x08 => {
                // Cause
                if data.len() >= 2 {
                    Some(format!("Cause: {}", data[1] & 0x7F))
                } else {
                    None
                }
            },
            _ => None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }
}

#[derive(Debug, Clone)]
pub struct DebugStatistics {
    pub total_messages: usize,
    pub messages_by_protocol: HashMap<ProtocolType, usize>,
    pub active_channels: usize,
    pub channels_by_state: HashMap<BChannelState, usize>,
    pub oldest_message: Option<DateTime<Utc>>,
    pub newest_message: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_debug_service_creation() {
        let config = DebugConfig::default();
        let service = DebugService::new(config);
        
        assert!(!service.is_running());
    }

    #[tokio::test]
    async fn test_sip_debug_enable() {
        let config = DebugConfig::default();
        let service = DebugService::new(config);
        
        service.set_sip_debug(true).await.unwrap();
        
        let config = service.config.read().await;
        assert!(config.sip_debug_enabled);
    }

    #[tokio::test]
    async fn test_channel_status_update() {
        let config = DebugConfig::default();
        let service = DebugService::new(config);
        
        service.update_channel_status(
            1, 1, 
            BChannelState::Connected,
            Some((Some("test-call".to_string()), Some("1234".to_string()), Some("5678".to_string())))
        ).await.unwrap();
        
        let channels = service.get_channel_status(Some(1)).await;
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].state, BChannelState::Connected);
        assert_eq!(channels[0].call_id, Some("test-call".to_string()));
    }
}