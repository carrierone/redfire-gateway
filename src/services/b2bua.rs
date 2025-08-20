//! B2BUA (Back-to-Back User Agent) implementation
//! 
//! This module provides comprehensive B2BUA functionality for call relay,
//! session management, and media bridging between two SIP call legs.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use crate::config::{B2buaConfig, RouteType, NumberTranslation};
use crate::protocols::sip::{SipEvent, SipHandler};
use crate::protocols::rtp::{RtpEvent, RtpHandler};
use crate::{Error, Result};

/// B2BUA call leg identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CallLeg {
    A, // Incoming call leg
    B, // Outgoing call leg
}

/// B2BUA call state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum B2buaCallState {
    Idle,
    Establishing,
    Ringing,
    Connected,
    Disconnecting,
    Terminated,
}

/// B2BUA call correlation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct B2buaCall {
    pub id: String,
    pub state: B2buaCallState,
    pub leg_a_session_id: String,
    pub leg_b_session_id: Option<String>,
    pub leg_a_rtp_session_id: Option<String>,
    pub leg_b_rtp_session_id: Option<String>,
    pub caller: String,
    pub callee: String,
    pub destination_uri: String,
    #[serde(skip, default = "Instant::now")]
    pub created_at: Instant,
    #[serde(skip, default)]
    pub connected_at: Option<Instant>,
    #[serde(skip, default)]
    pub terminated_at: Option<Instant>,
    #[serde(skip, default = "Instant::now")]
    pub last_activity: Instant,
    #[serde(skip, default)]
    pub call_duration: Option<Duration>,
    pub routing_info: RoutingInfo,
}

/// Call routing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingInfo {
    pub route_type: RouteType,
    pub target_gateway: Option<String>,
    pub number_translation: Option<NumberTranslation>,
    pub codec_preference: Vec<String>,
    pub priority: u8,
}

/// B2BUA media relay information
#[derive(Debug, Clone)]
pub struct MediaRelay {
    pub call_id: String,
    pub leg_a_rtp_port: u16,
    pub leg_b_rtp_port: u16,
    pub leg_a_remote_addr: Option<SocketAddr>,
    pub leg_b_remote_addr: Option<SocketAddr>,
    pub bytes_relayed_a_to_b: u64,
    pub bytes_relayed_b_to_a: u64,
    pub packets_relayed_a_to_b: u64,
    pub packets_relayed_b_to_a: u64,
    pub started_at: Instant,
    pub last_activity: Instant,
}

/// B2BUA events
#[derive(Debug, Clone)]
pub enum B2buaEvent {
    CallEstablishing {
        call_id: String,
        caller: String,
        callee: String,
    },
    CallConnected {
        call_id: String,
        duration_to_connect: Duration,
    },
    CallTerminated {
        call_id: String,
        reason: String,
        duration: Option<Duration>,
    },
    MediaRelayStarted {
        call_id: String,
        leg_a_port: u16,
        leg_b_port: u16,
    },
    MediaRelayStats {
        call_id: String,
        stats: MediaRelay,
    },
    RoutingDecision {
        call_id: String,
        caller: String,
        callee: String,
        route: RoutingInfo,
    },
    Error {
        call_id: Option<String>,
        message: String,
    },
}


/// B2BUA service implementation
pub struct B2buaService {
    config: B2buaConfig,
    sip_handler: Arc<RwLock<SipHandler>>,
    rtp_handler: Arc<RwLock<RtpHandler>>,
    calls: Arc<DashMap<String, B2buaCall>>,
    media_relays: Arc<DashMap<String, MediaRelay>>,
    event_tx: mpsc::UnboundedSender<B2buaEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<B2buaEvent>>,
    sip_event_rx: Option<mpsc::UnboundedReceiver<SipEvent>>,
    rtp_event_rx: Option<mpsc::UnboundedReceiver<RtpEvent>>,
    is_running: bool,
}

