//! Performance monitoring service for system metrics

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sysinfo::System;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Interval};
use tracing::{error, info, warn};

use crate::config::PerformanceConfig;
use crate::Result;

/// Performance metrics snapshot
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub timestamp: Instant,
    pub cpu_usage: f32,
    pub memory_usage: f64,
    pub memory_total: u64,
    pub memory_used: u64,
    pub load_average: f64,
    pub disk_usage: f64,
    pub disk_total: u64,
    pub disk_used: u64,
    pub network_bytes_sent: u64,
    pub network_bytes_received: u64,
    pub network_packets_sent: u64,
    pub network_packets_received: u64,
    pub network_errors_in: u64,
    pub network_errors_out: u64,
}

/// Performance thresholds for alerting
#[derive(Debug, Clone)]
pub struct PerformanceThresholds {
    pub cpu_warning: f32,
    pub cpu_critical: f32,
    pub memory_warning: f64,
    pub memory_critical: f64,
    pub disk_warning: f64,
    pub disk_critical: f64,
    pub load_warning: f64,
    pub load_critical: f64,
    pub network_error_rate: f64,
    pub network_utilization_warning: f64,
}

/// Performance alert levels
#[derive(Debug, Clone, PartialEq)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

/// Performance alert
#[derive(Debug, Clone)]
pub struct PerformanceAlert {
    pub timestamp: Instant,
    pub level: AlertLevel,
    pub metric: String,
    pub value: f64,
    pub threshold: f64,
    pub message: String,
}

/// Performance events
#[derive(Debug, Clone)]
pub enum PerformanceEvent {
    MetricsUpdate(PerformanceMetrics),
    Alert(PerformanceAlert),
    ThresholdExceeded {
        metric: String,
        value: f64,
        threshold: f64,
        level: AlertLevel,
    },
}

/// Performance monitoring service
pub struct PerformanceMonitor {
    config: PerformanceConfig,
    thresholds: PerformanceThresholds,
    metrics_history: Arc<RwLock<VecDeque<PerformanceMetrics>>>,
    system: System,
    event_tx: mpsc::UnboundedSender<PerformanceEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<PerformanceEvent>>,
    collection_interval: Option<Interval>,
    is_running: bool,
    last_network_stats: Option<NetworkStats>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct NetworkStats {
    timestamp: Instant,
    bytes_sent: u64,
    bytes_received: u64,
    packets_sent: u64,
    packets_received: u64,
    errors_in: u64,
    errors_out: u64,
}

impl PerformanceMonitor {
    pub fn new(config: PerformanceConfig) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        let thresholds = PerformanceThresholds {
            cpu_warning: config.thresholds.cpu.warning as f32,
            cpu_critical: config.thresholds.cpu.critical as f32,
            memory_warning: config.thresholds.memory.warning,
            memory_critical: config.thresholds.memory.critical,
            disk_warning: config.thresholds.disk.warning,
            disk_critical: config.thresholds.disk.critical,
            load_warning: config.thresholds.load.warning,
            load_critical: config.thresholds.load.critical,
            network_error_rate: config.thresholds.network.error_rate,
            network_utilization_warning: config.thresholds.network.utilization_warning,
        };

        let mut system = System::new_all();
        system.refresh_all();

        Ok(Self {
            config,
            thresholds,
            metrics_history: Arc::new(RwLock::new(VecDeque::new())),
            system,
            event_tx,
            event_rx: Some(event_rx),
            collection_interval: None,
            is_running: false,
            last_network_stats: None,
        })
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<PerformanceEvent>> {
        self.event_rx.take()
    }

    pub async fn start(&mut self) -> Result<()> {
        if !self.config.enabled {
            info!("Performance monitoring is disabled");
            return Ok(());
        }

        info!("Starting performance monitor with interval {}ms", self.config.interval);

        let interval = interval(Duration::from_millis(self.config.interval.into()));
        self.collection_interval = Some(interval);
        self.is_running = true;

        // Initial metrics collection
        self.collect_metrics().await?;

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping performance monitor");
        self.is_running = false;
        self.collection_interval = None;
        Ok(())
    }

