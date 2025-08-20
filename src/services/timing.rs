//! Timing and Clock Synchronization Service
//! 
//! Provides comprehensive timing support including:
//! - Internal stratum clock sources
//! - GPS timing synchronization
//! - TDMoE clock recovery and distribution
//! - Network timing protocols (NTP, PTP)
//! - Clock quality monitoring and alarms

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tracing::{debug, info, warn};

use crate::{Error, Result};

/// Stratum levels for clock quality
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StratumLevel {
    /// Stratum 0 - Reference clocks (GPS, atomic clocks)
    Stratum0,
    /// Stratum 1 - Primary servers synchronized to Stratum 0
    Stratum1,
    /// Stratum 2 - Secondary servers synchronized to Stratum 1
    Stratum2,
    /// Stratum 3 - Tertiary servers
    Stratum3,
    /// Stratum 4 - Quaternary servers (lowest reliable level)
    Stratum4,
    /// Invalid/unsynchronized
    Invalid,
}

impl StratumLevel {
    pub fn accuracy_ppm(&self) -> f64 {
        match self {
            StratumLevel::Stratum0 => 0.0001,  // 0.1 ppm
            StratumLevel::Stratum1 => 0.001,   // 1 ppm
            StratumLevel::Stratum2 => 0.01,    // 10 ppm
            StratumLevel::Stratum3 => 0.1,     // 100 ppm
            StratumLevel::Stratum4 => 1.0,     // 1000 ppm
            StratumLevel::Invalid => f64::INFINITY,
        }
    }

    pub fn max_drift_ns_per_sec(&self) -> u64 {
        (self.accuracy_ppm() * 1000.0) as u64
    }
}

