//! SNMP service for network management

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info};

use crate::config::SnmpConfig;
use crate::{Error, Result};

/// SNMP version
#[derive(Debug, Clone, PartialEq)]
pub enum SnmpVersion {
    V1,
    V2c,
    V3,
}

/// SNMP PDU types
#[derive(Debug, Clone, PartialEq)]
pub enum PduType {
    GetRequest,
    GetNextRequest,
    GetBulkRequest,
    SetRequest,
    GetResponse,
    Trap,
    InformRequest,
    Report,
}

/// SNMP error status
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorStatus {
    NoError = 0,
    TooBig = 1,
    NoSuchName = 2,
    BadValue = 3,
    ReadOnly = 4,
    GenErr = 5,
    NoAccess = 6,
    WrongType = 7,
    WrongLength = 8,
    WrongEncoding = 9,
    WrongValue = 10,
    NoCreation = 11,
    InconsistentValue = 12,
    ResourceUnavailable = 13,
    CommitFailed = 14,
    UndoFailed = 15,
    AuthorizationError = 16,
    NotWritable = 17,
    InconsistentName = 18,
}

/// SNMP data types
#[derive(Debug, Clone)]
pub enum SnmpValue {
    Integer(i32),
    OctetString(Vec<u8>),
    Null,
    ObjectId(Vec<u32>),
    IpAddress([u8; 4]),
    Counter32(u32),
    Gauge32(u32),
    TimeTicks(u32),
    Opaque(Vec<u8>),
    Counter64(u64),
}

/// Object Identifier (OID)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Oid {
    pub components: Vec<u32>,
}

impl Oid {
    pub fn new(components: Vec<u32>) -> Self {
        Self { components }
    }

    pub fn from_string(s: &str) -> Result<Self> {
        let components: std::result::Result<Vec<u32>, _> = s.split('.')
            .filter(|part| !part.is_empty())
            .map(|part| part.parse::<u32>())
            .collect();
        
        match components {
            Ok(comps) => Ok(Self::new(comps)),
            Err(_) => Err(Error::parse(format!("Invalid OID: {}", s))),
        }
    }

    pub fn to_string(&self) -> String {
        self.components.iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(".")
    }

    pub fn append(&self, component: u32) -> Self {
        let mut new_components = self.components.clone();
        new_components.push(component);
        Self::new(new_components)
    }

    pub fn is_child_of(&self, parent: &Oid) -> bool {
        if self.components.len() <= parent.components.len() {
            return false;
        }
        
        self.components[..parent.components.len()] == parent.components[..]
    }
}

/// MIB variable binding
#[derive(Debug, Clone)]
pub struct VarBind {
    pub oid: Oid,
    pub value: SnmpValue,
}

/// SNMP request/response
#[derive(Debug, Clone)]
pub struct SnmpMessage {
    pub version: SnmpVersion,
    pub community: String,
    pub pdu_type: PduType,
    pub request_id: u32,
    pub error_status: ErrorStatus,
    pub error_index: u32,
    pub var_binds: Vec<VarBind>,
}

/// SNMP trap information
#[derive(Debug, Clone)]
pub struct SnmpTrap {
    pub enterprise_oid: Oid,
    pub agent_addr: IpAddr,
    pub generic_trap: u32,
    pub specific_trap: u32,
    pub timestamp: u32,
    pub var_binds: Vec<VarBind>,
}

/// MIB tree node
#[derive(Debug, Clone)]
pub struct MibNode {
    pub oid: Oid,
    pub name: String,
    pub description: String,
    pub access: MibAccess,
    pub data_type: String,
    pub value_getter: Option<String>, // Function name to get value
    pub value_setter: Option<String>, // Function name to set value
}

#[derive(Debug, Clone, PartialEq)]
pub enum MibAccess {
    ReadOnly,
    ReadWrite,
    WriteOnly,
    NotAccessible,
}

/// SNMP events
#[derive(Debug, Clone)]
pub enum SnmpEvent {
    RequestReceived { 
        source: SocketAddr, 
        request: SnmpMessage 
    },
    ResponseSent { 
        destination: SocketAddr, 
        response: SnmpMessage 
    },
    TrapSent { 
        destination: SocketAddr, 
        trap: SnmpTrap 
    },
    AuthenticationFailure { 
        source: SocketAddr, 
        community: String 
    },
    Error { 
        source: SocketAddr, 
        error: String 
    },
}