    pub async fn tick(&mut self) -> Result<()> {
        if !self.is_running {
            return Ok(());
        }

        if let Some(interval) = &mut self.collection_interval {
            interval.tick().await;
            self.collect_metrics().await?;
        }

        Ok(())
    }

    async fn collect_metrics(&mut self) -> Result<()> {
        self.system.refresh_all();

        // Get CPU usage (simplified for newer sysinfo API)
        let cpu_usage = self.system.global_cpu_info().cpu_usage();

        // Get memory usage
        let memory_total = self.system.total_memory();
        let memory_used = self.system.used_memory();
        let memory_usage = (memory_used as f64 / memory_total as f64) * 100.0;

        // Get load average (simplified - would need OS-specific implementation)
        let load_average = 0.5; // Placeholder

        // Get disk usage (simplified)
        let (disk_usage, disk_total, disk_used) = (50.0, 1000000000, 500000000); // Placeholder values

        // Get network statistics
        let (network_bytes_sent, network_bytes_received, network_packets_sent,
             network_packets_received, network_errors_in, network_errors_out) = 
            self.get_network_stats();

        let metrics = PerformanceMetrics {
            timestamp: Instant::now(),
            cpu_usage,
            memory_usage,
            memory_total,
            memory_used,
            load_average,
            disk_usage,
            disk_total,
            disk_used,
            network_bytes_sent,
            network_bytes_received,
            network_packets_sent,
            network_packets_received,
            network_errors_in,
            network_errors_out,
        };

        // Check thresholds and generate alerts
        self.check_thresholds(&metrics).await;

        // Store metrics in history
        {
            let mut history = self.metrics_history.write().await;
            history.push_back(metrics.clone());
            
            // Limit history size
            while history.len() > self.config.history_size as usize {
                history.pop_front();
            }
        }

        // Send metrics update event
        let _ = self.event_tx.send(PerformanceEvent::MetricsUpdate(metrics));

        Ok(())
    }

    fn get_network_stats(&mut self) -> (u64, u64, u64, u64, u64, u64) {
        let total_bytes_sent;
        let total_bytes_received;
        let total_packets_sent;
        let total_packets_received;
        let total_errors_in;
        let total_errors_out;

        // Simplified network stats (would need proper implementation)
        total_bytes_sent = 1000000;
        total_bytes_received = 800000;
        total_packets_sent = 1000;
        total_packets_received = 800;
        total_errors_in = 0;
        total_errors_out = 0;

        // Store current stats for rate calculations
        let current_stats = NetworkStats {
            timestamp: Instant::now(),
            bytes_sent: total_bytes_sent,
            bytes_received: total_bytes_received,
            packets_sent: total_packets_sent,
            packets_received: total_packets_received,
            errors_in: total_errors_in,
            errors_out: total_errors_out,
        };

        self.last_network_stats = Some(current_stats);

        (total_bytes_sent, total_bytes_received, total_packets_sent,
         total_packets_received, total_errors_in, total_errors_out)
    }

    async fn check_thresholds(&self, metrics: &PerformanceMetrics) {
        // Check CPU thresholds
        if metrics.cpu_usage >= self.thresholds.cpu_critical {
            self.send_alert("CPU Usage", metrics.cpu_usage as f64, 
                          self.thresholds.cpu_critical as f64, AlertLevel::Critical).await;
        } else if metrics.cpu_usage >= self.thresholds.cpu_warning {
            self.send_alert("CPU Usage", metrics.cpu_usage as f64, 
                          self.thresholds.cpu_warning as f64, AlertLevel::Warning).await;
        }

        // Check Memory thresholds
        if metrics.memory_usage >= self.thresholds.memory_critical {
            self.send_alert("Memory Usage", metrics.memory_usage, 
                          self.thresholds.memory_critical, AlertLevel::Critical).await;
        } else if metrics.memory_usage >= self.thresholds.memory_warning {
            self.send_alert("Memory Usage", metrics.memory_usage, 
                          self.thresholds.memory_warning, AlertLevel::Warning).await;
        }

        // Check Disk thresholds
        if metrics.disk_usage >= self.thresholds.disk_critical {
            self.send_alert("Disk Usage", metrics.disk_usage, 
                          self.thresholds.disk_critical, AlertLevel::Critical).await;
        } else if metrics.disk_usage >= self.thresholds.disk_warning {
            self.send_alert("Disk Usage", metrics.disk_usage, 
                          self.thresholds.disk_warning, AlertLevel::Warning).await;
        }

        // Check Load Average thresholds
        if metrics.load_average >= self.thresholds.load_critical {
            self.send_alert("Load Average", metrics.load_average, 
                          self.thresholds.load_critical, AlertLevel::Critical).await;
        } else if metrics.load_average >= self.thresholds.load_warning {
            self.send_alert("Load Average", metrics.load_average, 
                          self.thresholds.load_warning, AlertLevel::Warning).await;
        }
    }