/// Clock source types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClockSourceType {
    /// Internal crystal oscillator
    Internal {
        frequency: u64,  // Hz
        temperature_compensated: bool,
        aging_rate_ppm_per_year: f64,
    },
    /// GPS timing receiver
    Gps {
        device_path: String,
        antenna_status: GpsAntennaStatus,
        satellite_count: u8,
        fix_type: GpsFixType,
    },
    /// Network Time Protocol
    Ntp {
        servers: Vec<String>,
        poll_interval: Duration,
        last_sync: Option<DateTime<Utc>>,
    },
    /// Precision Time Protocol (IEEE 1588)
    Ptp {
        domain: u8,
        role: PtpRole,
        master_clock_id: Option<String>,
    },
    /// TDMoE recovered clock
    TdmoeRecovered {
        source_span: u32,
        quality: TdmClockQuality,
        slip_count: u64,
    },
    /// External reference clock
    External {
        interface: String,
        frequency: u64,
        signal_type: String, // "10MHz", "E1", "T1", etc.
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GpsAntennaStatus {
    Ok,
    ShortCircuit,
    OpenCircuit,
    NotConnected,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GpsFixType {
    NoFix,
    Fix2D,
    Fix3D,
    DifferentialFix,
    PrecisionTimeProtocol,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PtpRole {
    Master,
    Slave,
    Boundary,
    Transparent,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TdmClockQuality {
    /// Primary reference source (GPS, atomic clock)
    Primary,
    /// High-quality secondary source  
    Secondary,
    /// Medium-quality tertiary source
    Tertiary,
    /// Low-quality or degraded source
    Degraded,
    /// No usable timing signal
    Invalid,
}

impl TdmClockQuality {
    pub fn to_stratum_level(&self) -> StratumLevel {
        match self {
            TdmClockQuality::Primary => StratumLevel::Stratum1,
            TdmClockQuality::Secondary => StratumLevel::Stratum2,
            TdmClockQuality::Tertiary => StratumLevel::Stratum3,
            TdmClockQuality::Degraded => StratumLevel::Stratum4,
            TdmClockQuality::Invalid => StratumLevel::Invalid,
        }
    }
}

/// Clock status and health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockStatus {
    pub source_id: String,
    pub source_type: ClockSourceType,
    pub stratum_level: StratumLevel,
    pub is_active: bool,
    pub is_holdover: bool,
    pub last_sync: Option<DateTime<Utc>>,
    pub frequency_offset_ppb: i64,  // parts per billion
    pub phase_offset_ns: i64,       // nanoseconds
    pub time_error_ns: u64,         // absolute error estimate
    pub allan_variance: f64,        // frequency stability metric
    pub temperature_c: Option<f32>,
    pub signal_strength_db: Option<f32>,
    pub error_count: u64,
    pub sync_count: u64,
    pub uptime: Duration,
}

/// Timing events for monitoring and alarms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimingEvent {
    ClockSourceAdded {
        source_id: String,
        source_type: ClockSourceType,
    },
    ClockSourceRemoved {
        source_id: String,
    },
    ClockSourceSelected {
        source_id: String,
        stratum_level: StratumLevel,
    },
    ClockSynchronized {
        source_id: String,
        offset_ns: i64,
        accuracy_ns: u64,
    },
    ClockLossOfSync {
        source_id: String,
        duration: Duration,
    },
    ClockHoldover {
        source_id: String,
        reason: String,
    },
    FrequencyDrift {
        source_id: String,
        drift_ppb: i64,
        threshold_ppb: i64,
    },
    GpsSignalLost {
        satellite_count: u8,
        last_fix: DateTime<Utc>,
    },
    GpsSignalRestored {
        satellite_count: u8,
        fix_type: GpsFixType,
    },
    TdmClockSlip {
        span_id: u32,
        slip_type: String, // "positive", "negative"
        accumulated_slips: u64,
    },
    StratumLevelChanged {
        old_stratum: StratumLevel,
        new_stratum: StratumLevel,
    },
}

/// Timing service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingConfig {
    pub enable_internal_clock: bool,
    pub enable_gps: bool,
    pub enable_ntp: bool,
    pub enable_ptp: bool,
    pub gps_device: String,
    pub ntp_servers: Vec<String>,
    pub ptp_domain: u8,
    pub clock_selection_algorithm: ClockSelectionAlgorithm,
    pub holdover_duration: Duration,
    pub frequency_correction_enabled: bool,
    pub phase_correction_enabled: bool,
    pub max_frequency_offset_ppb: i64,
    pub max_phase_offset_ns: i64,
    pub monitoring_interval: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ClockSelectionAlgorithm {
    /// Select highest stratum (best quality)
    HighestStratum,
    /// Select lowest accumulated error
    LowestError,
    /// Select most stable (lowest Allan variance)
    MostStable,
    /// Manual selection
    Manual,
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self {
            enable_internal_clock: true,
            enable_gps: false,
            enable_ntp: true,
            enable_ptp: false,
            gps_device: "/dev/ttyUSB0".to_string(),
            ntp_servers: vec![
                "pool.ntp.org".to_string(),
                "time.google.com".to_string(),
            ],
            ptp_domain: 0,
            clock_selection_algorithm: ClockSelectionAlgorithm::HighestStratum,
            holdover_duration: Duration::from_secs(24 * 3600),
            frequency_correction_enabled: true,
            phase_correction_enabled: true,
            max_frequency_offset_ppb: 100_000, // 100 ppm
            max_phase_offset_ns: 1_000_000,    // 1 ms
            monitoring_interval: Duration::from_secs(10),
        }
    }
}

/// Timing service for comprehensive clock management
pub struct TimingService {
    config: Arc<RwLock<TimingConfig>>,
    clock_sources: Arc<RwLock<HashMap<String, ClockStatus>>>,
    selected_clock: Arc<RwLock<Option<String>>>,
    system_stratum: Arc<RwLock<StratumLevel>>,
    reference_time: Arc<RwLock<SystemTime>>,
    frequency_offset: Arc<RwLock<i64>>, // ppb
    phase_offset: Arc<RwLock<i64>>,     // ns
    event_tx: mpsc::UnboundedSender<TimingEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<TimingEvent>>,
    is_running: bool,
}

impl TimingService {
    pub fn new(config: TimingConfig) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        Self {
            config: Arc::new(RwLock::new(config)),
            clock_sources: Arc::new(RwLock::new(HashMap::new())),
            selected_clock: Arc::new(RwLock::new(None)),
            system_stratum: Arc::new(RwLock::new(StratumLevel::Invalid)),
            reference_time: Arc::new(RwLock::new(SystemTime::now())),
            frequency_offset: Arc::new(RwLock::new(0)),
            phase_offset: Arc::new(RwLock::new(0)),
            event_tx,
            event_rx: Some(event_rx),
            is_running: false,
        }
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<TimingEvent>> {
        self.event_rx.take()
    }

    /// Start the timing service
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting timing service");
        self.is_running = true;

        // Initialize clock sources based on configuration
        let config = self.config.read().await;
        
        if config.enable_internal_clock {
            self.add_internal_clock().await?;
        }

        if config.enable_gps {
            self.add_gps_source(&config.gps_device).await?;
        }

        if config.enable_ntp {
            for server in &config.ntp_servers {
                self.add_ntp_source(server.clone()).await?;
            }
        }

        if config.enable_ptp {
            self.add_ptp_source(config.ptp_domain).await?;
        }

        // Start monitoring tasks
        self.start_monitoring_tasks().await?;

        Ok(())
    }

    /// Stop the timing service
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping timing service");
        self.is_running = false;
        Ok(())
    }

    /// Add internal clock source
    async fn add_internal_clock(&self) -> Result<()> {
        let source_id = "internal".to_string();
        let source_type = ClockSourceType::Internal {
            frequency: 25_000_000, // 25 MHz crystal
            temperature_compensated: true,
            aging_rate_ppm_per_year: 1.0,
        };

        let status = ClockStatus {
            source_id: source_id.clone(),
            source_type: source_type.clone(),
            stratum_level: StratumLevel::Stratum4, // Internal is lowest priority
            is_active: true,
            is_holdover: false,
            last_sync: Some(Utc::now()),
            frequency_offset_ppb: 0,
            phase_offset_ns: 0,
            time_error_ns: 1_000_000, // 1ms initial uncertainty
            allan_variance: 1e-9,
            temperature_c: Some(45.0),
            signal_strength_db: None,
            error_count: 0,
            sync_count: 1,
            uptime: Duration::from_secs(0),
        };

        {
            let mut sources = self.clock_sources.write().await;
            sources.insert(source_id.clone(), status);
        }

        let _ = self.event_tx.send(TimingEvent::ClockSourceAdded {
            source_id,
            source_type,
        });

        info!("Added internal clock source");
        Ok(())
    }

    /// Add GPS timing source
    pub async fn add_gps_source(&self, device_path: &str) -> Result<()> {
        let source_id = "gps".to_string();
        let source_type = ClockSourceType::Gps {
            device_path: device_path.to_string(),
            antenna_status: GpsAntennaStatus::Unknown,
            satellite_count: 0,
            fix_type: GpsFixType::NoFix,
        };

        let status = ClockStatus {
            source_id: source_id.clone(),
            source_type: source_type.clone(),
            stratum_level: StratumLevel::Stratum1, // GPS is high quality
            is_active: false, // Will be activated when GPS lock is acquired
            is_holdover: false,
            last_sync: None,
            frequency_offset_ppb: 0,
            phase_offset_ns: 0,
            time_error_ns: 100_000, // 100μs typical GPS accuracy
            allan_variance: 1e-12,  // GPS has excellent stability
            temperature_c: None,
            signal_strength_db: Some(-30.0), // Typical GPS signal strength
            error_count: 0,
            sync_count: 0,
            uptime: Duration::from_secs(0),
        };

        {
            let mut sources = self.clock_sources.write().await;
            sources.insert(source_id.clone(), status);
        }

        let _ = self.event_tx.send(TimingEvent::ClockSourceAdded {
            source_id,
            source_type,
        });

        info!("Added GPS timing source: {}", device_path);
        Ok(())
    }

    /// Add NTP source
    pub async fn add_ntp_source(&self, server: String) -> Result<()> {
        let source_id = format!("ntp-{}", server);
        let source_type = ClockSourceType::Ntp {
            servers: vec![server.clone()],
            poll_interval: Duration::from_secs(64),
            last_sync: None,
        };

        let status = ClockStatus {
            source_id: source_id.clone(),
            source_type: source_type.clone(),
            stratum_level: StratumLevel::Stratum3, // Assume Stratum 3 for network sources
            is_active: false,
            is_holdover: false,
            last_sync: None,
            frequency_offset_ppb: 0,
            phase_offset_ns: 0,
            time_error_ns: 10_000_000, // 10ms typical NTP accuracy
            allan_variance: 1e-6,
            temperature_c: None,
            signal_strength_db: None,
            error_count: 0,
            sync_count: 0,
            uptime: Duration::from_secs(0),
        };

        {
            let mut sources = self.clock_sources.write().await;
            sources.insert(source_id.clone(), status);
        }

        let _ = self.event_tx.send(TimingEvent::ClockSourceAdded {
            source_id,
            source_type,
        });

        info!("Added NTP timing source: {}", server);
        Ok(())
    }

    /// Add PTP source
    pub async fn add_ptp_source(&self, domain: u8) -> Result<()> {
        let source_id = format!("ptp-domain-{}", domain);
        let source_type = ClockSourceType::Ptp {
            domain,
            role: PtpRole::Slave,
            master_clock_id: None,
        };

        let status = ClockStatus {
            source_id: source_id.clone(),
            source_type: source_type.clone(),
            stratum_level: StratumLevel::Stratum2, // PTP typically provides good accuracy
            is_active: false,
            is_holdover: false,
            last_sync: None,
            frequency_offset_ppb: 0,
            phase_offset_ns: 0,
            time_error_ns: 1_000, // 1μs typical PTP accuracy
            allan_variance: 1e-9,
            temperature_c: None,
            signal_strength_db: None,
            error_count: 0,
            sync_count: 0,
            uptime: Duration::from_secs(0),
        };

        {
            let mut sources = self.clock_sources.write().await;
            sources.insert(source_id.clone(), status);
        }

        let _ = self.event_tx.send(TimingEvent::ClockSourceAdded {
            source_id,
            source_type,
        });

        info!("Added PTP timing source: domain {}", domain);
        Ok(())
    }

    /// Add TDMoE recovered clock source
    pub async fn add_tdmoe_clock_source(&self, span_id: u32, quality: TdmClockQuality) -> Result<()> {
        let source_id = format!("tdmoe-span-{}", span_id);
        let source_type = ClockSourceType::TdmoeRecovered {
            source_span: span_id,
            quality,
            slip_count: 0,
        };

        let status = ClockStatus {
            source_id: source_id.clone(),
            source_type: source_type.clone(),
            stratum_level: quality.to_stratum_level(),
            is_active: true,
            is_holdover: false,
            last_sync: Some(Utc::now()),
            frequency_offset_ppb: 0,
            phase_offset_ns: 0,
            time_error_ns: match quality {
                TdmClockQuality::Primary => 1_000,      // 1μs
                TdmClockQuality::Secondary => 10_000,    // 10μs
                TdmClockQuality::Tertiary => 100_000,    // 100μs
                TdmClockQuality::Degraded => 1_000_000,  // 1ms
                TdmClockQuality::Invalid => u64::MAX,
            },
            allan_variance: match quality {
                TdmClockQuality::Primary => 1e-11,
                TdmClockQuality::Secondary => 1e-9,
                TdmClockQuality::Tertiary => 1e-7,
                TdmClockQuality::Degraded => 1e-5,
                TdmClockQuality::Invalid => 1.0,
            },
            temperature_c: None,
            signal_strength_db: None,
            error_count: 0,
            sync_count: 1,
            uptime: Duration::from_secs(0),
        };

        {
            let mut sources = self.clock_sources.write().await;
            sources.insert(source_id.clone(), status);
        }

        let _ = self.event_tx.send(TimingEvent::ClockSourceAdded {
            source_id,
            source_type,
        });

        info!("Added TDMoE clock source: span {} quality {:?}", span_id, quality);
        
        // Trigger clock selection
        self.select_best_clock().await?;
        
        Ok(())
    }

    /// Remove a clock source
    pub async fn remove_clock_source(&self, source_id: &str) -> Result<()> {
        {
            let mut sources = self.clock_sources.write().await;
            if sources.remove(source_id).is_some() {
                info!("Removed clock source: {}", source_id);
                
                let _ = self.event_tx.send(TimingEvent::ClockSourceRemoved {
                    source_id: source_id.to_string(),
                });
            }
        }

        // If the removed source was selected, choose a new one
        {
            let selected = self.selected_clock.read().await;
            if selected.as_ref().map_or(false, |s| s == source_id) {
                drop(selected);
                self.select_best_clock().await?;
            }
        }

        Ok(())
    }

    /// Select the best available clock source
    async fn select_best_clock(&self) -> Result<()> {
        let sources = self.clock_sources.read().await;
        let config = self.config.read().await;
        
        let best_source = match config.clock_selection_algorithm {
            ClockSelectionAlgorithm::HighestStratum => {
                sources.values()
                    .filter(|s| s.is_active)
                    .min_by_key(|s| s.stratum_level as u8)
            },
            ClockSelectionAlgorithm::LowestError => {
                sources.values()
                    .filter(|s| s.is_active)
                    .min_by_key(|s| s.time_error_ns)
            },
            ClockSelectionAlgorithm::MostStable => {
                sources.values()
                    .filter(|s| s.is_active)
                    .min_by(|a, b| a.allan_variance.partial_cmp(&b.allan_variance).unwrap())
            },
            ClockSelectionAlgorithm::Manual => {
                // Manual selection requires external intervention
                return Ok(());
            },
        };

        if let Some(source) = best_source {
            let mut selected = self.selected_clock.write().await;
            let mut stratum = self.system_stratum.write().await;
            
            let old_selection = selected.clone();
            *selected = Some(source.source_id.clone());
            *stratum = source.stratum_level;
            
            if old_selection.as_ref() != Some(&source.source_id) {
                let _ = self.event_tx.send(TimingEvent::ClockSourceSelected {
                    source_id: source.source_id.clone(),
                    stratum_level: source.stratum_level,
                });
                
                info!("Selected clock source: {} (Stratum {:?})", 
                      source.source_id, source.stratum_level);
            }
        } else {
            let mut selected = self.selected_clock.write().await;
            let mut stratum = self.system_stratum.write().await;
            
            *selected = None;
            *stratum = StratumLevel::Invalid;
            
            warn!("No active clock sources available");
        }

        Ok(())
    }

    /// Start monitoring tasks
    async fn start_monitoring_tasks(&self) -> Result<()> {
        let service = self.clone();
        tokio::spawn(async move {
            service.monitor_clock_sources().await;
        });

        let service = self.clone();
        tokio::spawn(async move {
            service.monitor_gps_receiver().await;
        });

        let service = self.clone();
        tokio::spawn(async move {
            service.monitor_ntp_sources().await;
        });

        Ok(())
    }

    /// Monitor clock sources for health and performance
    async fn monitor_clock_sources(&self) {
        let mut interval = interval(Duration::from_secs(10));
        
        while self.is_running {
            interval.tick().await;
            
            let mut sources = self.clock_sources.write().await;
            let config = self.config.read().await;
            
            for (source_id, status) in sources.iter_mut() {
                // Update uptime
                status.uptime += Duration::from_secs(10);
                
                // Check for frequency drift
                if status.frequency_offset_ppb.abs() > config.max_frequency_offset_ppb {
                    let _ = self.event_tx.send(TimingEvent::FrequencyDrift {
                        source_id: source_id.clone(),
                        drift_ppb: status.frequency_offset_ppb,
                        threshold_ppb: config.max_frequency_offset_ppb,
                    });
                }
                
                // Check for loss of sync
                if let Some(last_sync) = status.last_sync {
                    let since_sync = Utc::now() - last_sync;
                    if since_sync > chrono::Duration::minutes(5) && !status.is_holdover {
                        status.is_holdover = true;
                        let _ = self.event_tx.send(TimingEvent::ClockHoldover {
                            source_id: source_id.clone(),
                            reason: "No sync for 5 minutes".to_string(),
                        });
                    }
                }
                
                // Simulate some measurement updates
                self.update_clock_measurements(status).await;
            }
        }
    }

    /// Monitor GPS receiver
    async fn monitor_gps_receiver(&self) {
        let mut interval = interval(Duration::from_secs(30));
        
        while self.is_running {
            interval.tick().await;
            
            // Simulate GPS receiver monitoring
            let mut sources = self.clock_sources.write().await;
            if let Some(gps_status) = sources.get_mut("gps") {
                if let ClockSourceType::Gps { 
                    antenna_status, 
                    satellite_count, 
                    fix_type,
                    .. 
                } = &mut gps_status.source_type {
                    
                    // Simulate GPS status updates
                    let was_active = gps_status.is_active;
                    
                    // Simulate varying satellite count and fix quality
                    *satellite_count = 4 + (rand::random::<u8>() % 8); // 4-11 satellites
                    *antenna_status = if rand::random::<f32>() > 0.95 {
                        GpsAntennaStatus::OpenCircuit
                    } else {
                        GpsAntennaStatus::Ok
                    };
                    
                    *fix_type = if *satellite_count >= 4 && *antenna_status == GpsAntennaStatus::Ok {
                        if *satellite_count >= 8 {
                            GpsFixType::Fix3D
                        } else {
                            GpsFixType::Fix2D
                        }
                    } else {
                        GpsFixType::NoFix
                    };
                    
                    gps_status.is_active = *fix_type != GpsFixType::NoFix;
                    
                    if gps_status.is_active {
                        gps_status.last_sync = Some(Utc::now());
                        gps_status.sync_count += 1;
                        gps_status.time_error_ns = match fix_type {
                            GpsFixType::Fix3D => 50_000,        // 50μs
                            GpsFixType::Fix2D => 100_000,       // 100μs
                            GpsFixType::DifferentialFix => 10_000, // 10μs
                            _ => 1_000_000, // 1ms
                        };
                        
                        if !was_active {
                            let _ = self.event_tx.send(TimingEvent::GpsSignalRestored {
                                satellite_count: *satellite_count,
                                fix_type: *fix_type,
                            });
                        }
                    } else if was_active {
                        let _ = self.event_tx.send(TimingEvent::GpsSignalLost {
                            satellite_count: *satellite_count,
                            last_fix: gps_status.last_sync.unwrap_or_else(Utc::now),
                        });
                    }
                }
            }
        }
    }

    /// Monitor NTP sources
    async fn monitor_ntp_sources(&self) {
        let mut interval = interval(Duration::from_secs(60));
        
        while self.is_running {
            interval.tick().await;
            
            // Simulate NTP monitoring - in real implementation would query NTP servers
            let mut sources = self.clock_sources.write().await;
            for (source_id, status) in sources.iter_mut() {
                if source_id.starts_with("ntp-") {
                    // Simulate NTP sync
                    status.is_active = rand::random::<f32>() > 0.1; // 90% availability
                    if status.is_active {
                        status.last_sync = Some(Utc::now());
                        status.sync_count += 1;
                        status.phase_offset_ns = (rand::random::<i32>() % 20_000_000) as i64; // ±20ms
                        status.time_error_ns = 5_000_000 + (rand::random::<u32>() % 10_000_000) as u64; // 5-15ms
                    }
                }
            }
        }
    }

    /// Update clock measurements (simulated)
    async fn update_clock_measurements(&self, status: &mut ClockStatus) {
        // Simulate realistic clock behavior
        match &mut status.source_type {
            ClockSourceType::Internal { aging_rate_ppm_per_year, .. } => {
                // Internal clocks drift over time
                let aging_per_sec = *aging_rate_ppm_per_year / (365.25 * 24.0 * 3600.0);
                status.frequency_offset_ppb += (aging_per_sec * 1000.0) as i64;
                
                // Add some noise
                status.frequency_offset_ppb += (rand::random::<i32>() % 100 - 50) as i64;
                status.phase_offset_ns += (rand::random::<i32>() % 1000 - 500) as i64;
            },
            
            ClockSourceType::Gps { .. } => {
                if status.is_active {
                    // GPS provides very stable timing
                    status.frequency_offset_ppb = (rand::random::<i32>() % 10 - 5) as i64;
                    status.phase_offset_ns = (rand::random::<i32>() % 100 - 50) as i64;
                }
            },
            
            ClockSourceType::TdmoeRecovered { slip_count, source_span, .. } => {
                // TDM clocks can experience slips
                if rand::random::<f32>() < 0.001 { // 0.1% chance of slip per measurement
                    *slip_count += 1;
                    let slip_type = if rand::random::<bool>() { "positive" } else { "negative" };
                    
                    let _ = self.event_tx.send(TimingEvent::TdmClockSlip {
                        span_id: *source_span,
                        slip_type: slip_type.to_string(),
                        accumulated_slips: *slip_count,
                    });
                }
            },
            
            _ => {
                // Default behavior for other clock types
                status.frequency_offset_ppb += (rand::random::<i32>() % 20 - 10) as i64;
                status.phase_offset_ns += (rand::random::<i32>() % 200 - 100) as i64;
            }
        }
        
        // Update Allan variance (simplified simulation)
        status.allan_variance = status.allan_variance * 0.99 + 
            (status.frequency_offset_ppb as f64 * 1e-9).abs() * 0.01;
    }

    /// Get current system time with corrections
    pub async fn get_corrected_time(&self) -> SystemTime {
        let phase_offset = *self.phase_offset.read().await;
        let base_time = *self.reference_time.read().await;
        
        // Apply phase correction
        if phase_offset.is_positive() {
            base_time + Duration::from_nanos(phase_offset as u64)
        } else {
            base_time - Duration::from_nanos((-phase_offset) as u64)
        }
    }

    /// Get current stratum level
    pub async fn get_stratum_level(&self) -> StratumLevel {
        *self.system_stratum.read().await
    }

    /// Get selected clock source
    pub async fn get_selected_clock(&self) -> Option<String> {
        self.selected_clock.read().await.clone()
    }

    /// Get all clock sources
    pub async fn get_clock_sources(&self) -> HashMap<String, ClockStatus> {
        self.clock_sources.read().await.clone()
    }

    /// Get clock source by ID
    pub async fn get_clock_source(&self, source_id: &str) -> Option<ClockStatus> {
        let sources = self.clock_sources.read().await;
        sources.get(source_id).cloned()
    }

    /// Force clock source selection
    pub async fn select_clock_source(&self, source_id: &str) -> Result<()> {
        let sources = self.clock_sources.read().await;
        if sources.contains_key(source_id) {
            let mut selected = self.selected_clock.write().await;
            *selected = Some(source_id.to_string());
            
            let _ = self.event_tx.send(TimingEvent::ClockSourceSelected {
                source_id: source_id.to_string(),
                stratum_level: sources[source_id].stratum_level,
            });
            
            info!("Manually selected clock source: {}", source_id);
            Ok(())
        } else {
            Err(Error::invalid_state(format!("Clock source not found: {}", source_id)))
        }
    }

    /// Update TDMoE clock quality
    pub async fn update_tdmoe_clock_quality(&self, span_id: u32, quality: TdmClockQuality) -> Result<()> {
        let source_id = format!("tdmoe-span-{}", span_id);
        let mut sources = self.clock_sources.write().await;
        
        if let Some(status) = sources.get_mut(&source_id) {
            if let ClockSourceType::TdmoeRecovered { quality: ref mut q, .. } = &mut status.source_type {
                let old_stratum = status.stratum_level;
                *q = quality;
                status.stratum_level = quality.to_stratum_level();
                
                if old_stratum != status.stratum_level {
                    let _ = self.event_tx.send(TimingEvent::StratumLevelChanged {
                        old_stratum,
                        new_stratum: status.stratum_level,
                    });
                }
                
                debug!("Updated TDMoE clock quality for span {}: {:?}", span_id, quality);
            }
        }
        
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }
}

