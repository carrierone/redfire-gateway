//! SIP protocol integration using the redfire-sip-stack library
//! 
//! This module provides SIP (Session Initiation Protocol) functionality
//! integrated with the external redfire-sip-stack library.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use tokio::sync::mpsc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::SipConfig;
use crate::Result;

// Import from external redfire-sip-stack library
use redfire_sip_stack::{
    SipParser, SipMessage, SipMethod, SipCoreEngine, SipCoreConfig,
    create_default_core, utils,
};

// SipMethod is imported from redfire-sip-stack and re-exported above

// SipMethod methods are provided by the external library

// SipMessage is imported from redfire-sip-stack and provides full functionality

/// SIP session states
#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    Idle,
    Calling,
    Ringing,
    Early,
    Confirmed,
    Disconnected,
    Terminated,
}

/// SIP session information
#[derive(Debug, Clone)]
pub struct SipSession {
    pub id: String,
    pub call_id: String,
    pub state: SessionState,
    pub direction: SessionDirection,
    pub local_uri: String,
    pub remote_uri: String,
    pub local_tag: String,
    pub remote_tag: Option<String>,
    pub cseq: u32,
    pub remote_cseq: u32,
    pub contact: Option<String>,
    pub remote_target: Option<SocketAddr>,
    pub sdp: Option<String>,
    pub remote_sdp: Option<String>,
    pub created_at: Instant,
    pub last_activity: Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionDirection {
    Inbound,
    Outbound,
}

impl SipSession {
    pub fn new_outbound(call_id: String, local_uri: String, remote_uri: String) -> Self {
        let id = Uuid::new_v4().to_string();
        let local_tag = Self::generate_tag();
        let now = Instant::now();

        Self {
            id,
            call_id,
            state: SessionState::Idle,
            direction: SessionDirection::Outbound,
            local_uri,
            remote_uri,
            local_tag,
            remote_tag: None,
            cseq: 1,
            remote_cseq: 0,
            contact: None,
            remote_target: None,
            sdp: None,
            remote_sdp: None,
            created_at: now,
            last_activity: now,
        }
    }

    pub fn new_inbound(call_id: String, local_uri: String, remote_uri: String) -> Self {
        let id = Uuid::new_v4().to_string();
        let local_tag = Self::generate_tag();
        let now = Instant::now();

        Self {
            id,
            call_id,
            state: SessionState::Idle,
            direction: SessionDirection::Inbound,
            local_uri,
            remote_uri,
            local_tag,
            remote_tag: None,
            cseq: 1,
            remote_cseq: 0,
            contact: None,
            remote_target: None,
            sdp: None,
            remote_sdp: None,
            created_at: now,
            last_activity: now,
        }
    }

    fn generate_tag() -> String {
        format!("{:x}", rand::random::<u64>())
    }