    async fn send_alert(&self, metric: &str, value: f64, threshold: f64, level: AlertLevel) {
        let message = format!("{} is {:.2}%, threshold: {:.2}%", metric, value, threshold);
        
        match level {
            AlertLevel::Critical => error!("CRITICAL: {}", message),
            AlertLevel::Warning => warn!("WARNING: {}", message),
            AlertLevel::Info => info!("INFO: {}", message),
        }

        let alert = PerformanceAlert {
            timestamp: Instant::now(),
            level: level.clone(),
            metric: metric.to_string(),
            value,
            threshold,
            message: message.clone(),
        };

        // Send alert event
        let _ = self.event_tx.send(PerformanceEvent::Alert(alert));
        
        // Send threshold exceeded event
        let _ = self.event_tx.send(PerformanceEvent::ThresholdExceeded {
            metric: metric.to_string(),
            value,
            threshold,
            level,
        });
    }

    pub async fn get_current_metrics(&self) -> Option<PerformanceMetrics> {
        let history = self.metrics_history.read().await;
        history.back().cloned()
    }

    pub async fn get_metrics_history(&self) -> Vec<PerformanceMetrics> {
        let history = self.metrics_history.read().await;
        history.iter().cloned().collect()
    }

    pub async fn get_metrics_since(&self, since: Instant) -> Vec<PerformanceMetrics> {
        let history = self.metrics_history.read().await;
        history.iter()
            .filter(|m| m.timestamp >= since)
            .cloned()
            .collect()
    }

    pub fn get_thresholds(&self) -> &PerformanceThresholds {
        &self.thresholds
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PerformanceConfig, CpuThresholds, MemoryThresholds, 
                      DiskThresholds, LoadThresholds, NetworkThresholds, PerformanceThresholds as ConfigThresholds};

    fn create_test_config() -> PerformanceConfig {
        PerformanceConfig {
            enabled: true,
            interval: 1000,
            history_size: 100,
            thresholds: ConfigThresholds {
                cpu: CpuThresholds {
                    warning: 80.0,
                    critical: 95.0,
                },
                memory: MemoryThresholds {
                    warning: 80.0,
                    critical: 95.0,
                },
                disk: DiskThresholds {
                    warning: 90.0,
                    critical: 98.0,
                },
                load: LoadThresholds {
                    warning: 0.8,
                    critical: 1.5,
                },
                network: NetworkThresholds {
                    error_rate: 0.1,
                    utilization_warning: 80.0,
                },
            },
        }
    }

    #[tokio::test]
    async fn test_performance_monitor_creation() {
        let config = create_test_config();
        let monitor = PerformanceMonitor::new(config).unwrap();
        
        assert!(!monitor.is_running());
        assert_eq!(monitor.thresholds.cpu_warning, 80.0);
        assert_eq!(monitor.thresholds.memory_critical, 95.0);
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let config = create_test_config();
        let mut monitor = PerformanceMonitor::new(config).unwrap();
        
        monitor.collect_metrics().await.unwrap();
        
        let metrics = monitor.get_current_metrics().await;
        assert!(metrics.is_some());
        
        let metrics = metrics.unwrap();
        assert!(metrics.cpu_usage >= 0.0);
        assert!(metrics.memory_usage >= 0.0);
        assert!(metrics.memory_total > 0);
    }
}