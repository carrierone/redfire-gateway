//! Main gateway orchestrator for the Redfire Gateway

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::config::{GatewayConfig, PerformanceConfig, SnmpConfig};
use crate::interfaces::{TdmoeInterface, FreeTdmInterface};
use crate::protocols::{SipHandler, RtpHandler};
use crate::services::{
    PerformanceMonitor, AlarmManager, TestingService, AutoDetectionService,
    SnmpService, DebugService, InterfaceTestingService, TestAutomationService,
    TimingService, TimingConfig,
};
use crate::services::{
    alarms::AlarmConfig, auto_detection::AutoDetectionConfig, debug::DebugConfig,
    testing::TestingConfig,
};
use crate::Result;

/// Gateway status information
#[derive(Debug, Clone)]
pub struct GatewayStatus {
    pub running: bool,
    pub uptime: Duration,
    pub interfaces: InterfaceStatus,
    pub protocols: ProtocolStatus,
    pub sessions: SessionStatus,
}

#[derive(Debug, Clone)]
pub struct InterfaceStatus {
    pub tdmoe: String,
    pub freetdm: String,
}

#[derive(Debug, Clone)]
pub struct ProtocolStatus {
    pub sip: String,
    pub rtp: String,
}

#[derive(Debug, Clone)]
pub struct SessionStatus {
    pub active_calls: u32,
    pub active_channels: u32,
    pub sip_sessions: u32,
    pub rtp_sessions: u32,
}

/// Gateway events
#[derive(Debug, Clone)]
pub enum GatewayEvent {
    Started,
    Stopped,
    InterfaceUp { interface: String },
    InterfaceDown { interface: String },
    CallStarted { call_id: String },
    CallEnded { call_id: String },
    Error { message: String },
}

/// Main Redfire Gateway implementation
pub struct RedFireGateway {
    config: GatewayConfig,
    
    // Interfaces
    tdmoe_interface: Option<TdmoeInterface>,
    freetdm_interface: Option<FreeTdmInterface>,
    
    // Protocol handlers
    sip_handler: Option<SipHandler>,
    rtp_handler: Option<RtpHandler>,
    
    // Services
    performance_monitor: Option<PerformanceMonitor>,
    alarm_manager: Option<AlarmManager>,
    testing_service: Option<TestingService>,
    auto_detection_service: Option<AutoDetectionService>,
    snmp_service: Option<SnmpService>,
    debug_service: Option<DebugService>,
    interface_testing_service: Option<InterfaceTestingService>,
    test_automation_service: Option<TestAutomationService>,
    timing_service: Option<TimingService>,
    
    // Event handling
    event_tx: mpsc::UnboundedSender<GatewayEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<GatewayEvent>>,
    
    // Runtime state
    is_running: Arc<RwLock<bool>>,
    start_time: Option<std::time::Instant>,
    
    // Background tasks
    tasks: Vec<JoinHandle<()>>,
}

impl RedFireGateway {
    pub fn new(config: GatewayConfig) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        Ok(Self {
            config,
            tdmoe_interface: None,
            freetdm_interface: None,
            sip_handler: None,
            rtp_handler: None,
            performance_monitor: None,
            alarm_manager: None,
            testing_service: None,
            auto_detection_service: None,
            snmp_service: None,
            debug_service: None,
            interface_testing_service: None,
            test_automation_service: None,
            timing_service: None,
            event_tx,
            event_rx: Some(event_rx),
            is_running: Arc::new(RwLock::new(false)),
            start_time: None,
            tasks: Vec::new(),
        })
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<GatewayEvent>> {
        self.event_rx.take()
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("Starting Redfire Gateway");
        
        // Initialize interfaces
        self.initialize_interfaces().await?;
        
        // Initialize protocol handlers
        self.initialize_protocols().await?;
        
        // Initialize services
        self.initialize_services().await?;
        
        // Start all components
        self.start_components().await?;
        
        // Setup event handling
        self.setup_event_handlers().await?;
        
        // Mark as running
        {
            let mut is_running = self.is_running.write().await;
            *is_running = true;
        }
        self.start_time = Some(std::time::Instant::now());
        
        let _ = self.event_tx.send(GatewayEvent::Started);
        info!("Redfire Gateway started successfully");
        Ok(())
    }