impl B2buaService {
    pub fn new(
        config: B2buaConfig,
        sip_handler: Arc<RwLock<SipHandler>>,
        rtp_handler: Arc<RwLock<RtpHandler>>,
    ) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Ok(Self {
            config,
            sip_handler,
            rtp_handler,
            calls: Arc::new(DashMap::new()),
            media_relays: Arc::new(DashMap::new()),
            event_tx,
            event_rx: Some(event_rx),
            sip_event_rx: None,
            rtp_event_rx: None,
            is_running: false,
        })
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<B2buaEvent>> {
        self.event_rx.take()
    }

    pub fn set_sip_event_receiver(&mut self, rx: mpsc::UnboundedReceiver<SipEvent>) {
        self.sip_event_rx = Some(rx);
    }

    pub fn set_rtp_event_receiver(&mut self, rx: mpsc::UnboundedReceiver<RtpEvent>) {
        self.rtp_event_rx = Some(rx);
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("Starting B2BUA service");

        // Start SIP event processing
        if let Some(sip_rx) = self.sip_event_rx.take() {
            let calls_sip = Arc::clone(&self.calls);
            let event_tx_sip = self.event_tx.clone();
            let config_sip = self.config.clone();
            let sip_handler_sip = Arc::clone(&self.sip_handler);
            let rtp_handler_sip = Arc::clone(&self.rtp_handler);

            tokio::spawn(async move {
                Self::process_sip_events(
                    sip_rx,
                    calls_sip,
                    event_tx_sip,
                    config_sip,
                    sip_handler_sip,
                    rtp_handler_sip,
                ).await;
            });
        }

        // Start RTP event processing
        if let Some(rtp_rx) = self.rtp_event_rx.take() {
            let calls_rtp = Arc::clone(&self.calls);
            let media_relays_rtp = Arc::clone(&self.media_relays);
            let event_tx_rtp = self.event_tx.clone();

            tokio::spawn(async move {
                Self::process_rtp_events(rtp_rx, calls_rtp, media_relays_rtp, event_tx_rtp).await;
            });
        }

        // Start call monitoring
        let calls_monitor = Arc::clone(&self.calls);
        let event_tx_monitor = self.event_tx.clone();
        let call_timeout = Duration::from_secs(self.config.call_timeout as u64);

        tokio::spawn(async move {
            Self::call_monitor_loop(calls_monitor, event_tx_monitor, call_timeout).await;
        });

        // Start media relay monitoring
        let media_relays_monitor = Arc::clone(&self.media_relays);
        let event_tx_media = self.event_tx.clone();
        let media_timeout = Duration::from_secs(self.config.media_timeout as u64);

        tokio::spawn(async move {
            Self::media_monitor_loop(media_relays_monitor, event_tx_media, media_timeout).await;
        });

        self.is_running = true;
        info!("B2BUA service started successfully");
        Ok(())
    }

    async fn process_sip_events(
        mut sip_rx: mpsc::UnboundedReceiver<SipEvent>,
        calls: Arc<DashMap<String, B2buaCall>>,
        event_tx: mpsc::UnboundedSender<B2buaEvent>,
        config: B2buaConfig,
        sip_handler: Arc<RwLock<SipHandler>>,
        rtp_handler: Arc<RwLock<RtpHandler>>,
    ) {
        while let Some(event) = sip_rx.recv().await {
            match event {
                SipEvent::IncomingCall { session_id, call_id: _, from, to, sdp } => {
                    if let Err(e) = Self::handle_incoming_call(
                        session_id,
                        from,
                        to,
                        sdp,
                        &calls,
                        &event_tx,
                        &config,
                        &sip_handler,
                        &rtp_handler,
                    ).await {
                        error!("Failed to handle incoming call: {}", e);
                    }
                }
                SipEvent::CallAnswered { session_id, sdp } => {
                    if let Err(e) = Self::handle_call_answered(
                        session_id,
                        sdp,
                        &calls,
                        &event_tx,
                        &sip_handler,
                    ).await {
                        error!("Failed to handle call answered: {}", e);
                    }
                }
                SipEvent::CallTerminated { session_id, reason } => {
                    if let Err(e) = Self::handle_call_terminated(
                        session_id,
                        reason,
                        &calls,
                        &event_tx,
                        &sip_handler,
                    ).await {
                        error!("Failed to handle call terminated: {}", e);
                    }
                }
                _ => {
                    trace!("Unhandled SIP event: {:?}", event);
                }
            }
        }
    }

