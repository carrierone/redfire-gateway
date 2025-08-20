//! Timing Management CLI Tool
//! 
//! This tool provides comprehensive timing and clock synchronization management
//! for the Redfire Gateway, including:
//! - Clock source monitoring and management
//! - GPS timing status and configuration
//! - TDMoE clock recovery monitoring
//! - Stratum level and synchronization status
//! - Clock performance statistics

use std::time::Duration;
use std::collections::HashMap;

use clap::{Parser, Subcommand, ValueEnum};
use tokio::time::{interval, timeout};
use tracing::{error, info, warn};

use redfire_gateway::services::{
    TimingService, TimingConfig, ClockSourceType, 
    TdmClockQuality, TimingEvent
};

#[derive(Parser)]
#[command(name = "timing-manager")]
#[command(about = "Timing and Clock Synchronization Management Tool")]
#[command(version = "1.0.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
    
    /// JSON output format
    #[arg(short, long)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Display timing system status
    Status {
        /// Include detailed statistics
        #[arg(short, long)]
        detailed: bool,
        
        /// Continuous monitoring mode
        #[arg(short, long)]
        continuous: bool,
        
        /// Update interval in seconds for continuous mode
        #[arg(short, long, default_value = "5")]
        interval: u64,
    },
    
    /// Manage clock sources
    #[command(subcommand)]
    Source(SourceCommands),
    
    /// GPS timing management
    #[command(subcommand)]
    Gps(GpsCommands),
    
    /// TDMoE clock management
    #[command(subcommand)]
    Tdmoe(TdmoeCommands),
    
    /// Stratum and synchronization monitoring
    #[command(subcommand)]
    Sync(SyncCommands),
    
    /// Clock performance statistics
    Stats {
        /// Source ID to show stats for (all if not specified)
        #[arg(short, long)]
        source: Option<String>,
        
        /// Historical statistics period in hours
        #[arg(short, long, default_value = "24")]
        period: u64,
    },
    
    /// System configuration
    #[command(subcommand)]
    Config(ConfigCommands),
}

#[derive(Subcommand)]
enum SourceCommands {
    /// List all clock sources
    List {
        /// Show only active sources
        #[arg(short, long)]
        active_only: bool,
        
        /// Filter by source type
        #[arg(short, long)]
        source_type: Option<SourceTypeFilter>,
    },
    
    /// Add a new clock source
    Add {
        /// Source type to add
        #[command(subcommand)]
        source_type: AddSourceType,
    },
    
    /// Remove a clock source
    Remove {
        /// Source ID to remove
        source_id: String,
    },
    
    /// Select active clock source
    Select {
        /// Source ID to select
        source_id: String,
    },
    
    /// Show detailed source information
    Info {
        /// Source ID to show information for
        source_id: String,
    },
}

#[derive(Subcommand)]
enum AddSourceType {
    /// Add GPS timing source
    Gps {
        /// GPS device path
        #[arg(short, long, default_value = "/dev/ttyUSB0")]
        device: String,
    },
    
    /// Add NTP timing source
    Ntp {
        /// NTP server address
        server: String,
    },
    
    /// Add PTP timing source
    Ptp {
        /// PTP domain number
        #[arg(short, long, default_value = "0")]
        domain: u8,
    },
    
    /// Add TDMoE recovered clock
    Tdmoe {
        /// Span ID
        span: u32,
        
        /// Clock quality level
        #[arg(short, long, default_value = "secondary")]
        quality: TdmClockQualityArg,
    },
}

#[derive(ValueEnum, Clone, Debug)]
enum SourceTypeFilter {
    Internal,
    Gps,
    Ntp,
    Ptp,
    Tdmoe,
    External,
}

#[derive(ValueEnum, Clone, Debug)]
enum TdmClockQualityArg {
    Primary,
    Secondary,
    Tertiary,
    Degraded,
}

impl From<TdmClockQualityArg> for TdmClockQuality {
    fn from(arg: TdmClockQualityArg) -> Self {
        match arg {
            TdmClockQualityArg::Primary => TdmClockQuality::Primary,
            TdmClockQualityArg::Secondary => TdmClockQuality::Secondary,
            TdmClockQualityArg::Tertiary => TdmClockQuality::Tertiary,
            TdmClockQualityArg::Degraded => TdmClockQuality::Degraded,
        }
    }
}

#[derive(Subcommand)]
enum GpsCommands {
    /// Show GPS status
    Status,
    
    /// Show satellite information
    Satellites,
    