    async fn initialize_interfaces(&mut self) -> Result<()> {
        info!("Initializing interfaces");
        
        // Initialize TDMoE interface
        let tdmoe_config = crate::interfaces::tdmoe::TdmoeConfig {
            interface: self.config.tdmoe.interface.clone(),
            bind_port: 2427,
            remote_addr: None,
            channels: self.config.tdmoe.channels,
            keepalive_interval: Duration::from_secs(30),
            frame_timeout: Duration::from_secs(10),
            max_retries: 3,
        };
        
        let tdmoe_interface = TdmoeInterface::new(tdmoe_config).await?;
        self.tdmoe_interface = Some(tdmoe_interface);
        
        // Initialize FreeTDM interface if enabled
        if self.config.freetdm.enabled {
            let freetdm_interface = FreeTdmInterface::new(self.config.freetdm.clone())?;
            self.freetdm_interface = Some(freetdm_interface);
        }
        
        info!("Interfaces initialized");
        Ok(())
    }

    async fn initialize_protocols(&mut self) -> Result<()> {
        info!("Initializing protocol handlers");
        
        // Initialize SIP handler
        let sip_handler = SipHandler::new(self.config.sip.clone()).await?;
        self.sip_handler = Some(sip_handler);
        
        // Initialize RTP handler
        let rtp_handler = RtpHandler::new(self.config.rtp.port_range.clone())?;
        self.rtp_handler = Some(rtp_handler);
        
        info!("Protocol handlers initialized");
        Ok(())
    }

    async fn initialize_services(&mut self) -> Result<()> {
        info!("Initializing services");
        
        // Initialize Timing Service
        let timing_config = TimingConfig::default();
        let mut timing_service = TimingService::new(timing_config);
        timing_service.start().await?;
        self.timing_service = Some(timing_service);
        
        // Initialize Performance Monitor
        let performance_config = PerformanceConfig::default();
        let performance_monitor = PerformanceMonitor::new(performance_config)?;
        self.performance_monitor = Some(performance_monitor);
        
        // Initialize Alarm Manager
        let alarm_config = AlarmConfig::default();
        let alarm_manager = AlarmManager::new(alarm_config);
        self.alarm_manager = Some(alarm_manager);
        
        // Initialize Testing Service
        let testing_config = TestingConfig::default();
        let testing_service = TestingService::new(testing_config);
        self.testing_service = Some(testing_service);
        
        // Initialize Auto Detection Service
        let auto_detection_config = AutoDetectionConfig::default();
        let auto_detection_service = AutoDetectionService::new(auto_detection_config);
        self.auto_detection_service = Some(auto_detection_service);
        
        // Initialize SNMP Service
        let snmp_config = SnmpConfig::default();
        let snmp_service = SnmpService::new(snmp_config);
        self.snmp_service = Some(snmp_service);
        
        // Initialize Debug Service
        let debug_config = DebugConfig::default();
        let debug_service = DebugService::new(debug_config);
        self.debug_service = Some(debug_service);
        
        // Initialize Interface Testing Service
        let interface_testing_service = InterfaceTestingService::new();
        self.interface_testing_service = Some(interface_testing_service);
        
        // Initialize Test Automation Service
        if let Some(ref interface_testing) = self.interface_testing_service {
            let test_automation_service = TestAutomationService::new(
                std::sync::Arc::new(interface_testing.clone())
            );
            self.test_automation_service = Some(test_automation_service);
        }
        
        info!("Services initialized");
        Ok(())
    }

    async fn start_components(&mut self) -> Result<()> {
        info!("Starting components");
        
        // Start TDMoE interface
        if let Some(ref mut tdmoe) = self.tdmoe_interface {
            tdmoe.start().await?;
            let _ = self.event_tx.send(GatewayEvent::InterfaceUp {
                interface: "TDMoE".to_string(),
            });
        }
        
        // Start FreeTDM interface
        if let Some(ref mut freetdm) = self.freetdm_interface {
            freetdm.start().await?;
            if freetdm.is_running() {
                let _ = self.event_tx.send(GatewayEvent::InterfaceUp {
                    interface: "FreeTDM".to_string(),
                });
            }
        }
        
        // Start SIP handler
        if let Some(ref mut sip) = self.sip_handler {
            sip.start().await?;
        }
        
        // Start RTP handler
        if let Some(ref mut rtp) = self.rtp_handler {
            rtp.start().await?;
        }
        
        // Start services
        if let Some(ref mut performance) = self.performance_monitor {
            performance.start().await?;
        }
        
        if let Some(ref mut auto_detection) = self.auto_detection_service {
            auto_detection.start().await?;
        }
        
        if let Some(ref mut snmp) = self.snmp_service {
            snmp.start().await?;
        }
        
        if let Some(ref mut debug) = self.debug_service {
            debug.start().await?;
        }
        
        info!("All components started");
        Ok(())
    }

