//! Alarm management system for the gateway

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

use crate::{Error, Result};

/// Alarm severity levels
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlarmSeverity {
    Critical,
    Major,
    Minor,
    Warning,
    Indeterminate,
    Cleared,
}

/// Alarm types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlarmType {
    Equipment,
    Environmental,
    Processing,
    Quality,
    Communication,
    Security,
}

/// Alarm states
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AlarmState {
    Active,
    Cleared,
    Acknowledged,
    Suppressed,
}

/// Alarm source information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmSource {
    pub component: String,
    pub instance: String,
    pub location: Option<String>,
}

/// Individual alarm entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alarm {
    pub id: String,
    pub sequence_number: u64,
    pub severity: AlarmSeverity,
    pub alarm_type: AlarmType,
    pub state: AlarmState,
    pub source: AlarmSource,
    pub description: String,
    pub additional_info: HashMap<String, String>,
    pub raised_time: DateTime<Utc>,
    pub cleared_time: Option<DateTime<Utc>>,
    pub acknowledged_time: Option<DateTime<Utc>>,
    pub acknowledged_by: Option<String>,
    pub probable_cause: Option<String>,
    pub proposed_repair_action: Option<String>,
    pub event_count: u32,
    pub last_event_time: DateTime<Utc>,
}

/// Alarm filter criteria
#[derive(Debug, Clone)]
pub struct AlarmFilter {
    pub severity: Option<AlarmSeverity>,
    pub alarm_type: Option<AlarmType>,
    pub state: Option<AlarmState>,
    pub component: Option<String>,
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
}

/// Alarm statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmStatistics {
    pub total_alarms: u64,
    pub active_alarms: u64,
    pub critical_alarms: u64,
    pub major_alarms: u64,
    pub minor_alarms: u64,
    pub warning_alarms: u64,
    pub cleared_alarms: u64,
    pub acknowledged_alarms: u64,
    pub suppressed_alarms: u64,
    pub alarms_by_type: HashMap<AlarmType, u64>,
    pub alarms_by_component: HashMap<String, u64>,
}

/// Alarm events
#[derive(Debug, Clone)]
pub enum AlarmEvent {
    AlarmRaised(Alarm),
    AlarmCleared { id: String, cleared_by: String },
    AlarmAcknowledged { id: String, acknowledged_by: String },
    AlarmSuppressed { id: String, suppressed_by: String },
    AlarmUnsuppressed { id: String, unsuppressed_by: String },
    AlarmUpdated(Alarm),
    StatisticsUpdated(AlarmStatistics),
}

/// Alarm configuration
#[derive(Debug, Clone)]
pub struct AlarmConfig {
    pub max_active_alarms: usize,
    pub max_history_size: usize,
    pub auto_acknowledge_timeout: Option<Duration>,
    pub alarm_aging_timeout: Duration,
    pub enable_notification: bool,
    pub notification_endpoints: Vec<String>,
}

impl Default for AlarmConfig {
    fn default() -> Self {
        Self {
            max_active_alarms: 10000,
            max_history_size: 50000,
            auto_acknowledge_timeout: None,
            alarm_aging_timeout: Duration::from_secs(86400 * 30), // 30 days
            enable_notification: true,
            notification_endpoints: Vec::new(),
        }
    }
}

/// Alarm management system
pub struct AlarmManager {
    config: AlarmConfig,
    active_alarms: Arc<RwLock<HashMap<String, Alarm>>>,
    alarm_history: Arc<RwLock<VecDeque<Alarm>>>,
    sequence_counter: Arc<RwLock<u64>>,
    event_tx: mpsc::UnboundedSender<AlarmEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<AlarmEvent>>,
    statistics: Arc<RwLock<AlarmStatistics>>,
}

impl AlarmManager {
    pub fn new(config: AlarmConfig) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        Self {
            config,
            active_alarms: Arc::new(RwLock::new(HashMap::new())),
            alarm_history: Arc::new(RwLock::new(VecDeque::new())),
            sequence_counter: Arc::new(RwLock::new(1)),
            event_tx,
            event_rx: Some(event_rx),
            statistics: Arc::new(RwLock::new(AlarmStatistics::default())),
        }
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<AlarmEvent>> {
        self.event_rx.take()
    }