/// SNMP service
pub struct SnmpService {
    config: SnmpConfig,
    socket: Option<Arc<UdpSocket>>,
    mib_tree: Arc<RwLock<HashMap<Oid, MibNode>>>,
    trap_destinations: Arc<RwLock<Vec<SocketAddr>>>,
    event_tx: mpsc::UnboundedSender<SnmpEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<SnmpEvent>>,
    is_running: bool,
    #[allow(dead_code)]
    system_uptime_start: Instant,
}

impl SnmpService {
    pub fn new(config: SnmpConfig) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        Self {
            config,
            socket: None,
            mib_tree: Arc::new(RwLock::new(HashMap::new())),
            trap_destinations: Arc::new(RwLock::new(Vec::new())),
            event_tx,
            event_rx: Some(event_rx),
            is_running: false,
            system_uptime_start: Instant::now(),
        }
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<SnmpEvent>> {
        self.event_rx.take()
    }

    pub async fn start(&mut self) -> Result<()> {
        if !self.config.enabled {
            info!("SNMP service is disabled");
            return Ok(());
        }

        info!("Starting SNMP agent on {}:{}", self.config.bind_address, self.config.port);

        // Bind to socket
        let addr = format!("{}:{}", self.config.bind_address, self.config.port);
        let socket = UdpSocket::bind(&addr).await
            .map_err(|e| Error::network(format!("Failed to bind SNMP socket: {}", e)))?;
        
        self.socket = Some(Arc::new(socket));

        // Initialize MIB tree
        self.initialize_mib().await?;

        self.is_running = true;

        // Start message processing loop
        if let Some(socket) = &self.socket {
            let socket_clone = Arc::clone(socket);
            let event_tx = self.event_tx.clone();
            let mib_tree = Arc::clone(&self.mib_tree);
            let config = self.config.clone();
            
            tokio::spawn(async move {
                let mut buffer = [0u8; 1500]; // MTU-sized buffer
                
                loop {
                    match socket_clone.recv_from(&mut buffer).await {
                        Ok((len, src)) => {
                            let data = &buffer[..len];
                            if let Err(e) = Self::handle_snmp_request(
                                data, src, &socket_clone, &event_tx, &mib_tree, &config
                            ).await {
                                error!("Error handling SNMP request from {}: {}", src, e);
                                let _ = event_tx.send(SnmpEvent::Error {
                                    source: src,
                                    error: e.to_string(),
                                });
                            }
                        },
                        Err(e) => {
                            error!("Error receiving SNMP packet: {}", e);
                        }
                    }
                }
            });
        }

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping SNMP service");
        self.is_running = false;
        self.socket = None;
        Ok(())
    }