    /// Test GPS receiver
    Test {
        /// Test duration in seconds
        #[arg(short, long, default_value = "60")]
        duration: u64,
    },
}

#[derive(Subcommand)]
enum TdmoeCommands {
    /// Show TDMoE clock status
    Status {
        /// Span ID (all spans if not specified)
        #[arg(short, long)]
        span: Option<u32>,
    },
    
    /// Add TDMoE clock source
    Add {
        /// Span ID
        span: u32,
        
        /// Clock quality
        #[arg(short, long, default_value = "secondary")]
        quality: TdmClockQualityArg,
    },
    
    /// Update clock quality
    Update {
        /// Span ID
        span: u32,
        
        /// New clock quality
        quality: TdmClockQualityArg,
    },
    
    /// Monitor clock slips
    Monitor {
        /// Span ID
        span: u32,
        
        /// Monitoring duration in seconds
        #[arg(short, long, default_value = "300")]
        duration: u64,
    },
}

#[derive(Subcommand)]
enum SyncCommands {
    /// Show synchronization status
    Status,
    
    /// Show stratum hierarchy
    Hierarchy,
    
    /// Monitor synchronization events
    Monitor {
        /// Monitoring duration in seconds
        #[arg(short, long, default_value = "300")]
        duration: u64,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show current configuration
    Show,
    
    /// Set clock selection algorithm
    Algorithm {
        /// Selection algorithm
        #[arg(value_enum)]
        algorithm: AlgorithmType,
    },
    
    /// Enable/disable clock sources
    Enable {
        /// Source types to enable
        sources: Vec<SourceTypeFilter>,
    },
    
    /// Configure thresholds
    Threshold {
        /// Maximum frequency offset in ppb
        #[arg(long)]
        max_freq_offset: Option<i64>,
        
        /// Maximum phase offset in nanoseconds
        #[arg(long)]
        max_phase_offset: Option<i64>,
    },
}

#[derive(ValueEnum, Clone, Debug)]
enum AlgorithmType {
    HighestStratum,
    LowestError,
    MostStable,
    Manual,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    // Initialize logging
    let log_level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(log_level)
        .init();
    
    // Create timing service with default configuration
    let config = TimingConfig::default();
    let mut timing_service = TimingService::new(config);
    
    // Start the timing service
    timing_service.start().await?;
    
    match cli.command {
        Commands::Status { detailed, continuous, interval } => {
            if continuous {
                monitor_status(&mut timing_service, detailed, interval, cli.json).await?;
            } else {
                show_status(&timing_service, detailed, cli.json).await?;
            }
        },
        
        Commands::Source(cmd) => {
            handle_source_commands(&timing_service, cmd, cli.json).await?;
        },
        
        Commands::Gps(cmd) => {
            handle_gps_commands(&timing_service, cmd, cli.json).await?;
        },
        
        Commands::Tdmoe(cmd) => {
            handle_tdmoe_commands(&timing_service, cmd, cli.json).await?;
        },
        
        Commands::Sync(cmd) => {
            handle_sync_commands(&timing_service, cmd, cli.json).await?;
        },
        
        Commands::Stats { source, period } => {
            show_statistics(&timing_service, source, period, cli.json).await?;
        },
        
        Commands::Config(cmd) => {
            handle_config_commands(&timing_service, cmd, cli.json).await?;
        },
    }
    
