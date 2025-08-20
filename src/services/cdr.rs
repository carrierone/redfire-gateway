//! CDR (Call Detail Record) and billing integration for B2BUA
//! 
//! This module provides comprehensive call detail recording and billing
//! functionality for telecommunications compliance and revenue management.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

use crate::config::RouteType;
use crate::services::b2bua::{B2buaCall, B2buaCallState};
use crate::services::media_relay::MediaRelayStats;
use crate::services::transcoding::CodecType;
use crate::{Error, Result};

/// Call Detail Record structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallDetailRecord {
    pub id: String,
    pub call_id: String,
    pub session_id: String,
    pub caller: String,
    pub callee: String,
    pub original_called_number: String,
    pub translated_called_number: String,
    pub calling_party_category: CallingPartyCategory,
    pub call_type: CallType,
    pub route_type: RouteType,
    pub start_time: DateTime<Utc>,
    pub answer_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_seconds: u64,
    pub billable_duration_seconds: u64,
    pub disconnect_reason: Option<DisconnectReason>,
    pub quality_metrics: QualityMetrics,
    pub billing_info: BillingInfo,
    pub routing_info: RoutingCdrInfo,
    pub media_info: MediaCdrInfo,
    pub compliance_info: ComplianceInfo,
}

/// Calling party category for billing purposes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallingPartyCategory {
    Unknown,
    Subscriber,
    National,
    International,
    Emergency,
    Test,
    PayPhone,
    Cellular,
    Prison,
    Hotel,
}

/// Call type classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallType {
    Voice,
    Video,
    Fax,
    Data,
    Emergency,
    Test,
    Conference,
}

/// Call disconnect reasons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DisconnectReason {
    Normal,
    Busy,
    NoAnswer,
    Rejected,
    NetworkError,
    Timeout,
    SystemError,
    UserDisconnect,
    ProviderDisconnect,
    Congestion,
    Forbidden,
    NotFound,
    ServerError,
}

/// Quality metrics for the call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityMetrics {
    pub mos_score: Option<f32>,          // Mean Opinion Score
    pub packet_loss_rate: f32,
    pub jitter_ms: f32,
    pub latency_ms: f32,
    pub codec_a: String,
    pub codec_b: String,
    pub rtp_packets_sent: u64,
    pub rtp_packets_received: u64,
    pub rtp_bytes_sent: u64,
    pub rtp_bytes_received: u64,
    pub transcoding_used: bool,
}

/// Billing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingInfo {
    pub account_id: String,
    pub rate_plan: String,
    pub rate_per_minute: f64,
    pub currency: String,
    pub cost: f64,
    pub tax_amount: f64,
    pub billing_increment_seconds: u32,
    pub minimum_charge_seconds: u32,
    pub carrier_cost: f64,
    pub margin: f64,
    pub billing_category: BillingCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BillingCategory {
    Local,
    National,
    International,
    Mobile,
    Premium,
    Toll,
    Emergency,
    Test,
}

/// Routing information for CDR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingCdrInfo {
    pub rule_id: String,
    pub route_type: RouteType,
    pub target_gateway: String,
    pub number_translation_applied: bool,
    pub routing_decision_time_ms: u64,
    pub failover_attempts: u32,
}

/// Media information for CDR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaCdrInfo {
    pub leg_a_codec: String,
    pub leg_b_codec: String,
    pub transcoding_backend: Option<String>,
    pub media_relay_used: bool,
    pub dtmf_events: Vec<DtmfEvent>,
    pub media_processing_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DtmfEvent {
    pub digit: char,
    pub timestamp: DateTime<Utc>,
    pub duration_ms: u32,
    pub direction: String,
}