    async fn initialize_mib(&self) -> Result<()> {
        let mut mib = self.mib_tree.write().await;

        // System MIB (1.3.6.1.2.1.1)
        let system_oid = Oid::new(vec![1, 3, 6, 1, 2, 1, 1]);
        
        mib.insert(system_oid.append(1), MibNode {
            oid: system_oid.append(1),
            name: "sysDescr".to_string(),
            description: "System Description".to_string(),
            access: MibAccess::ReadOnly,
            data_type: "OCTET STRING".to_string(),
            value_getter: Some("get_sys_descr".to_string()),
            value_setter: None,
        });

        mib.insert(system_oid.append(2), MibNode {
            oid: system_oid.append(2),
            name: "sysObjectID".to_string(),
            description: "System Object ID".to_string(),
            access: MibAccess::ReadOnly,
            data_type: "OBJECT IDENTIFIER".to_string(),
            value_getter: Some("get_sys_object_id".to_string()),
            value_setter: None,
        });

        mib.insert(system_oid.append(3), MibNode {
            oid: system_oid.append(3),
            name: "sysUpTime".to_string(),
            description: "System Up Time".to_string(),
            access: MibAccess::ReadOnly,
            data_type: "TimeTicks".to_string(),
            value_getter: Some("get_sys_uptime".to_string()),
            value_setter: None,
        });

        mib.insert(system_oid.append(4), MibNode {
            oid: system_oid.append(4),
            name: "sysContact".to_string(),
            description: "System Contact".to_string(),
            access: MibAccess::ReadWrite,
            data_type: "OCTET STRING".to_string(),
            value_getter: Some("get_sys_contact".to_string()),
            value_setter: Some("set_sys_contact".to_string()),
        });

        mib.insert(system_oid.append(5), MibNode {
            oid: system_oid.append(5),
            name: "sysName".to_string(),
            description: "System Name".to_string(),
            access: MibAccess::ReadWrite,
            data_type: "OCTET STRING".to_string(),
            value_getter: Some("get_sys_name".to_string()),
            value_setter: Some("set_sys_name".to_string()),
        });

        mib.insert(system_oid.append(6), MibNode {
            oid: system_oid.append(6),
            name: "sysLocation".to_string(),
            description: "System Location".to_string(),
            access: MibAccess::ReadWrite,
            data_type: "OCTET STRING".to_string(),
            value_getter: Some("get_sys_location".to_string()),
            value_setter: Some("set_sys_location".to_string()),
        });

        // Gateway-specific MIB (enterprise OID would be assigned)
        let enterprise_oid = Oid::new(vec![1, 3, 6, 1, 4, 1, 99999]); // Example enterprise OID
        
        mib.insert(enterprise_oid.append(1), MibNode {
            oid: enterprise_oid.append(1),
            name: "gatewayVersion".to_string(),
            description: "Gateway Software Version".to_string(),
            access: MibAccess::ReadOnly,
            data_type: "OCTET STRING".to_string(),
            value_getter: Some("get_gateway_version".to_string()),
            value_setter: None,
        });

        mib.insert(enterprise_oid.append(2), MibNode {
            oid: enterprise_oid.append(2),
            name: "activeCalls".to_string(),
            description: "Number of Active Calls".to_string(),
            access: MibAccess::ReadOnly,
            data_type: "Gauge32".to_string(),
            value_getter: Some("get_active_calls".to_string()),
            value_setter: None,
        });

        info!("Initialized MIB tree with {} objects", mib.len());

        Ok(())
    }

    async fn handle_snmp_request(
        data: &[u8],
        src: SocketAddr,
        socket: &UdpSocket,
        event_tx: &mpsc::UnboundedSender<SnmpEvent>,
        mib_tree: &Arc<RwLock<HashMap<Oid, MibNode>>>,
        config: &SnmpConfig,
    ) -> Result<()> {
        // Parse SNMP message (simplified - real implementation would use ASN.1 BER/DER)
        let message = Self::parse_snmp_message(data)?;

        // Send request received event
        let _ = event_tx.send(SnmpEvent::RequestReceived {
            source: src,
            request: message.clone(),
        });

        // Authenticate
        if !Self::authenticate(&message, config) {
            let _ = event_tx.send(SnmpEvent::AuthenticationFailure {
                source: src,
                community: message.community.clone(),
            });
            return Err(Error::internal("Authentication failed"));
        }

        // Process request
        let response = Self::process_request(message, mib_tree).await?;

        // Send response
        let response_data = Self::encode_snmp_message(&response)?;
        socket.send_to(&response_data, src).await
            .map_err(|e| Error::network(format!("Failed to send SNMP response: {}", e)))?;

        // Send response sent event
        let _ = event_tx.send(SnmpEvent::ResponseSent {
            destination: src,
            response,
        });

        Ok(())
    }

    fn parse_snmp_message(data: &[u8]) -> Result<SnmpMessage> {
        // Simplified SNMP message parsing
        // Real implementation would use proper ASN.1 BER decoding
        
        if data.len() < 10 {
            return Err(Error::parse("SNMP message too short"));
        }

        // For simulation, create a dummy GetRequest
        Ok(SnmpMessage {
            version: SnmpVersion::V2c,
            community: "public".to_string(),
            pdu_type: PduType::GetRequest,
            request_id: 12345,
            error_status: ErrorStatus::NoError,
            error_index: 0,
            var_binds: vec![
                VarBind {
                    oid: Oid::new(vec![1, 3, 6, 1, 2, 1, 1, 1, 0]), // sysDescr.0
                    value: SnmpValue::Null,
                }
            ],
        })
    }

    fn authenticate(message: &SnmpMessage, config: &SnmpConfig) -> bool {
        // Simple community string authentication for SNMPv1/v2c
        match message.version {
            SnmpVersion::V1 | SnmpVersion::V2c => {
                message.community == config.community
            },
            SnmpVersion::V3 => {
                // SNMPv3 would require proper USM authentication
                true // Simplified for demo
            }
        }
    }