    async fn process_rtp_events(
        mut rtp_rx: mpsc::UnboundedReceiver<RtpEvent>,
        calls: Arc<DashMap<String, B2buaCall>>,
        media_relays: Arc<DashMap<String, MediaRelay>>,
        event_tx: mpsc::UnboundedSender<B2buaEvent>,
    ) {
        while let Some(event) = rtp_rx.recv().await {
            match event {
                RtpEvent::PacketReceived { session_id, packet, source: _ } => {
                    if let Err(e) = Self::handle_rtp_packet(
                        session_id,
                        packet,
                        &calls,
                        &media_relays,
                    ).await {
                        error!("Failed to handle RTP packet: {}", e);
                    }
                }
                RtpEvent::StreamStatistics { session_id: _, stats: _ } => {
                    // Update media relay statistics
                    // Implementation would update media relay stats
                }
                _ => {
                    trace!("Unhandled RTP event: {:?}", event);
                }
            }
        }
    }

    async fn handle_incoming_call(
        session_id: String,
        from: String,
        to: String,
        sdp: Option<String>,
        calls: &Arc<DashMap<String, B2buaCall>>,
        event_tx: &mpsc::UnboundedSender<B2buaEvent>,
        config: &B2buaConfig,
        sip_handler: &Arc<RwLock<SipHandler>>,
        rtp_handler: &Arc<RwLock<RtpHandler>>,
    ) -> Result<()> {
        // Check concurrent call limit
        if calls.len() >= config.max_concurrent_calls as usize {
            warn!("Maximum concurrent calls reached, rejecting call");
            return Err(Error::b2bua("Maximum concurrent calls reached"));
        }

        // Extract caller and callee information
        let caller = Self::extract_user_from_uri(&from)?;
        let callee = Self::extract_user_from_uri(&to)?;

        // Determine routing for this call
        let routing_info = Self::determine_routing(&callee, config)?;

        // Create B2BUA call
        let call_id = Uuid::new_v4().to_string();
        let call = B2buaCall {
            id: call_id.clone(),
            state: B2buaCallState::Establishing,
            leg_a_session_id: session_id,
            leg_b_session_id: None,
            leg_a_rtp_session_id: None,
            leg_b_rtp_session_id: None,
            caller: caller.clone(),
            callee: callee.clone(),
            destination_uri: Self::build_destination_uri(&callee, &routing_info)?,
            created_at: Instant::now(),
            connected_at: None,
            terminated_at: None,
            last_activity: Instant::now(),
            call_duration: None,
            routing_info: routing_info.clone(),
        };

        calls.insert(call_id.clone(), call);

        // Emit routing decision event
        let _ = event_tx.send(B2buaEvent::RoutingDecision {
            call_id: call_id.clone(),
            caller: caller.clone(),
            callee: callee.clone(),
            route: routing_info.clone(),
        });

        // Emit call establishing event
        let _ = event_tx.send(B2buaEvent::CallEstablishing {
            call_id: call_id.clone(),
            caller: caller.clone(),
            callee: callee.clone(),
        });

        // Set up media relay if enabled
        if config.enable_media_relay {
            Self::setup_media_relay(
                &call_id,
                sdp.as_deref(),
                rtp_handler,
                event_tx,
            ).await?;
        }

        // Initiate outbound call (leg B)
        Self::initiate_outbound_call(
            &call_id,
            &caller,
            &callee,
            &routing_info,
            sdp.as_deref(),
            calls,
            sip_handler,
        ).await?;

        info!("B2BUA call established: {} -> {}", caller, callee);
        Ok(())
    }