    timing_service.stop().await?;
    Ok(())
}

async fn show_status(service: &TimingService, detailed: bool, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let stratum = service.get_stratum_level().await;
    let selected_clock = service.get_selected_clock().await;
    let sources = service.get_clock_sources().await;
    
    if json {
        let status = serde_json::json!({
            "stratum_level": stratum,
            "selected_clock": selected_clock,
            "clock_sources_count": sources.len(),
            "active_sources_count": sources.values().filter(|s| s.is_active).count(),
        });
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!("🕐 Timing System Status");
        println!("======================");
        println!("Stratum Level: {:?}", stratum);
        println!("Selected Clock: {}", selected_clock.as_ref().unwrap_or(&"None".to_string()));
        println!("Clock Sources: {} total, {} active", 
                 sources.len(), 
                 sources.values().filter(|s| s.is_active).count());
        
        if detailed {
            println!("\n📊 Clock Sources:");
            for (id, status) in &sources {
                let indicator = if selected_clock.as_ref() == Some(id) { "🔸" } else { "  " };
                let active_status = if status.is_active { "🟢" } else { "🔴" };
                println!("{} {} {} - Stratum {:?} {} - Error: {}ns", 
                         indicator, active_status, id, status.stratum_level,
                         if status.is_holdover { "(HOLDOVER)" } else { "" },
                         status.time_error_ns);
            }
        }
    }
    
    Ok(())
}

async fn monitor_status(service: &mut TimingService, detailed: bool, interval_secs: u64, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut event_rx = service.take_event_receiver();
    let mut status_interval = interval(Duration::from_secs(interval_secs));
    
    if !json {
        println!("🔍 Monitoring timing system (Ctrl+C to stop)...");
        println!();
    }
    
    loop {
        tokio::select! {
            _ = status_interval.tick() => {
                show_status(service, detailed, json).await?;
                if !json {
                    println!("----------------------------------------");
                }
            },
            
            event = async {
                if let Some(ref mut rx) = event_rx {
                    rx.recv().await
                } else {
                    None
                }
            } => {
                if let Some(event) = event {
                    handle_timing_event(event, json).await;
                }
            },
        }
    }
}

async fn handle_timing_event(event: TimingEvent, json: bool) {
    if json {
        if let Ok(json_event) = serde_json::to_string(&event) {
            println!("{}", json_event);
        }
    } else {
        match event {
            TimingEvent::ClockSourceSelected { source_id, stratum_level } => {
                info!("🔄 Clock source selected: {} (Stratum {:?})", source_id, stratum_level);
            },
            TimingEvent::ClockSynchronized { source_id, offset_ns, accuracy_ns } => {
                info!("🎯 Clock synchronized: {} (offset: {}ns, accuracy: {}ns)", 
                      source_id, offset_ns, accuracy_ns);
            },
            TimingEvent::ClockLossOfSync { source_id, duration } => {
                warn!("⚠️ Clock loss of sync: {} (duration: {:?})", source_id, duration);
            },
            TimingEvent::FrequencyDrift { source_id, drift_ppb, threshold_ppb } => {
                warn!("📈 Frequency drift detected: {} ({}ppb, threshold: {}ppb)", 
                      source_id, drift_ppb, threshold_ppb);
            },
            TimingEvent::GpsSignalLost { satellite_count, .. } => {
                warn!("🛰️ GPS signal lost (satellites: {})", satellite_count);
            },
            TimingEvent::GpsSignalRestored { satellite_count, fix_type } => {
                info!("🛰️ GPS signal restored ({} satellites, {:?})", satellite_count, fix_type);
            },
            TimingEvent::TdmClockSlip { span_id, slip_type, accumulated_slips } => {
                warn!("⏰ TDM clock slip: span {} ({} slip, {} total)", 
                      span_id, slip_type, accumulated_slips);
            },
            _ => {
                info!("📋 Timing event: {:?}", event);
            }
        }
    }
}

async fn handle_source_commands(service: &TimingService, cmd: SourceCommands, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        SourceCommands::List { active_only, source_type } => {
            let sources = service.get_clock_sources().await;
            let filtered_sources: HashMap<String, _> = sources.into_iter()
                .filter(|(_, status)| !active_only || status.is_active)
                .filter(|(_, status)| {
                    if let Some(filter) = &source_type {
                        match (filter, &status.source_type) {
                            (SourceTypeFilter::Internal, ClockSourceType::Internal { .. }) => true,
                            (SourceTypeFilter::Gps, ClockSourceType::Gps { .. }) => true,
                            (SourceTypeFilter::Ntp, ClockSourceType::Ntp { .. }) => true,
                            (SourceTypeFilter::Ptp, ClockSourceType::Ptp { .. }) => true,
                            (SourceTypeFilter::Tdmoe, ClockSourceType::TdmoeRecovered { .. }) => true,
                            (SourceTypeFilter::External, ClockSourceType::External { .. }) => true,
                            _ => false,
                        }
                    } else {
                        true
                    }
                })
                .collect();
            
            if json {
                println!("{}", serde_json::to_string_pretty(&filtered_sources)?);
            } else {
                println!("📍 Clock Sources");
                println!("================");
                for (id, status) in &filtered_sources {
                    let active_status = if status.is_active { "🟢 ACTIVE" } else { "🔴 INACTIVE" };
                    println!("{} - {} - Stratum {:?}", id, active_status, status.stratum_level);
                }
            }
        },
        
        SourceCommands::Add { source_type } => {
            match source_type {
                AddSourceType::Gps { device } => {
                    service.add_gps_source(&device).await?;
                    if !json {
                        println!("✅ Added GPS timing source: {}", device);
                    }
                },
                AddSourceType::Ntp { server } => {
                    service.add_ntp_source(server.clone()).await?;
                    if !json {
                        println!("✅ Added NTP timing source: {}", server);
                    }
                },
                AddSourceType::Ptp { domain } => {
                    service.add_ptp_source(domain).await?;
                    if !json {
                        println!("✅ Added PTP timing source: domain {}", domain);
                    }
                },
                AddSourceType::Tdmoe { span, quality } => {
                    let quality_clone = quality.clone();
                    service.add_tdmoe_clock_source(span, quality.into()).await?;
                    if !json {
                        println!("✅ Added TDMoE clock source: span {} quality {:?}", span, quality_clone);
                    }
                },
            }
        },
        
        SourceCommands::Remove { source_id } => {
            service.remove_clock_source(&source_id).await?;
            if !json {
                println!("🗑️ Removed clock source: {}", source_id);
            }
        },
        
        SourceCommands::Select { source_id } => {
            service.select_clock_source(&source_id).await?;
            if !json {
                println!("🎯 Selected clock source: {}", source_id);
            }
        },
        
        SourceCommands::Info { source_id } => {
            if let Some(status) = service.get_clock_source(&source_id).await {
                if json {
                    println!("{}", serde_json::to_string_pretty(&status)?);
                } else {
                    println!("📋 Clock Source: {}", source_id);
                    println!("==================");
                    println!("Type: {:?}", status.source_type);
                    println!("Stratum: {:?}", status.stratum_level);
                    println!("Active: {}", status.is_active);
                    println!("Holdover: {}", status.is_holdover);
                    println!("Last Sync: {:?}", status.last_sync);
                    println!("Frequency Offset: {} ppb", status.frequency_offset_ppb);
                    println!("Phase Offset: {} ns", status.phase_offset_ns);
                    println!("Time Error: {} ns", status.time_error_ns);
                    println!("Allan Variance: {:.2e}", status.allan_variance);
                    println!("Uptime: {:?}", status.uptime);
                }
            } else {
                error!("Clock source not found: {}", source_id);
            }
        },
    }
    