    async fn setup_event_handlers(&mut self) -> Result<()> {
        info!("Setting up event handlers");
        
        // Handle TDMoE events
        if let Some(ref mut tdmoe) = self.tdmoe_interface {
            if let Some(mut event_rx) = tdmoe.take_event_receiver() {
                let event_tx = self.event_tx.clone();
                let task = tokio::spawn(async move {
                    while let Some(event) = event_rx.recv().await {
                        Self::handle_tdmoe_event(event, &event_tx).await;
                    }
                });
                self.tasks.push(task);
            }
        }
        
        // Handle FreeTDM events
        if let Some(ref mut freetdm) = self.freetdm_interface {
            if let Some(mut event_rx) = freetdm.take_event_receiver() {
                let event_tx = self.event_tx.clone();
                let task = tokio::spawn(async move {
                    while let Some(event) = event_rx.recv().await {
                        Self::handle_freetdm_event(event, &event_tx).await;
                    }
                });
                self.tasks.push(task);
            }
        }
        
        // Handle SIP events
        if let Some(ref mut sip) = self.sip_handler {
            if let Some(mut event_rx) = sip.take_event_receiver() {
                let event_tx = self.event_tx.clone();
                let task = tokio::spawn(async move {
                    while let Some(event) = event_rx.recv().await {
                        Self::handle_sip_event(event, &event_tx).await;
                    }
                });
                self.tasks.push(task);
            }
        }
        
        // Handle RTP events
        if let Some(ref mut rtp) = self.rtp_handler {
            if let Some(mut event_rx) = rtp.take_event_receiver() {
                let event_tx = self.event_tx.clone();
                let task = tokio::spawn(async move {
                    while let Some(event) = event_rx.recv().await {
                        Self::handle_rtp_event(event, &event_tx).await;
                    }
                });
                self.tasks.push(task);
            }
        }
        
        info!("Event handlers set up");
        Ok(())
    }

    async fn handle_tdmoe_event(
        event: crate::interfaces::tdmoe::TdmoeEvent,
        event_tx: &mpsc::UnboundedSender<GatewayEvent>,
    ) {
        use crate::interfaces::tdmoe::TdmoeEvent;
        
        match event {
            TdmoeEvent::FrameReceived { frame, source: _ } => {
                // Process TDMoE frame - in a real implementation, this would
                // route the frame to the appropriate protocol handler
                tracing::trace!("Received TDMoE frame on channel {}", frame.channel);
            }
            TdmoeEvent::ChannelStateChanged { channel, active } => {
                if active {
                    info!("TDMoE channel {} activated", channel);
                } else {
                    warn!("TDMoE channel {} deactivated", channel);
                }
            }
            TdmoeEvent::LoopbackResponse { channel, success, loopback_type: _ } => {
                info!("Loopback response for channel {}: {}", channel, if success { "OK" } else { "FAILED" });
            }
            TdmoeEvent::Error { error, channel } => {
                error!("TDMoE error on channel {:?}: {}", channel, error);
                let _ = event_tx.send(GatewayEvent::Error { 
                    message: format!("TDMoE: {}", error) 
                });
            }
        }
    }