    async fn handle_call_answered(
        session_id: String,
        sdp: Option<String>,
        calls: &Arc<DashMap<String, B2buaCall>>,
        event_tx: &mpsc::UnboundedSender<B2buaEvent>,
        sip_handler: &Arc<RwLock<SipHandler>>,
    ) -> Result<()> {
        // Find call by session ID
        let call_id = {
            let mut found_call_id = None;
            for call_entry in calls.iter() {
                let call = call_entry.value();
                if call.leg_b_session_id.as_ref() == Some(&session_id) {
                    found_call_id = Some(call.id.clone());
                    break;
                }
            }
            found_call_id
        };

        if let Some(call_id) = call_id {
            // Update call state
            if let Some(mut call) = calls.get_mut(&call_id) {
                call.state = B2buaCallState::Connected;
                call.connected_at = Some(Instant::now());
                call.last_activity = Instant::now();

                let duration_to_connect = call.connected_at.unwrap()
                    .duration_since(call.created_at);

                // Send 200 OK to leg A
                let sip_handler = sip_handler.read().await;
                sip_handler.send_response(
                    &call.leg_a_session_id,
                    200,
                    "OK",
                    sdp.as_deref(),
                ).await?;

                // Emit call connected event
                let _ = event_tx.send(B2buaEvent::CallConnected {
                    call_id: call_id.clone(),
                    duration_to_connect,
                });

                info!("B2BUA call connected: {}", call_id);
            }
        }

        Ok(())
    }

    async fn handle_call_terminated(
        session_id: String,
        reason: String,
        calls: &Arc<DashMap<String, B2buaCall>>,
        event_tx: &mpsc::UnboundedSender<B2buaEvent>,
        sip_handler: &Arc<RwLock<SipHandler>>,
    ) -> Result<()> {
        // Find and terminate call
        let call_to_terminate = {
            let mut found_call = None;
            for call_entry in calls.iter() {
                let call = call_entry.value();
                if call.leg_a_session_id == session_id || 
                   call.leg_b_session_id.as_ref() == Some(&session_id) {
                    found_call = Some(call.clone());
                    break;
                }
            }
            found_call
        };

        if let Some(call) = call_to_terminate {
            // Terminate both legs
            let sip_handler = sip_handler.read().await;
            
            // Terminate the other leg
            if call.leg_a_session_id != session_id {
                // Terminate leg A
                // Implementation would send BYE to leg A
            } else if let Some(leg_b_session_id) = &call.leg_b_session_id {
                if leg_b_session_id != &session_id {
                    // Terminate leg B
                    // Implementation would send BYE to leg B
                }
            }

            // Calculate call duration
            let duration = call.connected_at.map(|connected| {
                Instant::now().duration_since(connected)
            });

            // Remove call from active calls
            calls.remove(&call.id);

            // Emit call terminated event
            let _ = event_tx.send(B2buaEvent::CallTerminated {
                call_id: call.id.clone(),
                reason,
                duration,
            });

            info!("B2BUA call terminated: {} (duration: {:?})", call.id, duration);
        }

        Ok(())
    }