    /// Raise a new alarm
    pub async fn raise_alarm(
        &self,
        severity: AlarmSeverity,
        alarm_type: AlarmType,
        source: AlarmSource,
        description: String,
        additional_info: Option<HashMap<String, String>>,
        probable_cause: Option<String>,
        proposed_repair_action: Option<String>,
    ) -> Result<String> {
        let now = Utc::now();
        let sequence = {
            let mut counter = self.sequence_counter.write().await;
            let seq = *counter;
            *counter += 1;
            seq
        };

        // Generate alarm ID based on source and description hash
        let alarm_id = format!("{}-{}-{:08x}", 
            source.component, 
            source.instance,
            self.hash_string(&format!("{}{}", description, 
                additional_info.as_ref().map(|m| format!("{:?}", m)).unwrap_or_default()))
        );

        // Check if this alarm already exists (duplicate detection)
        {
            let mut active_alarms = self.active_alarms.write().await;
            if let Some(existing_alarm) = active_alarms.get_mut(&alarm_id) {
                // Update existing alarm
                existing_alarm.event_count += 1;
                existing_alarm.last_event_time = now;
                existing_alarm.severity = severity.clone();
                
                if let Some(ref info) = additional_info {
                    existing_alarm.additional_info.extend(info.clone());
                }

                let _ = self.event_tx.send(AlarmEvent::AlarmUpdated(existing_alarm.clone()));
                return Ok(alarm_id);
            }
        }

        let alarm = Alarm {
            id: alarm_id.clone(),
            sequence_number: sequence,
            severity: severity.clone(),
            alarm_type: alarm_type.clone(),
            state: AlarmState::Active,
            source: source.clone(),
            description: description.clone(),
            additional_info: additional_info.unwrap_or_default(),
            raised_time: now,
            cleared_time: None,
            acknowledged_time: None,
            acknowledged_by: None,
            probable_cause,
            proposed_repair_action,
            event_count: 1,
            last_event_time: now,
        };

        // Add to active alarms
        {
            let mut active_alarms = self.active_alarms.write().await;
            
            // Check limits
            if active_alarms.len() >= self.config.max_active_alarms {
                warn!("Maximum active alarms limit reached: {}", self.config.max_active_alarms);
                return Err(Error::internal("Maximum active alarms limit reached"));
            }
            
            active_alarms.insert(alarm_id.clone(), alarm.clone());
        }

        // Update statistics
        self.update_statistics().await;

        // Send alarm event
        let _ = self.event_tx.send(AlarmEvent::AlarmRaised(alarm));

        info!("Alarm raised: {} - {} - {}", alarm_id, severity_to_string(&severity), description);

        Ok(alarm_id)
    }

    /// Clear an active alarm
    pub async fn clear_alarm(&self, alarm_id: &str, cleared_by: String) -> Result<()> {
        let now = Utc::now();
        
        let _alarm = {
            let mut active_alarms = self.active_alarms.write().await;
            if let Some(mut alarm) = active_alarms.remove(alarm_id) {
                alarm.state = AlarmState::Cleared;
                alarm.cleared_time = Some(now);
                alarm.severity = AlarmSeverity::Cleared;
                
                // Move to history
                {
                    let mut history = self.alarm_history.write().await;
                    history.push_back(alarm.clone());
                    
                    // Limit history size
                    while history.len() > self.config.max_history_size {
                        history.pop_front();
                    }
                }
                
                alarm
            } else {
                return Err(Error::internal(format!("Alarm not found: {}", alarm_id)));
            }
        };

        // Update statistics
        self.update_statistics().await;

        // Send clear event
        let _ = self.event_tx.send(AlarmEvent::AlarmCleared {
            id: alarm_id.to_string(),
            cleared_by: cleared_by.clone(),
        });

        info!("Alarm cleared: {} by {}", alarm_id, cleared_by);

        Ok(())
    }