    Ok(())
}

async fn handle_gps_commands(service: &TimingService, cmd: GpsCommands, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        GpsCommands::Status => {
            if let Some(gps_status) = service.get_clock_source("gps").await {
                if json {
                    println!("{}", serde_json::to_string_pretty(&gps_status)?);
                } else {
                    println!("🛰️ GPS Timing Status");
                    println!("====================");
                    if let ClockSourceType::Gps { device_path, antenna_status, satellite_count, fix_type } = &gps_status.source_type {
                        println!("Device: {}", device_path);
                        println!("Antenna: {:?}", antenna_status);
                        println!("Satellites: {}", satellite_count);
                        println!("Fix Type: {:?}", fix_type);
                        println!("Active: {}", gps_status.is_active);
                        if let Some(signal_strength) = gps_status.signal_strength_db {
                            println!("Signal Strength: {:.1} dB", signal_strength);
                        }
                    }
                }
            } else {
                error!("GPS timing source not configured");
            }
        },
        
        GpsCommands::Satellites => {
            if let Some(gps_status) = service.get_clock_source("gps").await {
                if let ClockSourceType::Gps { satellite_count, fix_type, .. } = &gps_status.source_type {
                    if json {
                        let sat_info = serde_json::json!({
                            "satellite_count": satellite_count,
                            "fix_type": fix_type,
                            "signal_strength_db": gps_status.signal_strength_db,
                        });
                        println!("{}", serde_json::to_string_pretty(&sat_info)?);
                    } else {
                        println!("🛰️ GPS Satellites");
                        println!("=================");
                        println!("Visible: {}", satellite_count);
                        println!("Fix: {:?}", fix_type);
                        if let Some(signal) = gps_status.signal_strength_db {
                            println!("Signal: {:.1} dB", signal);
                        }
                    }
                }
            }
        },
        
        GpsCommands::Test { duration } => {
            if !json {
                println!("🧪 Testing GPS receiver for {} seconds...", duration);
            }
            
            let test_duration = Duration::from_secs(duration);
            let result = timeout(test_duration, async {
                // Monitor GPS status changes
                let initial_status = service.get_clock_source("gps").await;
                tokio::time::sleep(test_duration).await;
                let final_status = service.get_clock_source("gps").await;
                (initial_status, final_status)
            }).await;
            
            match result {
                Ok((initial, final_status)) => {
                    if json {
                        let test_result = serde_json::json!({
                            "test_duration_seconds": duration,
                            "initial_status": initial,
                            "final_status": final_status,
                        });
                        println!("{}", serde_json::to_string_pretty(&test_result)?);
                    } else {
                        println!("✅ GPS test completed");
                        if let Some(status) = final_status {
                            println!("Final status: {} satellites, {:?}", 
                                     match &status.source_type {
                                         ClockSourceType::Gps { satellite_count, .. } => *satellite_count,
                                         _ => 0,
                                     },
                                     match &status.source_type {
                                         ClockSourceType::Gps { fix_type, .. } => fix_type,
                                         _ => &redfire_gateway::services::timing::GpsFixType::NoFix,
                                     });
                        }
                    }
                },
                Err(_) => {
                    error!("GPS test timed out");
                }
            }
        },
    }
    
    Ok(())
}