    async fn handle_rtp_packet(
        session_id: String,
        packet: crate::protocols::rtp::RtpPacket,
        calls: &Arc<DashMap<String, B2buaCall>>,
        media_relays: &Arc<DashMap<String, MediaRelay>>,
    ) -> Result<()> {
        // Find call and relay packet to the other leg
        for call_entry in calls.iter() {
            let call = call_entry.value();
            
            // Determine relay direction
            let relay_to_session = if call.leg_a_rtp_session_id.as_ref() == Some(&session_id) {
                // Packet from leg A, relay to leg B
                call.leg_b_rtp_session_id.as_ref()
            } else if call.leg_b_rtp_session_id.as_ref() == Some(&session_id) {
                // Packet from leg B, relay to leg A
                call.leg_a_rtp_session_id.as_ref()
            } else {
                continue;
            };

            if let Some(target_session) = relay_to_session {
                // Update media relay statistics
                if let Some(mut relay) = media_relays.get_mut(&call.id) {
                    relay.last_activity = Instant::now();
                    
                    if call.leg_a_rtp_session_id.as_ref() == Some(&session_id) {
                        relay.packets_relayed_a_to_b += 1;
                        relay.bytes_relayed_a_to_b += packet.payload.len() as u64;
                    } else {
                        relay.packets_relayed_b_to_a += 1;
                        relay.bytes_relayed_b_to_a += packet.payload.len() as u64;
                    }
                }

                // Relay packet (implementation would forward to RTP handler)
                trace!("Relaying RTP packet from {} to {} for call {}",
                    session_id, target_session, call.id);
            }
            
            break;
        }

        Ok(())
    }

    async fn setup_media_relay(
        call_id: &str,
        sdp: Option<&str>,
        rtp_handler: &Arc<RwLock<RtpHandler>>,
        event_tx: &mpsc::UnboundedSender<B2buaEvent>,
    ) -> Result<()> {
        let rtp_handler = rtp_handler.read().await;
        
        // Create RTP sessions for both legs
        let leg_a_session = rtp_handler.create_session(
            format!("{}_leg_a", call_id),
            0, // payload type will be determined from SDP
        ).await?;

        let leg_b_session = rtp_handler.create_session(
            format!("{}_leg_b", call_id),
            0,
        ).await?;

        // Emit media relay started event
        let _ = event_tx.send(B2buaEvent::MediaRelayStarted {
            call_id: call_id.to_string(),
            leg_a_port: leg_a_session.local_port,
            leg_b_port: leg_b_session.local_port,
        });

        info!("Media relay set up for call {}: ports {} <-> {}",
            call_id, leg_a_session.local_port, leg_b_session.local_port);

        Ok(())
    }

    async fn initiate_outbound_call(
        call_id: &str,
        caller: &str,
        callee: &str,
        routing_info: &RoutingInfo,
        sdp: Option<&str>,
        calls: &Arc<DashMap<String, B2buaCall>>,
        sip_handler: &Arc<RwLock<SipHandler>>,
    ) -> Result<()> {
        let destination_uri = Self::build_destination_uri(callee, routing_info)?;
        let from_uri = format!("sip:{}@gateway", caller);

        // Initiate outbound SIP call
        let sip_handler = sip_handler.read().await;
        let target_addr = Self::resolve_target_address(&destination_uri).await?;
        
        let leg_b_session_id = sip_handler.send_invite(
            &destination_uri,
            &from_uri,
            sdp,
            target_addr,
        ).await?;

        // Update call with leg B session ID
        if let Some(mut call) = calls.get_mut(call_id) {
            call.leg_b_session_id = Some(leg_b_session_id);
            call.last_activity = Instant::now();
        }

        info!("Initiated outbound call leg B for call {}: {} -> {}",
            call_id, caller, destination_uri);

        Ok(())
    }

    async fn call_monitor_loop(
        calls: Arc<DashMap<String, B2buaCall>>,
        event_tx: mpsc::UnboundedSender<B2buaEvent>,
        timeout: Duration,
    ) {
        let mut monitor_interval = interval(Duration::from_secs(30));

        loop {
            monitor_interval.tick().await;
            let now = Instant::now();

            let timed_out_calls: Vec<String> = calls
                .iter()
                .filter(|entry| {
                    let call = entry.value();
                    now.duration_since(call.last_activity) > timeout
                })
                .map(|entry| entry.key().clone())
                .collect();

            for call_id in timed_out_calls {
                if let Some((_, call)) = calls.remove(&call_id) {
                    info!("B2BUA call timed out: {}", call_id);
                    let _ = event_tx.send(B2buaEvent::CallTerminated {
                        call_id,
                        reason: "Call timeout".to_string(),
                        duration: call.connected_at.map(|connected| {
                            now.duration_since(connected)
                        }),
                    });
                }
            }
        }
    }