    async fn handle_freetdm_event(
        event: crate::interfaces::freetdm::FreeTdmEvent,
        event_tx: &mpsc::UnboundedSender<GatewayEvent>,
    ) {
        use crate::interfaces::freetdm::FreeTdmEvent;
        
        match event {
            FreeTdmEvent::IncomingCall { span_id, channel_id, calling_number, called_number } => {
                info!("Incoming call on span {}, channel {}: {} -> {:?}", 
                    span_id, channel_id, calling_number.unwrap_or_default(), called_number);
                
                let call_id = format!("ftdm-{}-{}", span_id, channel_id);
                let _ = event_tx.send(GatewayEvent::CallStarted { call_id });
            }
            FreeTdmEvent::CallAnswered { span_id, channel_id } => {
                info!("Call answered on span {}, channel {}", span_id, channel_id);
            }
            FreeTdmEvent::CallHangup { span_id, channel_id, cause } => {
                info!("Call hangup on span {}, channel {} (cause: {})", span_id, channel_id, cause);
                
                let call_id = format!("ftdm-{}-{}", span_id, channel_id);
                let _ = event_tx.send(GatewayEvent::CallEnded { call_id });
            }
            FreeTdmEvent::Alarm { span_id, message, severity: _ } => {
                warn!("FreeTDM alarm on span {}: {}", span_id, message);
                let _ = event_tx.send(GatewayEvent::Error { 
                    message: format!("FreeTDM span {}: {}", span_id, message) 
                });
            }
            FreeTdmEvent::SpanUp { span_id } => {
                info!("FreeTDM span {} is UP", span_id);
            }
            FreeTdmEvent::SpanDown { span_id } => {
                warn!("FreeTDM span {} is DOWN", span_id);
                let _ = event_tx.send(GatewayEvent::InterfaceDown {
                    interface: format!("FreeTDM-Span-{}", span_id),
                });
            }
        }
    }

    async fn handle_sip_event(
        event: crate::protocols::sip::SipEvent,
        event_tx: &mpsc::UnboundedSender<GatewayEvent>,
    ) {
        use crate::protocols::sip::SipEvent;
        
        match event {
            SipEvent::IncomingCall { session_id, call_id, from, to, sdp: _ } => {
                info!("Incoming SIP call: {} ({} -> {})", call_id, from, to);
                let _ = event_tx.send(GatewayEvent::CallStarted { call_id: session_id });
            }
            SipEvent::CallRinging { session_id: _ } => {
                // Call is ringing
            }
            SipEvent::CallAnswered { session_id: _, sdp: _ } => {
                info!("SIP call answered");
            }
            SipEvent::CallTerminated { session_id, reason } => {
                info!("SIP call terminated: {}", reason);
                let _ = event_tx.send(GatewayEvent::CallEnded { call_id: session_id });
            }
            SipEvent::DtmfReceived { session_id: _, digit, duration: _ } => {
                tracing::debug!("DTMF received: {}", digit);
            }
            SipEvent::RegistrationReceived { user, contact: _, expires: _ } => {
                info!("SIP registration received for user: {}", user);
            }
            SipEvent::Started { listen_address } => {
                info!("SIP handler started on {}", listen_address);
            }
            SipEvent::InviteSent { session_id, target_uri } => {
                info!("SIP INVITE sent for session {} to {}", session_id, target_uri);
            }
            SipEvent::Error { message, session_id: _ } => {
                error!("SIP error: {}", message);
                let _ = event_tx.send(GatewayEvent::Error { 
                    message: format!("SIP: {}", message) 
                });
            }
        }
    }

    async fn handle_rtp_event(
        event: crate::protocols::rtp::RtpEvent,
        _event_tx: &mpsc::UnboundedSender<GatewayEvent>,
    ) {
        use crate::protocols::rtp::RtpEvent;
        
        match event {
            RtpEvent::PacketReceived { session_id: _, packet: _, source: _ } => {
                // Handle RTP packet
            }
            RtpEvent::SessionTimeout { session_id } => {
                warn!("RTP session timeout: {}", session_id);
            }
            RtpEvent::StreamStatistics { session_id: _, stats } => {
                tracing::trace!("RTP stats: packets_received={}, jitter={:.2}", 
                    stats.packets_received, stats.jitter);
            }
            RtpEvent::Error { message, session_id } => {
                error!("RTP error (session {:?}): {}", session_id, message);
            }
        }
    }

    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping Redfire Gateway");
        
        // Mark as not running
        {
            let mut is_running = self.is_running.write().await;
            *is_running = false;
        }
        
        // Cancel background tasks
        for task in self.tasks.drain(..) {
            task.abort();
        }
        
        // Stop all components
        if let Some(ref mut rtp) = self.rtp_handler {
            if let Err(e) = rtp.stop().await {
                error!("Error stopping RTP handler: {}", e);
            }
        }
        