/// Compliance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceInfo {
    pub jurisdiction: String,
    pub emergency_call: bool,
    pub lawful_intercept_required: bool,
    pub data_retention_class: DataRetentionClass,
    pub privacy_flags: PrivacyFlags,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataRetentionClass {
    Standard,      // 1 year
    Extended,      // 3 years
    Legal,         // 7 years
    Permanent,     // Indefinite
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyFlags {
    pub caller_id_blocked: bool,
    pub recording_enabled: bool,
    pub analytics_enabled: bool,
    pub location_tracking_enabled: bool,
}

/// Billing rate table entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingRate {
    pub id: String,
    pub prefix: String,
    pub description: String,
    pub rate_per_minute: f64,
    pub currency: String,
    pub billing_increment: u32,
    pub minimum_charge: u32,
    pub effective_date: DateTime<Utc>,
    pub expiry_date: Option<DateTime<Utc>>,
    pub category: BillingCategory,
}

/// CDR events
#[derive(Debug, Clone)]
pub enum CdrEvent {
    CallStarted {
        cdr_id: String,
        call_id: String,
        caller: String,
        callee: String,
    },
    CallAnswered {
        cdr_id: String,
        answer_time: DateTime<Utc>,
    },
    CallEnded {
        cdr_id: String,
        end_time: DateTime<Utc>,
        reason: DisconnectReason,
        duration: Duration,
    },
    CdrGenerated {
        cdr_id: String,
        call_duration: Duration,
        cost: f64,
    },
    BillingError {
        cdr_id: String,
        error: String,
    },
    RateNotFound {
        called_number: String,
        caller: String,
    },
    ComplianceAlert {
        cdr_id: String,
        alert_type: String,
        details: String,
    },
    Error {
        cdr_id: Option<String>,
        message: String,
    },
}

/// CDR storage backend
#[async_trait::async_trait]
pub trait CdrStorage: Send + Sync {
    async fn store_cdr(&self, cdr: &CallDetailRecord) -> Result<()>;
    async fn get_cdr(&self, cdr_id: &str) -> Result<Option<CallDetailRecord>>;
    async fn query_cdrs(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        filters: HashMap<String, String>,
    ) -> Result<Vec<CallDetailRecord>>;
    async fn aggregate_stats(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<CdrAggregateStats>;
}

#[derive(Debug, Clone)]
pub struct CdrAggregateStats {
    pub total_calls: u64,
    pub total_duration_seconds: u64,
    pub total_revenue: f64,
    pub average_call_duration: f64,
    pub calls_by_category: HashMap<String, u64>,
    pub revenue_by_category: HashMap<String, f64>,
}

/// File-based CDR storage
pub struct FileCdrStorage {
    base_path: PathBuf,
    rotation_size_mb: u64,
    current_file: Arc<RwLock<Option<std::fs::File>>>,
    current_file_size: Arc<RwLock<u64>>,
}

impl FileCdrStorage {
    pub fn new(base_path: PathBuf, rotation_size_mb: u64) -> Self {
        Self {
            base_path,
            rotation_size_mb,
            current_file: Arc::new(RwLock::new(None)),
            current_file_size: Arc::new(RwLock::new(0)),
        }
    }

    async fn get_current_file(&self) -> Result<std::fs::File> {
        let mut file_guard = self.current_file.write().await;
        let mut size_guard = self.current_file_size.write().await;

        // Check if we need to rotate the file
        if file_guard.is_none() || *size_guard > (self.rotation_size_mb * 1024 * 1024) {
            // Create new file with timestamp
            let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
            let filename = format!("cdr_{}.jsonl", timestamp);
            let filepath = self.base_path.join(filename);

            // Ensure directory exists
            if let Some(parent) = filepath.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let new_file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&filepath)?;

            *file_guard = Some(new_file);
            *size_guard = 0;

            info!("Created new CDR file: {:?}", filepath);
        }

        Ok(file_guard.as_ref().unwrap().try_clone()?)
    }
}

#[async_trait::async_trait]
impl CdrStorage for FileCdrStorage {
    async fn store_cdr(&self, cdr: &CallDetailRecord) -> Result<()> {
        let mut file = self.get_current_file().await?;
        let json_line = serde_json::to_string(cdr)?;
        let line_with_newline = format!("{}\n", json_line);
        
        file.write_all(line_with_newline.as_bytes())?;
        file.flush()?;

        // Update file size
        let mut size_guard = self.current_file_size.write().await;
        *size_guard += line_with_newline.len() as u64;

        Ok(())
    }