    async fn process_request(
        request: SnmpMessage,
        mib_tree: &Arc<RwLock<HashMap<Oid, MibNode>>>,
    ) -> Result<SnmpMessage> {
        let mut response = SnmpMessage {
            version: request.version,
            community: request.community,
            pdu_type: PduType::GetResponse,
            request_id: request.request_id,
            error_status: ErrorStatus::NoError,
            error_index: 0,
            var_binds: Vec::new(),
        };

        let mib = mib_tree.read().await;

        match request.pdu_type {
            PduType::GetRequest => {
                for (index, var_bind) in request.var_binds.iter().enumerate() {
                    if let Some(node) = mib.get(&var_bind.oid) {
                        let value = Self::get_mib_value(node).await;
                        response.var_binds.push(VarBind {
                            oid: var_bind.oid.clone(),
                            value,
                        });
                    } else {
                        response.error_status = ErrorStatus::NoSuchName;
                        response.error_index = (index + 1) as u32;
                        break;
                    }
                }
            },
            PduType::GetNextRequest => {
                for (index, var_bind) in request.var_binds.iter().enumerate() {
                    if let Some(next_oid) = Self::get_next_oid(&var_bind.oid, &mib) {
                        if let Some(node) = mib.get(&next_oid) {
                            let value = Self::get_mib_value(node).await;
                            response.var_binds.push(VarBind {
                                oid: next_oid,
                                value,
                            });
                        }
                    } else {
                        response.error_status = ErrorStatus::NoSuchName;
                        response.error_index = (index + 1) as u32;
                        break;
                    }
                }
            },
            _ => {
                response.error_status = ErrorStatus::GenErr;
            }
        }

        Ok(response)
    }

    async fn get_mib_value(node: &MibNode) -> SnmpValue {
        // Get actual values based on the getter function
        match node.value_getter.as_deref() {
            Some("get_sys_descr") => {
                SnmpValue::OctetString(b"Redfire TDMoE Gateway v1.0".to_vec())
            },
            Some("get_sys_object_id") => {
                SnmpValue::ObjectId(vec![1, 3, 6, 1, 4, 1, 99999, 1])
            },
            Some("get_sys_uptime") => {
                // Return uptime in centiseconds
                let uptime = SystemTime::now().duration_since(UNIX_EPOCH)
                    .unwrap_or(Duration::from_secs(0));
                SnmpValue::TimeTicks((uptime.as_secs() * 100) as u32)
            },
            Some("get_sys_contact") => {
                SnmpValue::OctetString(b"admin@redfire-gateway.local".to_vec())
            },
            Some("get_sys_name") => {
                SnmpValue::OctetString(b"redfire-gateway-1".to_vec())
            },
            Some("get_sys_location") => {
                SnmpValue::OctetString(b"Network Operations Center".to_vec())
            },
            Some("get_gateway_version") => {
                SnmpValue::OctetString(crate::VERSION.as_bytes().to_vec())
            },
            Some("get_active_calls") => {
                // Simulate active call count
                SnmpValue::Gauge32(5)
            },
            _ => SnmpValue::Null,
        }
    }

    fn get_next_oid(current_oid: &Oid, mib: &HashMap<Oid, MibNode>) -> Option<Oid> {
        // Find the lexicographically next OID in the MIB tree
        let mut candidates: Vec<&Oid> = mib.keys()
            .filter(|oid| Self::oid_compare(oid, current_oid) == std::cmp::Ordering::Greater)
            .collect();
        
        candidates.sort_by(|a, b| Self::oid_compare(a, b));
        candidates.first().map(|oid| (*oid).clone())
    }

    fn oid_compare(oid1: &Oid, oid2: &Oid) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        
        for (a, b) in oid1.components.iter().zip(oid2.components.iter()) {
            match a.cmp(b) {
                Ordering::Equal => continue,
                other => return other,
            }
        }
        