        if let Some(ref mut sip) = self.sip_handler {
            if let Err(e) = sip.stop().await {
                error!("Error stopping SIP handler: {}", e);
            }
        }
        
        if let Some(ref mut freetdm) = self.freetdm_interface {
            if let Err(e) = freetdm.stop().await {
                error!("Error stopping FreeTDM interface: {}", e);
            }
        }
        
        if let Some(ref tdmoe) = self.tdmoe_interface {
            if let Err(e) = tdmoe.stop().await {
                error!("Error stopping TDMoE interface: {}", e);
            }
        }
        
        let _ = self.event_tx.send(GatewayEvent::Stopped);
        info!("Redfire Gateway stopped");
        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    pub async fn get_status(&self) -> GatewayStatus {
        let is_running = self.is_running().await;
        let uptime = self.start_time
            .map(|start| start.elapsed())
            .unwrap_or_default();

        let interfaces = InterfaceStatus {
            tdmoe: if self.tdmoe_interface.is_some() { "running" } else { "disabled" }.to_string(),
            freetdm: if let Some(ref freetdm) = self.freetdm_interface {
                if freetdm.is_running() { "running" } else { "stopped" }.to_string()
            } else {
                "disabled".to_string()
            },
        };

        let protocols = ProtocolStatus {
            sip: if self.sip_handler.is_some() { "running" } else { "disabled" }.to_string(),
            rtp: if self.rtp_handler.is_some() { "running" } else { "disabled" }.to_string(),
        };

        let sessions = SessionStatus {
            active_calls: 0, // Would be calculated from actual sessions
            active_channels: self.get_active_channel_count().await,
            sip_sessions: self.sip_handler.as_ref()
                .map(|h| h.get_active_session_count() as u32)
                .unwrap_or(0),
            rtp_sessions: self.rtp_handler.as_ref()
                .map(|h| h.get_active_session_count() as u32)
                .unwrap_or(0),
        };

        GatewayStatus {
            running: is_running,
            uptime,
            interfaces,
            protocols,
            sessions,
        }
    }

    async fn get_active_channel_count(&self) -> u32 {
        let mut count = 0;
        
        if let Some(ref tdmoe) = self.tdmoe_interface {
            count += tdmoe.get_active_channels().len() as u32;
        }
        
        if let Some(ref freetdm) = self.freetdm_interface {
            count += freetdm.get_active_channel_count();
        }
        
        count
    }

    pub fn get_config(&self) -> &GatewayConfig {
        &self.config
    }

    pub async fn reload_config(&mut self, new_config: GatewayConfig) -> Result<()> {
        info!("Reloading gateway configuration");
        
        // Validate new configuration
        new_config.validate()?;
        
        // For now, we require a restart to apply new configuration
        // In a production system, you might want to support hot-reloading
        // of certain configuration parameters
        
        self.config = new_config;
        info!("Configuration reloaded (restart required for full effect)");
        Ok(())
    }

    // Placeholder methods for call routing - these would contain the actual
    // protocol translation logic in a real implementation
    
    pub async fn route_tdm_to_sip(&self, _channel: u16, _data: &[u8]) -> Result<()> {
        // Convert TDM frame to SIP/RTP
        // 1. Decode TDM data
        // 2. Convert to appropriate codec
        // 3. Send via RTP
        Ok(())
    }

    pub async fn route_sip_to_tdm(&self, _session_id: &str, _rtp_data: &[u8]) -> Result<()> {
        // Convert RTP to TDM frame
        // 1. Decode RTP packet
        // 2. Convert codec if needed
        // 3. Send via TDMoE
        Ok(())
    }
}

impl Drop for RedFireGateway {
    fn drop(&mut self) {
        // Abort any remaining tasks
        for task in self.tasks.drain(..) {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GatewayConfig;

    #[tokio::test]
    async fn test_gateway_creation() {
        let config = GatewayConfig::default_config();
        let gateway = RedFireGateway::new(config);
        assert!(gateway.is_ok());
    }

    #[tokio::test]
    async fn test_gateway_status() {
        let config = GatewayConfig::default_config();
        let gateway = RedFireGateway::new(config).unwrap();
        
        let status = gateway.get_status().await;
        assert!(!status.running);
        assert_eq!(status.uptime, Duration::ZERO);
    }
}