    async fn get_cdr(&self, _cdr_id: &str) -> Result<Option<CallDetailRecord>> {
        // Implementation would search through files
        // This is a simplified placeholder
        Ok(None)
    }

    async fn query_cdrs(
        &self,
        _start_time: DateTime<Utc>,
        _end_time: DateTime<Utc>,
        _filters: HashMap<String, String>,
    ) -> Result<Vec<CallDetailRecord>> {
        // Implementation would parse files and filter
        // This is a simplified placeholder
        Ok(vec![])
    }

    async fn aggregate_stats(
        &self,
        _start_time: DateTime<Utc>,
        _end_time: DateTime<Utc>,
    ) -> Result<CdrAggregateStats> {
        // Implementation would aggregate data from files
        // This is a simplified placeholder
        Ok(CdrAggregateStats {
            total_calls: 0,
            total_duration_seconds: 0,
            total_revenue: 0.0,
            average_call_duration: 0.0,
            calls_by_category: HashMap::new(),
            revenue_by_category: HashMap::new(),
        })
    }
}

/// CDR and billing service
pub struct CdrService {
    active_cdrs: Arc<DashMap<String, CallDetailRecord>>,
    billing_rates: Arc<RwLock<Vec<BillingRate>>>,
    storage: Arc<dyn CdrStorage>,
    event_tx: mpsc::UnboundedSender<CdrEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<CdrEvent>>,
    default_billing_config: BillingConfig,
    is_running: bool,
}

#[derive(Debug, Clone)]
pub struct BillingConfig {
    pub default_rate_per_minute: f64,
    pub default_currency: String,
    pub default_billing_increment: u32,
    pub default_minimum_charge: u32,
    pub tax_rate: f64,
    pub default_account_id: String,
}

impl Default for BillingConfig {
    fn default() -> Self {
        Self {
            default_rate_per_minute: 0.10,
            default_currency: "USD".to_string(),
            default_billing_increment: 60,
            default_minimum_charge: 60,
            tax_rate: 0.0,
            default_account_id: "default".to_string(),
        }
    }
}