    /// Acknowledge an alarm
    pub async fn acknowledge_alarm(&self, alarm_id: &str, acknowledged_by: String) -> Result<()> {
        let now = Utc::now();
        
        {
            let mut active_alarms = self.active_alarms.write().await;
            if let Some(alarm) = active_alarms.get_mut(alarm_id) {
                alarm.state = AlarmState::Acknowledged;
                alarm.acknowledged_time = Some(now);
                alarm.acknowledged_by = Some(acknowledged_by.clone());
            } else {
                return Err(Error::internal(format!("Alarm not found: {}", alarm_id)));
            }
        }

        // Update statistics
        self.update_statistics().await;

        // Send acknowledge event
        let _ = self.event_tx.send(AlarmEvent::AlarmAcknowledged {
            id: alarm_id.to_string(),
            acknowledged_by: acknowledged_by.clone(),
        });

        info!("Alarm acknowledged: {} by {}", alarm_id, acknowledged_by);

        Ok(())
    }

    /// Suppress an alarm
    pub async fn suppress_alarm(&self, alarm_id: &str, suppressed_by: String) -> Result<()> {
        {
            let mut active_alarms = self.active_alarms.write().await;
            if let Some(alarm) = active_alarms.get_mut(alarm_id) {
                alarm.state = AlarmState::Suppressed;
            } else {
                return Err(Error::internal(format!("Alarm not found: {}", alarm_id)));
            }
        }

        // Update statistics
        self.update_statistics().await;

        // Send suppress event
        let _ = self.event_tx.send(AlarmEvent::AlarmSuppressed {
            id: alarm_id.to_string(),
            suppressed_by: suppressed_by.clone(),
        });

        info!("Alarm suppressed: {} by {}", alarm_id, suppressed_by);

        Ok(())
    }

    /// Get all active alarms
    pub async fn get_active_alarms(&self) -> Vec<Alarm> {
        let active_alarms = self.active_alarms.read().await;
        active_alarms.values().cloned().collect()
    }

    /// Get active alarms with filter
    pub async fn get_filtered_alarms(&self, filter: &AlarmFilter) -> Vec<Alarm> {
        let active_alarms = self.active_alarms.read().await;
        
        active_alarms.values()
            .filter(|alarm| self.matches_filter(alarm, filter))
            .cloned()
            .collect()
    }

    /// Get alarm by ID
    pub async fn get_alarm(&self, alarm_id: &str) -> Option<Alarm> {
        let active_alarms = self.active_alarms.read().await;
        active_alarms.get(alarm_id).cloned()
    }

    /// Get alarm history
    pub async fn get_alarm_history(&self, limit: Option<usize>) -> Vec<Alarm> {
        let history = self.alarm_history.read().await;
        let iter = history.iter().rev();
        
        match limit {
            Some(n) => iter.take(n).cloned().collect(),
            None => iter.cloned().collect(),
        }
    }

    /// Get alarm statistics
    pub async fn get_statistics(&self) -> AlarmStatistics {
        let statistics = self.statistics.read().await;
        statistics.clone()
    }

    /// Clear all alarms for a component
    pub async fn clear_component_alarms(&self, component: &str, cleared_by: String) -> Result<u32> {
        let alarm_ids: Vec<String> = {
            let active_alarms = self.active_alarms.read().await;
            active_alarms.values()
                .filter(|alarm| alarm.source.component == component)
                .map(|alarm| alarm.id.clone())
                .collect()
        };

        let mut cleared_count = 0;
        for alarm_id in alarm_ids {
            if self.clear_alarm(&alarm_id, cleared_by.clone()).await.is_ok() {
                cleared_count += 1;
            }
        }

        Ok(cleared_count)
    }

    fn matches_filter(&self, alarm: &Alarm, filter: &AlarmFilter) -> bool {
        if let Some(ref severity) = filter.severity {
            if &alarm.severity != severity {
                return false;
            }
        }

        if let Some(ref alarm_type) = filter.alarm_type {
            if &alarm.alarm_type != alarm_type {
                return false;
            }
        }

        if let Some(ref state) = filter.state {
            if &alarm.state != state {
                return false;
            }
        }

        if let Some(ref component) = filter.component {
            if &alarm.source.component != component {
                return false;
            }
        }

        if let Some((start, end)) = filter.time_range {
            if alarm.raised_time < start || alarm.raised_time > end {
                return false;
            }
        }

        true
    }