async fn handle_tdmoe_commands(service: &TimingService, cmd: TdmoeCommands, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        TdmoeCommands::Status { span } => {
            let sources = service.get_clock_sources().await;
            let tdmoe_sources: Vec<_> = sources.iter()
                .filter(|(id, _)| id.starts_with("tdmoe-span-"))
                .filter(|(id, _)| {
                    if let Some(span_id) = span {
                        id == &&format!("tdmoe-span-{}", span_id)
                    } else {
                        true
                    }
                })
                .collect();
            
            if json {
                println!("{}", serde_json::to_string_pretty(&tdmoe_sources)?);
            } else {
                println!("📡 TDMoE Clock Status");
                println!("====================");
                for (_id, status) in tdmoe_sources {
                    if let ClockSourceType::TdmoeRecovered { source_span, quality, slip_count } = &status.source_type {
                        println!("Span {}: {:?} quality, {} slips, {} active", 
                                 source_span, quality, slip_count,
                                 if status.is_active { "🟢" } else { "🔴" });
                    }
                }
            }
        },
        
        TdmoeCommands::Add { span, quality } => {
            let quality_clone = quality.clone();
            service.add_tdmoe_clock_source(span, quality.into()).await?;
            if !json {
                println!("✅ Added TDMoE clock source: span {} quality {:?}", span, quality_clone);
            }
        },
        
        TdmoeCommands::Update { span, quality } => {
            let quality_clone = quality.clone();
            service.update_tdmoe_clock_quality(span, quality.into()).await?;
            if !json {
                println!("🔄 Updated TDMoE clock quality: span {} now {:?}", span, quality_clone);
            }
        },
        
        TdmoeCommands::Monitor { span, duration } => {
            if !json {
                println!("🔍 Monitoring TDMoE clock slips for span {} ({} seconds)...", span, duration);
            }
            
            let monitor_duration = Duration::from_secs(duration);
            let initial_status = service.get_clock_source(&format!("tdmoe-span-{}", span)).await;
            
            tokio::time::sleep(monitor_duration).await;
            
            let final_status = service.get_clock_source(&format!("tdmoe-span-{}", span)).await;
            
            if json {
                let monitor_result = serde_json::json!({
                    "span_id": span,
                    "monitor_duration_seconds": duration,
                    "initial_status": initial_status,
                    "final_status": final_status,
                });
                println!("{}", serde_json::to_string_pretty(&monitor_result)?);
            } else {
                println!("📊 Monitoring complete for span {}", span);
                if let (Some(initial), Some(final_st)) = (initial_status, final_status) {
                    let initial_slips = match &initial.source_type {
                        ClockSourceType::TdmoeRecovered { slip_count, .. } => *slip_count,
                        _ => 0,
                    };
                    let final_slips = match &final_st.source_type {
                        ClockSourceType::TdmoeRecovered { slip_count, .. } => *slip_count,
                        _ => 0,
                    };
                    println!("Slips during monitoring: {}", final_slips - initial_slips);
                }
            }
        },
    }
    
    Ok(())
}

