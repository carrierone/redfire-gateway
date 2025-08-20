//! Services module for the Redfire Gateway

pub mod performance;
pub mod alarms;
pub mod testing;
pub mod auto_detection;
pub mod snmp;
pub mod debug;
pub mod interface_testing;
pub mod test_automation;
pub mod timing;
pub mod b2bua;
pub mod clustering;
pub mod transcoding;
pub mod sip_router;
pub mod media_relay;
pub mod cdr;

pub use performance::{PerformanceMonitor, PerformanceMetrics, PerformanceEvent, PerformanceAlert};
pub use alarms::{AlarmManager, Alarm, AlarmSeverity, AlarmType, AlarmEvent, AlarmStatistics};
pub use testing::{TestingService, LoopbackConfig, BertConfig, TestEvent, LoopbackType, BertPattern};
pub use auto_detection::{AutoDetectionService, DetectionEvent, SwitchType, MobileNetworkType};
pub use snmp::{SnmpService, SnmpEvent, SnmpTrap, Oid};
pub use debug::{DebugService, DebugEvent, BChannelStatus, BChannelState, DebugMessage};
pub use interface_testing::{InterfaceTestingService, InterfaceTestType, TestPattern, InterfaceTestEvent, InterfaceTestResult};
pub use test_automation::{TestAutomationService, TestScenario, AutomationEvent, SessionSummary};
pub use timing::{TimingService, StratumLevel, ClockSourceType, ClockStatus, TimingEvent, TimingConfig, TdmClockQuality};
pub use b2bua::{B2buaService, B2buaCall, B2buaCallState, B2buaEvent, CallLeg, MediaRelay, RoutingInfo};
pub use clustering::{ClusteringService, ClusterNode, DistributedTransaction, ClusteringEvent, AnycastManager};
pub use transcoding::{TranscodingService, TranscodingSession, TranscodingEvent, CodecType, GpuDevice};
pub use sip_router::{SipRouter, RoutingDecision, RoutingContext, RouteTarget, RoutingEvent};
pub use media_relay::{MediaRelayService, MediaRelaySession, MediaRelayEvent, RelayDirection, JitterBuffer};
pub use cdr::{CdrService, CallDetailRecord, CdrEvent, BillingInfo, QualityMetrics};