    async fn update_statistics(&self) {
        let active_alarms = self.active_alarms.read().await;
        let history = self.alarm_history.read().await;
        
        let mut stats = AlarmStatistics::default();
        
        // Count active alarms by severity and type
        for alarm in active_alarms.values() {
            stats.active_alarms += 1;
            
            match alarm.severity {
                AlarmSeverity::Critical => stats.critical_alarms += 1,
                AlarmSeverity::Major => stats.major_alarms += 1,
                AlarmSeverity::Minor => stats.minor_alarms += 1,
                AlarmSeverity::Warning => stats.warning_alarms += 1,
                _ => {}
            }

            match alarm.state {
                AlarmState::Acknowledged => stats.acknowledged_alarms += 1,
                AlarmState::Suppressed => stats.suppressed_alarms += 1,
                _ => {}
            }

            *stats.alarms_by_type.entry(alarm.alarm_type.clone()).or_insert(0) += 1;
            *stats.alarms_by_component.entry(alarm.source.component.clone()).or_insert(0) += 1;
        }

        // Count cleared alarms from history
        stats.cleared_alarms = history.len() as u64;
        stats.total_alarms = stats.active_alarms + stats.cleared_alarms;

        {
            let mut statistics = self.statistics.write().await;
            *statistics = stats.clone();
        }

        // Send statistics update event
        let _ = self.event_tx.send(AlarmEvent::StatisticsUpdated(stats));
    }

    fn hash_string(&self, s: &str) -> u32 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish() as u32
    }
}

impl Default for AlarmStatistics {
    fn default() -> Self {
        Self {
            total_alarms: 0,
            active_alarms: 0,
            critical_alarms: 0,
            major_alarms: 0,
            minor_alarms: 0,
            warning_alarms: 0,
            cleared_alarms: 0,
            acknowledged_alarms: 0,
            suppressed_alarms: 0,
            alarms_by_type: HashMap::new(),
            alarms_by_component: HashMap::new(),
        }
    }
}

fn severity_to_string(severity: &AlarmSeverity) -> &'static str {
    match severity {
        AlarmSeverity::Critical => "CRITICAL",
        AlarmSeverity::Major => "MAJOR",
        AlarmSeverity::Minor => "MINOR",
        AlarmSeverity::Warning => "WARNING",
        AlarmSeverity::Indeterminate => "INDETERMINATE",
        AlarmSeverity::Cleared => "CLEARED",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_alarm_manager_creation() {
        let config = AlarmConfig::default();
        let manager = AlarmManager::new(config);
        
        let stats = manager.get_statistics().await;
        assert_eq!(stats.total_alarms, 0);
        assert_eq!(stats.active_alarms, 0);
    }

    #[tokio::test]
    async fn test_raise_alarm() {
        let config = AlarmConfig::default();
        let manager = AlarmManager::new(config);
        
        let source = AlarmSource {
            component: "test-component".to_string(),
            instance: "1".to_string(),
            location: None,
        };

        let alarm_id = manager.raise_alarm(
            AlarmSeverity::Critical,
            AlarmType::Equipment,
            source,
            "Test alarm".to_string(),
            None,
            None,
            None,
        ).await.unwrap();

        assert!(!alarm_id.is_empty());

        let alarms = manager.get_active_alarms().await;
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0].severity, AlarmSeverity::Critical);
        assert_eq!(alarms[0].description, "Test alarm");
    }

    #[tokio::test]
    async fn test_clear_alarm() {
        let config = AlarmConfig::default();
        let manager = AlarmManager::new(config);
        
        let source = AlarmSource {
            component: "test-component".to_string(),
            instance: "1".to_string(),
            location: None,
        };

        let alarm_id = manager.raise_alarm(
            AlarmSeverity::Major,
            AlarmType::Equipment,
            source,
            "Test alarm".to_string(),
            None,
            None,
            None,
        ).await.unwrap();

        manager.clear_alarm(&alarm_id, "test-user".to_string()).await.unwrap();

        let alarms = manager.get_active_alarms().await;
        assert_eq!(alarms.len(), 0);

        let history = manager.get_alarm_history(None).await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].state, AlarmState::Cleared);
    }
}