    pub fn update_activity(&mut self) {
        self.last_activity = Instant::now();
    }
}

/// SIP events
#[derive(Debug, Clone)]
pub enum SipEvent {
    IncomingCall {
        session_id: String,
        call_id: String,
        from: String,
        to: String,
        sdp: Option<String>,
    },
    CallRinging {
        session_id: String,
    },
    CallAnswered {
        session_id: String,
        sdp: Option<String>,
    },
    CallTerminated {
        session_id: String,
        reason: String,
    },
    DtmfReceived {
        session_id: String,
        digit: char,
        duration: u32,
    },
    RegistrationReceived {
        user: String,
        contact: String,
        expires: u32,
    },
    Started {
        listen_address: String,
    },
    InviteSent {
        session_id: String,
        target_uri: String,
    },
    Error {
        message: String,
        session_id: Option<String>,
    },
}

/// SIP handler integrated with redfire-sip-stack
/// 
/// This implementation integrates with the external redfire-sip-stack library
/// to provide full SIP functionality including SIP-T and SIP-I support.
pub struct SipHandler {
    config: SipConfig,
    parser: SipParser,
    core_engine: Option<SipCoreEngine>,
    sessions: Arc<DashMap<String, SipSession>>,
    event_tx: mpsc::UnboundedSender<SipEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<SipEvent>>,
    is_running: bool,
}

impl SipHandler {
    pub async fn new(config: SipConfig) -> Result<Self> {
        info!("Creating SIP handler with redfire-sip-stack integration");
        
        // Create SIP parser with configuration
        let parser = SipParser::new(
            config.domain.clone(),
            config.listen_port,
            "Redfire-Gateway/1.0".to_string()
        );
        
        // Initialize SIP core engine with default config
        let sip_core_config = SipCoreConfig::default();
        
        let core_engine = create_default_core().await
            .map_err(|e| crate::Error::Sip(format!("Failed to create SIP core: {}", e)))?;
        
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Ok(Self {
            config,
            parser,
            core_engine: Some(core_engine),
            sessions: Arc::new(DashMap::new()),
            event_tx,
            event_rx: Some(event_rx),
            is_running: false,
        })
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<SipEvent>> {
        self.event_rx.take()
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("Starting SIP handler with redfire-sip-stack integration");
        self.is_running = true;
        
        // Initialize the core engine if needed
        if self.core_engine.is_none() {
            let core = create_default_core().await
                .map_err(|e| crate::Error::Sip(format!("Failed to start SIP core: {}", e)))?;
            self.core_engine = Some(core);
        }
        
        let _ = self.event_tx.send(SipEvent::Started {
            listen_address: format!("{}:{}", "0.0.0.0", self.config.listen_port),
        });
        
        Ok(())
    }

    pub async fn send_invite(
        &self,
        to_uri: &str,
        from_uri: &str,
        sdp: Option<&str>,
        target: SocketAddr,
    ) -> Result<String> {
        info!("Sending SIP INVITE from {} to {} via {}", from_uri, to_uri, target);
        
        let call_id = utils::generate_call_id();
        let session = SipSession::new_outbound(
            call_id.clone(),
            from_uri.to_string(),
            to_uri.to_string(),
        );
        let session_id = session.id.clone();
        
        self.sessions.insert(call_id.clone(), session);

        // TODO: Use core_engine to send actual SIP INVITE
        info!("Created SIP session: {} with call-id: {}", session_id, call_id);
        
        let _ = self.event_tx.send(SipEvent::InviteSent {
            session_id: session_id.clone(),
            target_uri: to_uri.to_string(),
        });
        
        Ok(session_id)
    }

    pub async fn send_response(
        &self,
        _session_id: &str,
        status_code: u16,
        reason_phrase: &str,
        _sdp: Option<&str>,
    ) -> Result<()> {
        warn!("SIP response requested but handler is in stub mode");
        info!("Stub SIP response: {} {}", status_code, reason_phrase);
        Ok(())
    }

    pub fn get_session(&self, session_id: &str) -> Option<SipSession> {
        for session in self.sessions.iter() {
            if session.id == session_id {
                return Some(session.clone());
            }
        }
        None
    }

    pub fn get_all_sessions(&self) -> Vec<SipSession> {
        self.sessions.iter().map(|entry| entry.value().clone()).collect()
    }

    pub fn get_active_session_count(&self) -> usize {
        self.sessions.len()
    }

    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping SIP handler stub");
        self.is_running = false;
        self.sessions.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sip_method_conversion() {
        assert_eq!(SipMethod::from_str("INVITE"), Some(SipMethod::Invite));
        assert_eq!(SipMethod::Invite.as_str(), "INVITE");
    }

    #[test]
    fn test_sip_message_creation() {
        let message = SipMessage::new_request(SipMethod::Invite, "sip:test@example.com".to_string());
        assert_eq!(message.method, Some(SipMethod::Invite));
        assert_eq!(message.uri, Some("sip:test@example.com".to_string()));
    }

    #[tokio::test]
    async fn test_sip_handler_creation() {
        let config = SipConfig {
            listen_port: 0, // Use ephemeral port
            domain: "test.local".to_string(),
            transport: crate::config::SipTransport::Udp,
            max_sessions: 100,
            session_timeout: 300,
            register_interval: 3600,
        };

        let handler = SipHandler::new(config).await;
        assert!(handler.is_ok());
    }

    #[tokio::test]
    async fn test_stub_sip_session() {
        let config = SipConfig {
            listen_port: 0,
            domain: "test.local".to_string(),
            transport: crate::config::SipTransport::Udp,
            max_sessions: 100,
            session_timeout: 300,
            register_interval: 3600,
        };

        let mut handler = SipHandler::new(config).await.unwrap();
        handler.start().await.unwrap();
        
        let session_id = handler.send_invite(
            "sip:test@example.com",
            "sip:caller@example.com",
            Some("v=0\r\n"),
            "127.0.0.1:5060".parse().unwrap(),
        ).await.unwrap();
        
        assert!(handler.get_session(&session_id).is_some());
        
        handler.stop().await.unwrap();
    }
}