    async fn media_monitor_loop(
        media_relays: Arc<DashMap<String, MediaRelay>>,
        event_tx: mpsc::UnboundedSender<B2buaEvent>,
        timeout: Duration,
    ) {
        let mut monitor_interval = interval(Duration::from_secs(10));

        loop {
            monitor_interval.tick().await;
            let now = Instant::now();

            // Report media statistics
            for relay_entry in media_relays.iter() {
                let relay = relay_entry.value();
                let _ = event_tx.send(B2buaEvent::MediaRelayStats {
                    call_id: relay.call_id.clone(),
                    stats: relay.clone(),
                });
            }

            // Clean up inactive relays
            let inactive_relays: Vec<String> = media_relays
                .iter()
                .filter(|entry| {
                    now.duration_since(entry.value().last_activity) > timeout
                })
                .map(|entry| entry.key().clone())
                .collect();

            for call_id in inactive_relays {
                media_relays.remove(&call_id);
                debug!("Cleaned up inactive media relay: {}", call_id);
            }
        }
    }

    // Helper methods
    fn extract_user_from_uri(uri: &str) -> Result<String> {
        // Extract user portion from SIP URI
        if let Some(start) = uri.find("sip:") {
            let after_sip = &uri[start + 4..];
            if let Some(end) = after_sip.find('@') {
                return Ok(after_sip[..end].to_string());
            }
        }
        Err(Error::parse("Invalid SIP URI format"))
    }

    fn determine_routing(callee: &str, config: &B2buaConfig) -> Result<RoutingInfo> {
        // Simple routing logic - in practice this would be more sophisticated
        for rule in &config.routing_table {
            if callee.contains(&rule.pattern) {
                return Ok(RoutingInfo {
                    route_type: rule.route_type.clone(),
                    target_gateway: Some(rule.target.clone()),
                    number_translation: rule.translation.clone(),
                    codec_preference: vec!["PCMU".to_string(), "PCMA".to_string()],
                    priority: rule.priority,
                });
            }
        }

        // Default routing
        Ok(RoutingInfo {
            route_type: RouteType::Direct,
            target_gateway: config.default_route_gateway.clone(),
            number_translation: None,
            codec_preference: vec!["PCMU".to_string(), "PCMA".to_string()],
            priority: 100,
        })
    }

    fn build_destination_uri(callee: &str, routing_info: &RoutingInfo) -> Result<String> {
        let mut target_number = callee.to_string();

        // Apply number translation if configured
        if let Some(translation) = &routing_info.number_translation {
            if let Some(prefix_strip) = &translation.prefix_strip {
                if target_number.starts_with(prefix_strip) {
                    target_number = target_number[prefix_strip.len()..].to_string();
                }
            }
            if let Some(prefix_add) = &translation.prefix_add {
                target_number = format!("{}{}", prefix_add, target_number);
            }
        }

        match &routing_info.target_gateway {
            Some(gateway) => Ok(format!("sip:{}@{}", target_number, gateway)),
            None => Ok(format!("sip:{}@localhost", target_number)),
        }
    }

    async fn resolve_target_address(uri: &str) -> Result<SocketAddr> {
        // Simple resolution - in practice would use DNS resolution
        if let Some(at_pos) = uri.find('@') {
            let host_part = &uri[at_pos + 1..];
            let host = host_part.split(':').next().unwrap_or(host_part);
            
            // For demo purposes, use localhost
            let port = 5060;
            let addr = format!("127.0.0.1:{}", port);
            
            addr.parse().map_err(|e| Error::parse(&format!("Invalid address: {}", e)))
        } else {
            Err(Error::parse("Invalid URI format"))
        }
    }