impl CdrService {
    pub fn new(
        storage: Arc<dyn CdrStorage>,
        billing_config: BillingConfig,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Self {
            active_cdrs: Arc::new(DashMap::new()),
            billing_rates: Arc::new(RwLock::new(Vec::new())),
            storage,
            event_tx,
            event_rx: Some(event_rx),
            default_billing_config: billing_config,
            is_running: false,
        }
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<CdrEvent>> {
        self.event_rx.take()
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("Starting CDR service");

        // Start CDR finalization task
        let active_cdrs_finalizer = Arc::clone(&self.active_cdrs);
        let storage_finalizer = Arc::clone(&self.storage);
        let event_tx_finalizer = self.event_tx.clone();

        tokio::spawn(async move {
            Self::cdr_finalizer_loop(
                active_cdrs_finalizer,
                storage_finalizer,
                event_tx_finalizer,
            ).await;
        });

        self.is_running = true;
        info!("CDR service started successfully");
        Ok(())
    }

    pub async fn start_call_record(
        &self,
        call: &B2buaCall,
        caller_category: CallingPartyCategory,
        call_type: CallType,
    ) -> Result<String> {
        let cdr_id = Uuid::new_v4().to_string();
        let start_time = Utc::now();

        // Determine billing information
        let billing_info = self.calculate_billing_info(
            &call.callee,
            &call.caller,
            call_type.clone(),
        ).await?;

        let cdr = CallDetailRecord {
            id: cdr_id.clone(),
            call_id: call.id.clone(),
            session_id: call.leg_a_session_id.clone(),
            caller: call.caller.clone(),
            callee: call.callee.clone(),
            original_called_number: call.callee.clone(),
            translated_called_number: call.callee.clone(), // Would be updated by routing
            calling_party_category: caller_category,
            call_type,
            route_type: call.routing_info.route_type.clone(),
            start_time,
            answer_time: None,
            end_time: None,
            duration_seconds: 0,
            billable_duration_seconds: 0,
            disconnect_reason: None,
            quality_metrics: QualityMetrics {
                mos_score: None,
                packet_loss_rate: 0.0,
                jitter_ms: 0.0,
                latency_ms: 0.0,
                codec_a: "unknown".to_string(),
                codec_b: "unknown".to_string(),
                rtp_packets_sent: 0,
                rtp_packets_received: 0,
                rtp_bytes_sent: 0,
                rtp_bytes_received: 0,
                transcoding_used: false,
            },
            billing_info,
            routing_info: RoutingCdrInfo {
                rule_id: "unknown".to_string(),
                route_type: call.routing_info.route_type.clone(),
                target_gateway: call.routing_info.target_gateway.clone().unwrap_or_default(),
                number_translation_applied: call.routing_info.number_translation.is_some(),
                routing_decision_time_ms: 0,
                failover_attempts: 0,
            },
            media_info: MediaCdrInfo {
                leg_a_codec: "unknown".to_string(),
                leg_b_codec: "unknown".to_string(),
                transcoding_backend: None,
                media_relay_used: false,
                dtmf_events: Vec::new(),
                media_processing_enabled: false,
            },
            compliance_info: ComplianceInfo {
                jurisdiction: "US".to_string(),
                emergency_call: matches!(call.routing_info.route_type, RouteType::Emergency),
                lawful_intercept_required: false,
                data_retention_class: if matches!(call.routing_info.route_type, RouteType::Emergency) {
                    DataRetentionClass::Legal
                } else {
                    DataRetentionClass::Standard
                },
                privacy_flags: PrivacyFlags {
                    caller_id_blocked: false,
                    recording_enabled: false,
                    analytics_enabled: true,
                    location_tracking_enabled: false,
                },
            },
        };

        self.active_cdrs.insert(cdr_id.clone(), cdr);

        // Emit call started event
        let _ = self.event_tx.send(CdrEvent::CallStarted {
            cdr_id: cdr_id.clone(),
            call_id: call.id.clone(),
            caller: call.caller.clone(),
            callee: call.callee.clone(),
        });

        info!("Started CDR record: {} for call {}", cdr_id, call.id);
        Ok(cdr_id)
    }

    pub async fn update_call_answered(&self, cdr_id: &str) -> Result<()> {
        if let Some(mut cdr) = self.active_cdrs.get_mut(cdr_id) {
            let answer_time = Utc::now();
            cdr.answer_time = Some(answer_time);

            // Emit call answered event
            let _ = self.event_tx.send(CdrEvent::CallAnswered {
                cdr_id: cdr_id.to_string(),
                answer_time,
            });

            debug!("Updated CDR {} with answer time", cdr_id);
        }

        Ok(())
    }

    pub async fn update_media_info(
        &self,
        cdr_id: &str,
        media_stats: &MediaRelayStats,
        transcoding_backend: Option<&str>,
    ) -> Result<()> {
        if let Some(mut cdr) = self.active_cdrs.get_mut(cdr_id) {
            // Update quality metrics
            cdr.quality_metrics.codec_a = media_stats.codec_a.to_name().to_string();
            cdr.quality_metrics.codec_b = media_stats.codec_b.to_name().to_string();
            cdr.quality_metrics.rtp_packets_sent = media_stats.packets_relayed_a_to_b + media_stats.packets_relayed_b_to_a;
            cdr.quality_metrics.rtp_packets_received = media_stats.packets_relayed_a_to_b + media_stats.packets_relayed_b_to_a;
            cdr.quality_metrics.rtp_bytes_sent = media_stats.bytes_relayed_a_to_b + media_stats.bytes_relayed_b_to_a;
            cdr.quality_metrics.rtp_bytes_received = media_stats.bytes_relayed_a_to_b + media_stats.bytes_relayed_b_to_a;
            cdr.quality_metrics.packet_loss_rate = media_stats.packet_loss_rate as f32;
            cdr.quality_metrics.transcoding_used = transcoding_backend.is_some();

            // Update media info
            cdr.media_info.leg_a_codec = media_stats.codec_a.to_name().to_string();
            cdr.media_info.leg_b_codec = media_stats.codec_b.to_name().to_string();
            cdr.media_info.transcoding_backend = transcoding_backend.map(|s| s.to_string());
            cdr.media_info.media_relay_used = true;

            debug!("Updated CDR {} with media information", cdr_id);
        }

        Ok(())
    }

    pub async fn finalize_call_record(
        &self,
        cdr_id: &str,
        end_time: DateTime<Utc>,
        disconnect_reason: DisconnectReason,
    ) -> Result<()> {
        if let Some((_, mut cdr)) = self.active_cdrs.remove(cdr_id) {
            cdr.end_time = Some(end_time);
            cdr.disconnect_reason = Some(disconnect_reason.clone());

            // Calculate duration
            let duration = if let Some(answer_time) = cdr.answer_time {
                end_time.signed_duration_since(answer_time)
            } else {
                end_time.signed_duration_since(cdr.start_time)
            };

            cdr.duration_seconds = duration.num_seconds().max(0) as u64;

            // Calculate billable duration
            cdr.billable_duration_seconds = self.calculate_billable_duration(
                cdr.duration_seconds,
                &cdr.billing_info,
            );

            // Calculate final cost
            cdr.billing_info.cost = self.calculate_call_cost(
                cdr.billable_duration_seconds,
                &cdr.billing_info,
            );

            // Store CDR
            if let Err(e) = self.storage.store_cdr(&cdr).await {
                error!("Failed to store CDR {}: {}", cdr_id, e);
                let _ = self.event_tx.send(CdrEvent::Error {
                    cdr_id: Some(cdr_id.to_string()),
                    message: format!("Failed to store CDR: {}", e),
                });
            } else {
                // Emit events
                let _ = self.event_tx.send(CdrEvent::CallEnded {
                    cdr_id: cdr_id.to_string(),
                    end_time,
                    reason: disconnect_reason,
                    duration: Duration::from_secs(cdr.duration_seconds),
                });

                let _ = self.event_tx.send(CdrEvent::CdrGenerated {
                    cdr_id: cdr_id.to_string(),
                    call_duration: Duration::from_secs(cdr.duration_seconds),
                    cost: cdr.billing_info.cost,
                });

                info!("Finalized CDR: {} (duration: {}s, cost: ${:.2})",
                    cdr_id, cdr.duration_seconds, cdr.billing_info.cost);
            }
        }

        Ok(())
    }

    async fn calculate_billing_info(
        &self,
        called_number: &str,
        caller: &str,
        call_type: CallType,
    ) -> Result<BillingInfo> {
        // Find applicable billing rate
        let rate = self.find_billing_rate(called_number).await;

        let (rate_per_minute, category, billing_increment, minimum_charge) = match rate {
            Some(r) => (
                r.rate_per_minute,
                r.category,
                r.billing_increment,
                r.minimum_charge,
            ),
            None => {
                // Emit rate not found event
                let _ = self.event_tx.send(CdrEvent::RateNotFound {
                    called_number: called_number.to_string(),
                    caller: caller.to_string(),
                });

                (
                    self.default_billing_config.default_rate_per_minute,
                    BillingCategory::Local,
                    self.default_billing_config.default_billing_increment,
                    self.default_billing_config.default_minimum_charge,
                )
            }
        };

        Ok(BillingInfo {
            account_id: self.default_billing_config.default_account_id.clone(),
            rate_plan: "standard".to_string(),
            rate_per_minute,
            currency: self.default_billing_config.default_currency.clone(),
            cost: 0.0, // Will be calculated at end of call
            tax_amount: 0.0,
            billing_increment_seconds: billing_increment,
            minimum_charge_seconds: minimum_charge,
            carrier_cost: rate_per_minute * 0.7, // 70% of retail rate
            margin: rate_per_minute * 0.3,       // 30% margin
            billing_category: category,
        })
    }

    async fn find_billing_rate(&self, called_number: &str) -> Option<BillingRate> {
        let rates = self.billing_rates.read().await;
        
        // Find the longest matching prefix
        let mut best_match: Option<&BillingRate> = None;
        let mut best_prefix_length = 0;

        for rate in rates.iter() {
            if called_number.starts_with(&rate.prefix) && rate.prefix.len() > best_prefix_length {
                best_match = Some(rate);
                best_prefix_length = rate.prefix.len();
            }
        }

        best_match.cloned()
    }

    fn calculate_billable_duration(&self, actual_duration: u64, billing_info: &BillingInfo) -> u64 {
        let increment = billing_info.billing_increment_seconds as u64;
        let minimum = billing_info.minimum_charge_seconds as u64;

        // Apply minimum charge
        let duration = actual_duration.max(minimum);

        // Round up to billing increment
        if increment > 0 {
            ((duration + increment - 1) / increment) * increment
        } else {
            duration
        }
    }

    fn calculate_call_cost(&self, billable_duration: u64, billing_info: &BillingInfo) -> f64 {
        let minutes = billable_duration as f64 / 60.0;
        let base_cost = minutes * billing_info.rate_per_minute;
        let tax = base_cost * self.default_billing_config.tax_rate;
        base_cost + tax
    }

    async fn cdr_finalizer_loop(
        active_cdrs: Arc<DashMap<String, CallDetailRecord>>,
        storage: Arc<dyn CdrStorage>,
        event_tx: mpsc::UnboundedSender<CdrEvent>,
    ) {
        let mut finalizer_interval = interval(Duration::from_secs(300)); // 5 minutes

        loop {
            finalizer_interval.tick().await;
            let now = Utc::now();
            let max_age = chrono::Duration::hours(24); // Auto-finalize after 24 hours

            // Find CDRs that should be auto-finalized
            let to_finalize: Vec<(String, CallDetailRecord)> = active_cdrs
                .iter()
                .filter(|entry| {
                    now.signed_duration_since(entry.value().start_time) > max_age
                })
                .map(|entry| (entry.key().clone(), entry.value().clone()))
                .collect();

            for (cdr_id, mut cdr) in to_finalize {
                // Auto-finalize with timeout reason
                cdr.end_time = Some(now);
                cdr.disconnect_reason = Some(DisconnectReason::Timeout);
                cdr.duration_seconds = max_age.num_seconds() as u64;

                // Remove from active CDRs
                active_cdrs.remove(&cdr_id);

                // Store CDR
                if let Err(e) = storage.store_cdr(&cdr).await {
                    error!("Failed to auto-finalize CDR {}: {}", cdr_id, e);
                } else {
                    let _ = event_tx.send(CdrEvent::CallEnded {
                        cdr_id: cdr_id.clone(),
                        end_time: now,
                        reason: DisconnectReason::Timeout,
                        duration: Duration::from_secs(cdr.duration_seconds),
                    });

                    warn!("Auto-finalized stale CDR: {}", cdr_id);
                }
            }
        }
    }

    // Public API methods
    pub async fn load_billing_rates(&self, rates: Vec<BillingRate>) -> Result<()> {
        let mut rates_guard = self.billing_rates.write().await;
        *rates_guard = rates;
        info!("Loaded {} billing rates", rates_guard.len());
        Ok(())
    }

    pub async fn get_active_cdr_count(&self) -> usize {
        self.active_cdrs.len()
    }

    pub async fn get_cdr_statistics(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<CdrAggregateStats> {
        self.storage.aggregate_stats(start_time, end_time).await
    }

    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping CDR service");

        // Finalize all active CDRs
        let now = Utc::now();
        let active_cdr_ids: Vec<String> = self.active_cdrs.iter().map(|entry| entry.key().clone()).collect();
        
        for cdr_id in active_cdr_ids {
            let _ = self.finalize_call_record(&cdr_id, now, DisconnectReason::SystemError).await;
        }

        self.is_running = false;
        info!("CDR service stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_file_cdr_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileCdrStorage::new(temp_dir.path().to_path_buf(), 10);

        let cdr = CallDetailRecord {
            id: "test-cdr".to_string(),
            call_id: "test-call".to_string(),
            session_id: "test-session".to_string(),
            caller: "1000".to_string(),
            callee: "2000".to_string(),
            original_called_number: "2000".to_string(),
            translated_called_number: "2000".to_string(),
            calling_party_category: CallingPartyCategory::Subscriber,
            call_type: CallType::Voice,
            route_type: RouteType::Direct,
            start_time: Utc::now(),
            answer_time: Some(Utc::now()),
            end_time: Some(Utc::now()),
            duration_seconds: 60,
            billable_duration_seconds: 60,
            disconnect_reason: Some(DisconnectReason::Normal),
            quality_metrics: QualityMetrics {
                mos_score: Some(4.2),
                packet_loss_rate: 0.1,
                jitter_ms: 20.0,
                latency_ms: 50.0,
                codec_a: "G711U".to_string(),
                codec_b: "G711U".to_string(),
                rtp_packets_sent: 3000,
                rtp_packets_received: 2950,
                rtp_bytes_sent: 480000,
                rtp_bytes_received: 472000,
                transcoding_used: false,
            },
            billing_info: BillingInfo {
                account_id: "test-account".to_string(),
                rate_plan: "standard".to_string(),
                rate_per_minute: 0.10,
                currency: "USD".to_string(),
                cost: 0.10,
                tax_amount: 0.0,
                billing_increment_seconds: 60,
                minimum_charge_seconds: 60,
                carrier_cost: 0.07,
                margin: 0.03,
                billing_category: BillingCategory::Local,
            },
            routing_info: RoutingCdrInfo {
                rule_id: "local-rule".to_string(),
                route_type: RouteType::Direct,
                target_gateway: "local".to_string(),
                number_translation_applied: false,
                routing_decision_time_ms: 5,
                failover_attempts: 0,
            },
            media_info: MediaCdrInfo {
                leg_a_codec: "G711U".to_string(),
                leg_b_codec: "G711U".to_string(),
                transcoding_backend: None,
                media_relay_used: true,
                dtmf_events: vec![],
                media_processing_enabled: false,
            },
            compliance_info: ComplianceInfo {
                jurisdiction: "US".to_string(),
                emergency_call: false,
                lawful_intercept_required: false,
                data_retention_class: DataRetentionClass::Standard,
                privacy_flags: PrivacyFlags {
                    caller_id_blocked: false,
                    recording_enabled: false,
                    analytics_enabled: true,
                    location_tracking_enabled: false,
                },
            },
        };

        let result = storage.store_cdr(&cdr).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_billing_calculation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(FileCdrStorage::new(temp_dir.path().to_path_buf(), 10));
        
        let service = CdrService::new(storage, BillingConfig::default());
        
        let billing_info = BillingInfo {
            account_id: "test".to_string(),
            rate_plan: "standard".to_string(),
            rate_per_minute: 0.10,
            currency: "USD".to_string(),
            cost: 0.0,
            tax_amount: 0.0,
            billing_increment_seconds: 60,
            minimum_charge_seconds: 60,
            carrier_cost: 0.07,
            margin: 0.03,
            billing_category: BillingCategory::Local,
        };

        // Test billable duration calculation
        let billable = service.calculate_billable_duration(45, &billing_info);
        assert_eq!(billable, 60); // Should round up to minimum

        let billable2 = service.calculate_billable_duration(90, &billing_info);
        assert_eq!(billable2, 120); // Should round up to next increment

        // Test cost calculation
        let cost = service.calculate_call_cost(120, &billing_info);
        assert_eq!(cost, 0.20); // 2 minutes * $0.10/minute
    }
}