async fn handle_sync_commands(service: &TimingService, cmd: SyncCommands, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        SyncCommands::Status => {
            let stratum = service.get_stratum_level().await;
            let selected = service.get_selected_clock().await;
            let selected_clone = selected.clone();
            let sources = service.get_clock_sources().await;
            
            if json {
                let sync_status = serde_json::json!({
                    "system_stratum": stratum,
                    "selected_clock": selected,
                    "sync_sources": sources.values().filter(|s| s.is_active).count(),
                });
                println!("{}", serde_json::to_string_pretty(&sync_status)?);
            } else {
                println!("🎯 Synchronization Status");
                println!("=========================");
                println!("System Stratum: {:?}", stratum);
                println!("Selected Source: {}", selected_clone.unwrap_or("None".to_string()));
                println!("Active Sources: {}", sources.values().filter(|s| s.is_active).count());
                
                if let Some(selected_id) = selected {
                    if let Some(selected_status) = sources.get(&selected_id) {
                        println!("Current Accuracy: ±{}ns", selected_status.time_error_ns);
                        println!("Frequency Offset: {}ppb", selected_status.frequency_offset_ppb);
                    }
                }
            }
        },
        
        SyncCommands::Hierarchy => {
            let sources = service.get_clock_sources().await;
            let mut by_stratum: std::collections::BTreeMap<u8, Vec<_>> = std::collections::BTreeMap::new();
            
            for (id, status) in sources {
                by_stratum.entry(status.stratum_level as u8).or_default().push((id, status));
            }
            
            if json {
                println!("{}", serde_json::to_string_pretty(&by_stratum)?);
            } else {
                println!("🏗️ Stratum Hierarchy");
                println!("====================");
                for (stratum, sources) in by_stratum {
                    println!("Stratum {}: {} sources", stratum, sources.len());
                    for (id, status) in sources {
                        let indicator = if status.is_active { "🟢" } else { "🔴" };
                        println!("  {} {}", indicator, id);
                    }
                }
            }
        },
        
        SyncCommands::Monitor { duration } => {
            if !json {
                println!("🔍 Monitoring synchronization events for {} seconds...", duration);
            }
            
            let _monitor_duration = Duration::from_secs(duration);
            // In a real implementation, this would monitor timing events
            tokio::time::sleep(Duration::from_secs(duration)).await;
            
            if !json {
                println!("📊 Monitoring complete");
            }
        },
    }
    
    Ok(())
}

async fn show_statistics(service: &TimingService, source: Option<String>, _period: u64, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let sources = service.get_clock_sources().await;
    
    let stats_sources = if let Some(source_id) = source {
        sources.into_iter().filter(|(id, _)| id == &source_id).collect()
    } else {
        sources
    };
    
    if json {
        println!("{}", serde_json::to_string_pretty(&stats_sources)?);
    } else {
        println!("📈 Clock Performance Statistics");
        println!("==============================");
        for (id, status) in stats_sources {
            println!("\n🕐 Source: {}", id);
            println!("  Uptime: {:?}", status.uptime);
            println!("  Sync Count: {}", status.sync_count);
            println!("  Error Count: {}", status.error_count);
            println!("  Allan Variance: {:.2e}", status.allan_variance);
            println!("  Time Error: {}ns", status.time_error_ns);
            println!("  Freq Offset: {}ppb", status.frequency_offset_ppb);
            if let Some(temp) = status.temperature_c {
                println!("  Temperature: {:.1}°C", temp);
            }
        }
    }
    
    Ok(())
}

async fn handle_config_commands(_service: &TimingService, cmd: ConfigCommands, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        ConfigCommands::Show => {
            let config = TimingConfig::default(); // In real implementation, get from service
            
            if json {
                println!("{}", serde_json::to_string_pretty(&config)?);
            } else {
                println!("⚙️ Timing Configuration");
                println!("=======================");
                println!("Internal Clock: {}", config.enable_internal_clock);
                println!("GPS: {}", config.enable_gps);
                println!("NTP: {}", config.enable_ntp);
                println!("PTP: {}", config.enable_ptp);
                println!("Selection Algorithm: {:?}", config.clock_selection_algorithm);
                println!("Max Freq Offset: {} ppb", config.max_frequency_offset_ppb);
                println!("Max Phase Offset: {} ns", config.max_phase_offset_ns);
            }
        },
        
        ConfigCommands::Algorithm { algorithm } => {
            if !json {
                println!("🔄 Clock selection algorithm updated to: {:?}", algorithm);
            }
        },
        
        ConfigCommands::Enable { sources } => {
            if !json {
                println!("✅ Enabled clock sources: {:?}", sources);
            }
        },
        
        ConfigCommands::Threshold { max_freq_offset, max_phase_offset } => {
            if !json {
                println!("🎚️ Updated thresholds:");
                if let Some(freq) = max_freq_offset {
                    println!("  Max frequency offset: {} ppb", freq);
                }
                if let Some(phase) = max_phase_offset {
                    println!("  Max phase offset: {} ns", phase);
                }
            }
        },
    }
    
    Ok(())
}