    // Public API methods
    pub fn get_active_calls(&self) -> Vec<B2buaCall> {
        self.calls.iter().map(|entry| entry.value().clone()).collect()
    }

    pub fn get_call(&self, call_id: &str) -> Option<B2buaCall> {
        self.calls.get(call_id).map(|entry| entry.value().clone())
    }

    pub fn get_active_call_count(&self) -> usize {
        self.calls.len()
    }

    pub fn get_media_relay_stats(&self, call_id: &str) -> Option<MediaRelay> {
        self.media_relays.get(call_id).map(|entry| entry.value().clone())
    }

    pub async fn terminate_call(&self, call_id: &str, reason: &str) -> Result<()> {
        if let Some((_, call)) = self.calls.remove(call_id) {
            // Terminate both legs
            let sip_handler = self.sip_handler.read().await;
            
            // Send BYE to both legs (implementation would handle this)
            // This is simplified for the core structure
            
            let _ = self.event_tx.send(B2buaEvent::CallTerminated {
                call_id: call_id.to_string(),
                reason: reason.to_string(),
                duration: call.connected_at.map(|connected| {
                    Instant::now().duration_since(connected)
                }),
            });

            info!("Manually terminated B2BUA call: {} ({})", call_id, reason);
            Ok(())
        } else {
            Err(Error::b2bua("Call not found"))
        }
    }

    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping B2BUA service");
        
        // Terminate all active calls
        let active_calls: Vec<String> = self.calls.iter().map(|entry| entry.key().clone()).collect();
        for call_id in active_calls {
            let _ = self.terminate_call(&call_id, "Service shutdown").await;
        }

        self.calls.clear();
        self.media_relays.clear();
        self.is_running = false;
        
        info!("B2BUA service stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PortRange, SipConfig, SipTransport};

    #[tokio::test]
    async fn test_b2bua_service_creation() {
        let sip_config = SipConfig {
            listen_port: 0,
            domain: "test.local".to_string(),
            transport: SipTransport::Udp,
            max_sessions: 100,
            session_timeout: 300,
            register_interval: 3600,
        };

        let rtp_config = PortRange { min: 10000, max: 10100 };
        
        let sip_handler = Arc::new(RwLock::new(
            SipHandler::new(sip_config).await.unwrap()
        ));
        
        let rtp_handler = Arc::new(RwLock::new(
            RtpHandler::new(rtp_config).unwrap()
        ));

        let b2bua_config = B2buaConfig {
            max_concurrent_calls: 100,
            call_timeout: 300,
            media_timeout: 60,
            default_route_gateway: None,
            enable_media_relay: true,
            enable_codec_transcoding: false,
            routing_table: vec![],
        };

        let service = B2buaService::new(b2bua_config, sip_handler, rtp_handler);
        assert!(service.is_ok());
    }

    #[test]
    fn test_uri_extraction() {
        let uri = "sip:1234@example.com";
        let user = B2buaService::extract_user_from_uri(uri).unwrap();
        assert_eq!(user, "1234");
    }

    #[test]
    fn test_routing_determination() {
        let config = B2buaConfig {
            max_concurrent_calls: 100,
            call_timeout: 300,
            media_timeout: 60,
            default_route_gateway: Some("gateway.example.com".to_string()),
            enable_media_relay: true,
            enable_codec_transcoding: false,
            routing_table: vec![
                RoutingRule {
                    pattern: "911".to_string(),
                    route_type: RouteType::Emergency,
                    target: "emergency.psap.com".to_string(),
                    priority: 1,
                    translation: None,
                }
            ],
        };

        let routing = B2buaService::determine_routing("911", &config).unwrap();
        assert!(matches!(routing.route_type, RouteType::Emergency));
        assert_eq!(routing.target_gateway, Some("emergency.psap.com".to_string()));
    }
}