        oid1.components.len().cmp(&oid2.components.len())
    }

    fn encode_snmp_message(_message: &SnmpMessage) -> Result<Vec<u8>> {
        // Simplified SNMP message encoding
        // Real implementation would use proper ASN.1 BER encoding
        
        let mut data = Vec::new();
        
        // For simulation, return a minimal valid SNMP response
        data.extend_from_slice(b"\x30\x82\x00\x28"); // SEQUENCE
        data.extend_from_slice(b"\x02\x01\x01"); // version = 1 (v2c)
        data.extend_from_slice(b"\x04\x06public"); // community
        data.extend_from_slice(b"\xa2\x82\x00\x19"); // GetResponse PDU
        data.extend_from_slice(b"\x02\x04\x00\x00\x30\x39"); // request ID
        data.extend_from_slice(b"\x02\x01\x00"); // error status = 0
        data.extend_from_slice(b"\x02\x01\x00"); // error index = 0
        data.extend_from_slice(b"\x30\x82\x00\x0b"); // VarBindList
        data.extend_from_slice(b"\x30\x82\x00\x07"); // VarBind
        data.extend_from_slice(b"\x06\x03\x2b\x06\x01"); // OID
        data.extend_from_slice(b"\x04\x00"); // Value (empty string)
        
        Ok(data)
    }

    /// Send SNMP trap
    pub async fn send_trap(&self, trap: SnmpTrap) -> Result<()> {
        if !self.is_running {
            return Err(Error::invalid_state("SNMP service is not running"));
        }

        let destinations = {
            let dests = self.trap_destinations.read().await;
            dests.clone()
        };

        if let Some(socket) = &self.socket {
            for dest in destinations {
                let trap_data = Self::encode_trap(&trap)?;
                
                match socket.send_to(&trap_data, dest).await {
                    Ok(_) => {
                        info!("Sent SNMP trap to {}", dest);
                        let _ = self.event_tx.send(SnmpEvent::TrapSent {
                            destination: dest,
                            trap: trap.clone(),
                        });
                    },
                    Err(e) => {
                        error!("Failed to send SNMP trap to {}: {}", dest, e);
                    }
                }
            }
        }

        Ok(())
    }

    fn encode_trap(_trap: &SnmpTrap) -> Result<Vec<u8>> {
        // Simplified trap encoding
        // Real implementation would use proper ASN.1 BER encoding
        let mut data = Vec::new();
        data.extend_from_slice(b"\x30\x82\x00\x40"); // SEQUENCE
        data.extend_from_slice(b"\x02\x01\x00"); // version = 0 (v1)
        data.extend_from_slice(b"\x04\x06public"); // community
        data.extend_from_slice(b"\xa4\x82\x00\x33"); // Trap PDU
        // Add trap fields (simplified)
        Ok(data)
    }

    /// Add trap destination
    pub async fn add_trap_destination(&self, dest: SocketAddr) -> Result<()> {
        let mut destinations = self.trap_destinations.write().await;
        if !destinations.contains(&dest) {
            destinations.push(dest);
            info!("Added SNMP trap destination: {}", dest);
        }
        Ok(())
    }

    /// Remove trap destination  
    pub async fn remove_trap_destination(&self, dest: &SocketAddr) -> Result<()> {
        let mut destinations = self.trap_destinations.write().await;
        destinations.retain(|d| d != dest);
        info!("Removed SNMP trap destination: {}", dest);
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> SnmpConfig {
        SnmpConfig {
            enabled: true,
            community: "public".to_string(),
            port: 1161, // Non-privileged port for testing
            bind_address: "127.0.0.1".to_string(),
            version: "v2c".to_string(),
        }
    }

    #[tokio::test]
    async fn test_oid_creation() {
        let oid = Oid::from_string("1.3.6.1.2.1.1.1.0").unwrap();
        assert_eq!(oid.components, vec![1, 3, 6, 1, 2, 1, 1, 1, 0]);
        assert_eq!(oid.to_string(), "1.3.6.1.2.1.1.1.0");
    }

    #[tokio::test]
    async fn test_snmp_service_creation() {
        let config = create_test_config();
        let service = SnmpService::new(config);
        
        assert!(!service.is_running());
    }

    #[tokio::test]
    async fn test_mib_initialization() {
        let config = create_test_config();
        let mut service = SnmpService::new(config);
        
        service.initialize_mib().await.unwrap();
        
        let mib = service.mib_tree.read().await;
        assert!(mib.len() > 0);
        
        // Check system MIB objects
        let sys_descr_oid = Oid::new(vec![1, 3, 6, 1, 2, 1, 1, 1]);
        assert!(mib.contains_key(&sys_descr_oid));
    }
}