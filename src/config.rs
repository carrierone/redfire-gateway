//! Configuration management for the Redfire Gateway

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    pub general: GeneralConfig,
    pub tdmoe: TdmoeConfig,
    pub e1: E1Config,
    pub t1: T1Config,
    pub sip: SipConfig,
    pub rtp: RtpConfig,
    pub pri: PriConfig,
    pub sigtran: SigtranConfig,
    pub freetdm: FreeTdmConfig,
    pub trunk: TrunkConfig,
    pub nfas: NfasConfig,
    pub mobile: MobileConfig,
    pub feature_group: FeatureGroupConfig,
    pub performance: PerformanceConfig,
    pub logging: LoggingConfig,
    pub snmp: SnmpConfig,
    pub testing: TestingConfig,
    pub b2bua: B2buaConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub node_id: String,
    pub description: String,
    pub location: String,
    pub contact: String,
    pub max_calls: u32,
    pub call_timeout: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TdmoeConfig {
    pub interface: String,
    pub channels: u16,
    pub mtu: u16,
    pub qos_dscp: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E1Config {
    pub interface: String,
    pub framing: E1Framing,
    pub line_code: E1LineCode,
    pub clock_source: ClockSource,
    pub time_slots: Vec<u8>,
    pub channel_associated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct T1Config {
    pub interface: String,
    pub framing: T1Framing,
    pub line_code: T1LineCode,
    pub clock_source: ClockSource,
    pub time_slots: Vec<u8>,
    pub channel_associated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SipConfig {
    pub listen_port: u16,
    pub domain: String,
    pub transport: SipTransport,
    pub max_sessions: u32,
    pub session_timeout: u32,
    pub register_interval: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtpConfig {
    pub port_range: PortRange,
    pub jitter_buffer_size: u32,
    pub packet_timeout: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriConfig {
    pub variant: PriVariant,
    pub layer1: Layer1Type,
    pub time_slots: Vec<u8>,
    pub switch_type: String,
    pub network_specific: bool,
    pub point_to_point: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigtranConfig {
    pub enabled: bool,
    pub point_codes: PointCodes,
    pub variant: SigtranVariant,
    pub sctp_port: u16,
    pub heartbeat_interval: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeTdmConfig {
    pub enabled: bool,
    pub config_file: String,
    pub spans: Vec<FreeTdmSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrunkConfig {
    pub trunk_type: TrunkType,
    pub signaling: SignalingType,
    pub codec: CodecConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NfasConfig {
    pub enabled: bool,
    pub groups: Vec<NfasGroup>,
    pub switchover_timeout: u32,
    pub heartbeat_interval: u32,
    pub max_switchover_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileConfig {
    pub enabled: bool,
    pub network_type: NetworkType,
    pub msc: MscConfig,
    pub codecs: MobileCodecConfig,
    pub qos: QosConfig,
    pub emergency: EmergencyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureGroupConfig {
    pub fgd: FeatureGroupDConfig,
    pub fgb: FeatureGroupBConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub enabled: bool,
    pub interval: u32,
    pub history_size: u32,
    pub thresholds: PerformanceThresholds,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: 30,
            history_size: 100,
            thresholds: PerformanceThresholds::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub file: Option<String>,
    pub max_size: u64,
    pub max_files: u32,
    pub format: LogFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnmpConfig {
    pub enabled: bool,
    pub community: String,
    pub port: u16,
    pub bind_address: String,
    pub version: SnmpVersion,
}

impl Default for SnmpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            community: "public".to_string(),
            port: 161,
            bind_address: "0.0.0.0".to_string(),
            version: SnmpVersion::V2c,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestingConfig {
    pub loopback: LoopbackConfig,
    pub bert: BertConfig,
}

// Supporting types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum E1Framing {
    #[serde(rename = "crc4")]
    Crc4,
    #[serde(rename = "no-crc4")]
    NoCrc4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum E1LineCode {
    #[serde(rename = "hdb3")]
    Hdb3,
    #[serde(rename = "ami")]
    Ami,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum T1Framing {
    #[serde(rename = "esf")]
    Esf,
    #[serde(rename = "d4")]
    D4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum T1LineCode {
    #[serde(rename = "b8zs")]
    B8zs,
    #[serde(rename = "ami")]
    Ami,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClockSource {
    #[serde(rename = "internal")]
    Internal,
    #[serde(rename = "external")]
    External,
    #[serde(rename = "recovered")]
    Recovered,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SipTransport {
    #[serde(rename = "udp")]
    Udp,
    #[serde(rename = "tcp")]
    Tcp,
    #[serde(rename = "tls")]
    Tls,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PriVariant {
    #[serde(rename = "etsi")]
    Etsi,
    #[serde(rename = "ni2")]
    Ni2,
    #[serde(rename = "ansi")]
    Ansi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SigtranVariant {
    #[serde(rename = "itu")]
    Itu,
    #[serde(rename = "ansi")]
    Ansi,
    #[serde(rename = "etsi")]
    Etsi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Layer1Type {
    #[serde(rename = "e1")]
    E1,
    #[serde(rename = "t1")]
    T1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrunkType {
    #[serde(rename = "voice")]
    Voice,
    #[serde(rename = "data")]
    Data,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SignalingType {
    #[serde(rename = "pri")]
    Pri,
    #[serde(rename = "cas")]
    Cas,
    #[serde(rename = "ss7")]
    Ss7,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkType {
    #[serde(rename = "3g")]
    ThreeG,
    #[serde(rename = "4g")]
    FourG,
    #[serde(rename = "volte")]
    Volte,
    #[serde(rename = "vowifi")]
    VoWifi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    #[serde(rename = "json")]
    Json,
    #[serde(rename = "compact")]
    Compact,
    #[serde(rename = "full")]
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SnmpVersion {
    #[serde(rename = "v1")]
    V1,
    #[serde(rename = "v2c")]
    V2c,
    #[serde(rename = "v3")]
    V3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRange {
    pub min: u16,
    pub max: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointCodes {
    pub local: u32,
    pub remote: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeTdmSpan {
    pub span_id: u32,
    pub name: String,
    pub trunk_type: Layer1Type,
    pub d_channel: u8,
    pub channels: Vec<FreeTdmChannel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeTdmChannel {
    pub id: u8,
    pub channel_type: ChannelType,
    pub enabled: bool,
    pub signaling: SignalingType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelType {
    #[serde(rename = "bchan")]
    BChannel,
    #[serde(rename = "dchan")]
    DChannel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecConfig {
    pub allowed_codecs: Vec<String>,
    pub preferred_codec: String,
    pub dtmf: DtmfConfig,
    pub clear_channel_config: ClearChannelConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DtmfConfig {
    pub method: DtmfMethod,
    pub payload_type: u8,
    pub duration: u32,
    pub volume: i8,
    pub inter_digit_delay: u32,
    pub sip_info_content_type: String,
    pub inband_frequencies: InbandFrequencies,
    pub redundancy: u8,
    pub end_of_event: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DtmfMethod {
    #[serde(rename = "rfc2833")]
    Rfc2833,
    #[serde(rename = "sip_info")]
    SipInfo,
    #[serde(rename = "inband")]
    Inband,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InbandFrequencies {
    pub low_freq: Vec<u16>,
    pub high_freq: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearChannelConfig {
    pub enabled: bool,
    pub data_rate: u32,
    pub protocol: ClearChannelProtocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClearChannelProtocol {
    #[serde(rename = "v110")]
    V110,
    #[serde(rename = "hdlc")]
    Hdlc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NfasGroup {
    pub group_id: u32,
    pub primary_span: u32,
    pub backup_spans: Vec<u32>,
    pub load_balancing: bool,
    pub ces: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MscConfig {
    pub address: String,
    pub port: u16,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileCodecConfig {
    pub amr: AmrConfig,
    pub amr_wb: AmrWbConfig,
    pub evs: EvsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmrConfig {
    pub enabled: bool,
    pub modes: Vec<String>,
    pub mode_set: u32,
    pub octet_align: bool,
    pub robust_sorting: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmrWbConfig {
    pub enabled: bool,
    pub modes: Vec<String>,
    pub mode_set: u32,
    pub octet_align: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvsConfig {
    pub enabled: bool,
    pub primary_mode: u32,
    pub modes: Vec<String>,
    pub vbr: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QosConfig {
    pub enabled: bool,
    pub conversational_class: u32,
    pub max_bitrate: u32,
    pub guaranteed_bitrate: u32,
    pub transfer_delay: u32,
    pub traffic_handling_priority: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyConfig {
    pub enabled: bool,
    pub emergency_numbers: Vec<String>,
    pub location_info: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureGroupDConfig {
    pub enabled: bool,
    pub seizure_timeout: u32,
    pub wink_duration: u32,
    pub ani_delivery: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureGroupBConfig {
    pub enabled: bool,
    pub seizure_timeout: u32,
    pub mf_signaling: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceThresholds {
    pub cpu: ThresholdConfig,
    pub memory: ThresholdConfig,
    pub disk: ThresholdConfig,
    pub load: LoadThresholdConfig,
    pub network: NetworkThresholdConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdConfig {
    pub warning: f64,
    pub critical: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadThresholdConfig {
    pub warning: f64,
    pub critical: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkThresholdConfig {
    pub error_rate: f64,
    pub utilization_warning: f64,
}

impl Default for PerformanceThresholds {
    fn default() -> Self {
        Self {
            cpu: ThresholdConfig::default(),
            memory: ThresholdConfig::default(),
            disk: ThresholdConfig::default(),
            load: LoadThresholdConfig::default(),
            network: NetworkThresholdConfig::default(),
        }
    }
}

impl Default for ThresholdConfig {
    fn default() -> Self {
        Self {
            warning: 80.0,
            critical: 95.0,
        }
    }
}

impl Default for LoadThresholdConfig {
    fn default() -> Self {
        Self {
            warning: 4.0,
            critical: 8.0,
        }
    }
}

impl Default for NetworkThresholdConfig {
    fn default() -> Self {
        Self {
            error_rate: 1.0,
            utilization_warning: 80.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopbackConfig {
    pub enabled: bool,
    pub timeout: u32,
    pub max_concurrent: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BertConfig {
    pub enabled: bool,
    pub patterns: Vec<String>,
    pub default_duration: u32,
    pub error_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct B2buaConfig {
    pub enabled: bool,
    pub max_concurrent_calls: u32,
    pub call_timeout: u32,
    pub media_timeout: u32,
    pub default_route_gateway: Option<String>,
    pub enable_media_relay: bool,
    pub enable_codec_transcoding: bool,
    pub transcoding_backend: TranscodingBackend,
    pub enable_simd: bool,
    pub simd_instruction_set: Option<String>,
    pub auto_detect_simd: bool,
    pub simd_fallback: bool,
    pub enable_gpu: bool,
    pub gpu_device_id: Option<u32>,
    pub gpu_backend: Option<String>,
    pub auto_detect_gpu: bool,
    pub gpu_fallback: bool,
    pub gpu_memory_limit_mb: Option<u64>,
    pub routing_table: Vec<RoutingRule>,
    pub clustering: ClusteringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TranscodingBackend {
    #[serde(rename = "cpu")]
    Cpu,
    #[serde(rename = "simd")]
    Simd,
    #[serde(rename = "simd-avx2")]
    SimdAvx2,
    #[serde(rename = "simd-avx512")]
    SimdAvx512,
    #[serde(rename = "cuda")]
    Cuda,
    #[serde(rename = "rocm")]
    Rocm,
    #[serde(rename = "gpu")]
    Gpu,
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: String,
    pub pattern: String,
    pub route_type: RouteType,
    pub target: String,
    pub priority: u8,
    pub translation: Option<NumberTranslation>,
    pub codec_preference: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RouteType {
    #[serde(rename = "direct")]
    Direct,
    #[serde(rename = "gateway")]
    Gateway,
    #[serde(rename = "trunk")]
    Trunk,
    #[serde(rename = "emergency")]
    Emergency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumberTranslation {
    pub prefix_strip: Option<String>,
    pub prefix_add: Option<String>,
    pub suffix_strip: Option<String>,
    pub suffix_add: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusteringConfig {
    pub enabled: bool,
    pub cluster_id: String,
    pub node_id: String,
    pub anycast_addresses: Vec<String>,
    pub sync_port: u16,
    pub heartbeat_interval: u32,
    pub transaction_sync_enabled: bool,
    pub shared_state_backend: SharedStateBackend,
    pub consensus_algorithm: ConsensusAlgorithm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SharedStateBackend {
    #[serde(rename = "redis")]
    Redis { addresses: Vec<String>, password: Option<String> },
    #[serde(rename = "etcd")]
    Etcd { endpoints: Vec<String> },
    #[serde(rename = "consul")]
    Consul { endpoints: Vec<String> },
    #[serde(rename = "raft")]
    Raft { peers: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusAlgorithm {
    #[serde(rename = "raft")]
    Raft,
    #[serde(rename = "pbft")]
    Pbft,
    #[serde(rename = "hashgraph")]
    Hashgraph,
}

impl GatewayConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: GatewayConfig = toml::from_str(&contents)
            .map_err(|e| Error::parse(format!("Invalid TOML: {}", e)))?;
        Ok(config)
    }

    pub fn load_from_env() -> Result<Self> {
        let mut settings = config::Config::builder();
        
        // Load from environment variables with REDFIRE_ prefix
        settings = settings.add_source(
            config::Environment::with_prefix("REDFIRE")
                .separator("_")
        );
        
        let config = settings.build()?;
        let gateway_config = config.try_deserialize()?;
        Ok(gateway_config)
    }

    pub fn validate(&self) -> Result<()> {
        // Validate port ranges
        if self.rtp.port_range.min >= self.rtp.port_range.max {
            return Err(Error::parse("Invalid RTP port range"));
        }

        // Validate time slots
        for slot in &self.e1.time_slots {
            if *slot == 0 || *slot > 31 {
                return Err(Error::parse("Invalid E1 time slot"));
            }
        }

        for slot in &self.t1.time_slots {
            if *slot == 0 || *slot > 24 {
                return Err(Error::parse("Invalid T1 time slot"));
            }
        }

        // Validate codec configuration
        if self.trunk.codec.allowed_codecs.is_empty() {
            return Err(Error::parse("No codecs configured"));
        }

        Ok(())
    }

    pub fn default_config() -> Self {
        Self {
            general: GeneralConfig {
                node_id: "redfire-gateway-1".to_string(),
                description: "Redfire TDMoE to SIP Gateway".to_string(),
                location: "Network Operations Center".to_string(),
                contact: "admin@redfire-gateway.local".to_string(),
                max_calls: 1000,
                call_timeout: 300,
            },
            tdmoe: TdmoeConfig {
                interface: "eth0".to_string(),
                channels: 30,
                mtu: 1500,
                qos_dscp: 46,
            },
            e1: E1Config {
                interface: "span1".to_string(),
                framing: E1Framing::Crc4,
                line_code: E1LineCode::Hdb3,
                clock_source: ClockSource::External,
                time_slots: (1..32).filter(|&x| x != 16).collect(),
                channel_associated: false,
            },
            t1: T1Config {
                interface: "span1".to_string(),
                framing: T1Framing::Esf,
                line_code: T1LineCode::B8zs,
                clock_source: ClockSource::External,
                time_slots: (1..25).filter(|&x| x != 24).collect(),
                channel_associated: false,
            },
            sip: SipConfig {
                listen_port: 5060,
                domain: "redfire-gateway.local".to_string(),
                transport: SipTransport::Udp,
                max_sessions: 500,
                session_timeout: 300,
                register_interval: 3600,
            },
            rtp: RtpConfig {
                port_range: PortRange { min: 10000, max: 20000 },
                jitter_buffer_size: 50,
                packet_timeout: 1000,
            },
            pri: PriConfig {
                variant: PriVariant::Etsi,
                layer1: Layer1Type::E1,
                time_slots: (1..32).filter(|&x| x != 16).collect(),
                switch_type: "euroISDN".to_string(),
                network_specific: false,
                point_to_point: false,
            },
            sigtran: SigtranConfig {
                enabled: false,
                point_codes: PointCodes { local: 1, remote: 2 },
                variant: SigtranVariant::Itu,
                sctp_port: 2905,
                heartbeat_interval: 30,
            },
            freetdm: FreeTdmConfig {
                enabled: false,
                config_file: "/etc/freetdm.conf".to_string(),
                spans: vec![],
            },
            trunk: TrunkConfig {
                trunk_type: TrunkType::Voice,
                signaling: SignalingType::Pri,
                codec: CodecConfig {
                    allowed_codecs: vec!["g711a".to_string(), "g711u".to_string()],
                    preferred_codec: "g711a".to_string(),
                    dtmf: DtmfConfig {
                        method: DtmfMethod::Rfc2833,
                        payload_type: 101,
                        duration: 100,
                        volume: -10,
                        inter_digit_delay: 50,
                        sip_info_content_type: "application/dtmf-relay".to_string(),
                        inband_frequencies: InbandFrequencies {
                            low_freq: vec![697, 770, 852, 941],
                            high_freq: vec![1209, 1336, 1477, 1633],
                        },
                        redundancy: 3,
                        end_of_event: true,
                    },
                    clear_channel_config: ClearChannelConfig {
                        enabled: false,
                        data_rate: 64000,
                        protocol: ClearChannelProtocol::V110,
                    },
                },
            },
            nfas: NfasConfig {
                enabled: false,
                groups: vec![],
                switchover_timeout: 5000,
                heartbeat_interval: 30000,
                max_switchover_attempts: 3,
            },
            mobile: MobileConfig {
                enabled: false,
                network_type: NetworkType::FourG,
                msc: MscConfig {
                    address: "127.0.0.1".to_string(),
                    port: 5060,
                    protocol: "sip".to_string(),
                },
                codecs: MobileCodecConfig {
                    amr: AmrConfig {
                        enabled: false,
                        modes: vec!["7.40".to_string(), "12.2".to_string()],
                        mode_set: 0,
                        octet_align: true,
                        robust_sorting: false,
                    },
                    amr_wb: AmrWbConfig {
                        enabled: false,
                        modes: vec!["12.65".to_string(), "23.85".to_string()],
                        mode_set: 0,
                        octet_align: true,
                    },
                    evs: EvsConfig {
                        enabled: false,
                        primary_mode: 0,
                        modes: vec!["9.6".to_string(), "13.2".to_string()],
                        vbr: false,
                    },
                },
                qos: QosConfig {
                    enabled: false,
                    conversational_class: 1,
                    max_bitrate: 64000,
                    guaranteed_bitrate: 64000,
                    transfer_delay: 100,
                    traffic_handling_priority: 1,
                },
                emergency: EmergencyConfig {
                    enabled: true,
                    emergency_numbers: vec!["112".to_string(), "911".to_string()],
                    location_info: false,
                },
            },
            feature_group: FeatureGroupConfig {
                fgd: FeatureGroupDConfig {
                    enabled: false,
                    seizure_timeout: 5000,
                    wink_duration: 200,
                    ani_delivery: true,
                },
                fgb: FeatureGroupBConfig {
                    enabled: false,
                    seizure_timeout: 5000,
                    mf_signaling: true,
                },
            },
            performance: PerformanceConfig {
                enabled: true,
                interval: 5000,
                history_size: 720,
                thresholds: PerformanceThresholds {
                    cpu: ThresholdConfig { warning: 80.0, critical: 95.0 },
                    memory: ThresholdConfig { warning: 80.0, critical: 95.0 },
                    disk: ThresholdConfig { warning: 90.0, critical: 98.0 },
                    load: LoadThresholdConfig { warning: 0.8, critical: 1.5 },
                    network: NetworkThresholdConfig { error_rate: 0.1, utilization_warning: 80.0 },
                },
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                file: Some("/var/log/redfire-gateway.log".to_string()),
                max_size: 100 * 1024 * 1024, // 100MB
                max_files: 10,
                format: LogFormat::Json,
            },
            snmp: SnmpConfig {
                enabled: true,
                community: "public".to_string(),
                port: 161,
                bind_address: "0.0.0.0".to_string(),
                version: SnmpVersion::V2c,
            },
            testing: TestingConfig {
                loopback: LoopbackConfig {
                    enabled: true,
                    timeout: 10000,
                    max_concurrent: 10,
                },
                bert: BertConfig {
                    enabled: true,
                    patterns: vec!["prbs_15".to_string(), "prbs_23".to_string()],
                    default_duration: 60,
                    error_threshold: 0.001,
                },
            },
            b2bua: B2buaConfig {
                enabled: true,
                max_concurrent_calls: 500,
                call_timeout: 300,
                media_timeout: 60,
                default_route_gateway: None,
                enable_media_relay: true,
                enable_codec_transcoding: false,
                transcoding_backend: TranscodingBackend::Auto,
                enable_simd: true,
                simd_instruction_set: None, // Auto-detect
                auto_detect_simd: true,
                simd_fallback: true,
                enable_gpu: true,
                gpu_device_id: None, // Auto-select
                gpu_backend: None,   // Auto-detect (CUDA/ROCm)
                auto_detect_gpu: true,
                gpu_fallback: true,
                gpu_memory_limit_mb: None, // No limit
                routing_table: vec![
                    RoutingRule {
                        id: "emergency".to_string(),
                        pattern: "^(911|112)$".to_string(),
                        route_type: RouteType::Emergency,
                        target: "emergency.psap.local".to_string(),
                        priority: 1,
                        translation: None,
                        codec_preference: vec!["g711u".to_string()],
                    },
                    RoutingRule {
                        id: "local".to_string(),
                        pattern: "^[2-9][0-9]{3}$".to_string(),
                        route_type: RouteType::Direct,
                        target: "localhost".to_string(),
                        priority: 10,
                        translation: None,
                        codec_preference: vec!["g711a".to_string(), "g711u".to_string()],
                    },
                ],
                clustering: ClusteringConfig {
                    enabled: false,
                    cluster_id: "redfire-cluster-1".to_string(),
                    node_id: "node-1".to_string(),
                    anycast_addresses: vec!["192.168.1.100".to_string()],
                    sync_port: 8080,
                    heartbeat_interval: 30,
                    transaction_sync_enabled: true,
                    shared_state_backend: SharedStateBackend::Redis {
                        addresses: vec!["redis://localhost:6379".to_string()],
                        password: None,
                    },
                    consensus_algorithm: ConsensusAlgorithm::Raft,
                },
            },
        }
    }
}