impl Clone for TimingService {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            clock_sources: Arc::clone(&self.clock_sources),
            selected_clock: Arc::clone(&self.selected_clock),
            system_stratum: Arc::clone(&self.system_stratum),
            reference_time: Arc::clone(&self.reference_time),
            frequency_offset: Arc::clone(&self.frequency_offset),
            phase_offset: Arc::clone(&self.phase_offset),
            event_tx: self.event_tx.clone(),
            event_rx: None, // Don't clone receiver
            is_running: self.is_running,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_timing_service_creation() {
        let config = TimingConfig::default();
        let service = TimingService::new(config);
        
        assert!(!service.is_running());
        assert_eq!(service.get_stratum_level().await, StratumLevel::Invalid);
    }

    #[tokio::test]
    async fn test_clock_source_management() {
        let config = TimingConfig::default();
        let mut service = TimingService::new(config);
        
        service.start().await.unwrap();
        
        // Should have internal clock
        let sources = service.get_clock_sources().await;
        assert!(sources.contains_key("internal"));
        
        // Add TDMoE clock
        service.add_tdmoe_clock_source(1, TdmClockQuality::Primary).await.unwrap();
        
        let sources = service.get_clock_sources().await;
        assert!(sources.contains_key("tdmoe-span-1"));
        
        // Should select the TDMoE clock (higher quality)
        let selected = service.get_selected_clock().await;
        assert_eq!(selected, Some("tdmoe-span-1".to_string()));
        
        service.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_stratum_levels() {
        assert!(StratumLevel::Stratum1 < StratumLevel::Stratum2);
        assert!(StratumLevel::Stratum0.accuracy_ppm() < StratumLevel::Stratum1.accuracy_ppm());
        
        let quality = TdmClockQuality::Primary;
        assert_eq!(quality.to_stratum_level(), StratumLevel::Stratum1);
    }
}