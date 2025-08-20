//! Testing services for gateway diagnostics (Loopback and BERT)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tracing::info;

use crate::{Error, Result};

/// Loopback test types
#[derive(Debug, Clone, PartialEq)]
pub enum LoopbackType {
    Local,     // Internal loopback within the gateway
    Remote,    // Remote loopback request to far end
    Line,      // Line loopback (physical layer)
}

/// BERT test patterns
#[derive(Debug, Clone, PartialEq)]
pub enum BertPattern {
    Prbs15,    // Pseudo-Random Binary Sequence 2^15-1
    Prbs23,    // Pseudo-Random Binary Sequence 2^23-1  
    Prbs31,    // Pseudo-Random Binary Sequence 2^31-1
    AllZeros,  // All zeros pattern
    AllOnes,   // All ones pattern
    Alternating, // Alternating 0101... pattern
    Qrss,      // Quasi-Random Signal Source
}

/// Test status
#[derive(Debug, Clone, PartialEq)]
pub enum TestStatus {
    Idle,
    Running,
    Completed,
    Failed,
    Stopped,
}

/// Loopback test configuration
#[derive(Debug, Clone)]
pub struct LoopbackConfig {
    pub channel: u16,
    pub loopback_type: LoopbackType,
    pub timeout: Duration,
    pub test_duration: Option<Duration>,
}

/// Loopback test results
#[derive(Debug, Clone)]
pub struct LoopbackResult {
    pub channel: u16,
    pub loopback_type: LoopbackType,
    pub status: TestStatus,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub duration: Duration,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub packets_lost: u64,
    pub round_trip_times: Vec<Duration>,
    pub min_rtt: Duration,
    pub max_rtt: Duration,
    pub avg_rtt: Duration,
    pub jitter: Duration,
    pub success: bool,
    pub error_message: Option<String>,
}

/// BERT test configuration
#[derive(Debug, Clone)]
pub struct BertConfig {
    pub channel: u16,
    pub pattern: BertPattern,
    pub duration: Duration,
    pub bit_rate: u32, // bits per second
    pub error_threshold: f64, // error rate threshold (0.0-1.0)
}

/// BERT test results
#[derive(Debug, Clone)]
pub struct BertResult {
    pub channel: u16,
    pub pattern: BertPattern,
    pub status: TestStatus,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub duration: Duration,
    pub bit_rate: u32,
    pub bits_transmitted: u64,
    pub bits_received: u64,
    pub error_bits: u64,
    pub error_seconds: u32,
    pub severely_error_seconds: u32,
    pub unavailable_seconds: u32,
    pub bit_error_rate: f64,
    pub error_free_seconds: u32,
    pub sync_time: Duration,
    pub loss_of_sync_count: u32,
    pub pattern_sync: bool,
    pub signal_level_db: f64,
    pub jitter_us: f64,
    pub success: bool,
    pub error_message: Option<String>,
}

/// Test events
#[derive(Debug, Clone)]
pub enum TestEvent {
    LoopbackStarted { channel: u16, config: LoopbackConfig },
    LoopbackCompleted { channel: u16, result: LoopbackResult },
    LoopbackFailed { channel: u16, error: String },
    BertStarted { channel: u16, config: BertConfig },
    BertProgress { channel: u16, progress: f64, intermediate_result: BertResult },
    BertCompleted { channel: u16, result: BertResult },
    BertFailed { channel: u16, error: String },
    TestStopped { channel: u16, test_type: String },
}

/// Testing service configuration
#[derive(Debug, Clone)]
pub struct TestingConfig {
    pub loopback_enabled: bool,
    pub loopback_timeout: Duration,
    pub loopback_max_concurrent: u16,
    pub bert_enabled: bool,
    pub bert_patterns: Vec<BertPattern>,
    pub bert_default_duration: Duration,
    pub bert_error_threshold: f64,
    pub bert_max_concurrent: u16,
}

impl Default for TestingConfig {
    fn default() -> Self {
        Self {
            loopback_enabled: true,
            loopback_timeout: Duration::from_secs(10),
            loopback_max_concurrent: 10,
            bert_enabled: true,
            bert_patterns: vec![BertPattern::Prbs15, BertPattern::Prbs23],
            bert_default_duration: Duration::from_secs(60),
            bert_error_threshold: 0.001,
            bert_max_concurrent: 5,
        }
    }
}

/// Testing service
pub struct TestingService {
    config: TestingConfig,
    active_loopback_tests: Arc<RwLock<HashMap<u16, LoopbackResult>>>,
    active_bert_tests: Arc<RwLock<HashMap<u16, BertResult>>>,
    completed_loopback_tests: Arc<RwLock<HashMap<String, LoopbackResult>>>,
    completed_bert_tests: Arc<RwLock<HashMap<String, BertResult>>>,
    event_tx: mpsc::UnboundedSender<TestEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<TestEvent>>,
}

impl TestingService {
    pub fn new(config: TestingConfig) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        Self {
            config,
            active_loopback_tests: Arc::new(RwLock::new(HashMap::new())),
            active_bert_tests: Arc::new(RwLock::new(HashMap::new())),
            completed_loopback_tests: Arc::new(RwLock::new(HashMap::new())),
            completed_bert_tests: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            event_rx: Some(event_rx),
        }
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<TestEvent>> {
        self.event_rx.take()
    }

    /// Start a loopback test
    pub async fn start_loopback_test(&self, config: LoopbackConfig) -> Result<()> {
        if !self.config.loopback_enabled {
            return Err(Error::not_supported("Loopback testing is disabled"));
        }

        // Check if channel is already being tested
        {
            let active_tests = self.active_loopback_tests.read().await;
            if active_tests.contains_key(&config.channel) {
                return Err(Error::invalid_state(format!(
                    "Loopback test already running on channel {}", config.channel
                )));
            }

            // Check concurrent test limit
            if active_tests.len() >= self.config.loopback_max_concurrent as usize {
                return Err(Error::internal("Maximum concurrent loopback tests reached"));
            }
        }

        info!("Starting loopback test on channel {} with type {:?}", 
              config.channel, config.loopback_type);

        // Initialize test result
        let result = LoopbackResult {
            channel: config.channel,
            loopback_type: config.loopback_type.clone(),
            status: TestStatus::Running,
            start_time: Instant::now(),
            end_time: None,
            duration: Duration::from_secs(0),
            packets_sent: 0,
            packets_received: 0,
            packets_lost: 0,
            round_trip_times: Vec::new(),
            min_rtt: Duration::from_millis(u64::MAX),
            max_rtt: Duration::from_secs(0),
            avg_rtt: Duration::from_secs(0),
            jitter: Duration::from_secs(0),
            success: false,
            error_message: None,
        };

        // Add to active tests
        {
            let mut active_tests = self.active_loopback_tests.write().await;
            active_tests.insert(config.channel, result.clone());
        }

        // Send start event
        let _ = self.event_tx.send(TestEvent::LoopbackStarted {
            channel: config.channel,
            config: config.clone(),
        });

        // Spawn test task
        let testing_service = self.clone();
        let test_config = config.clone();
        let channel = config.channel;
        tokio::spawn(async move {
            let test_result = testing_service.run_loopback_test(test_config).await;
            
            match test_result {
                Ok(result) => {
                    let _ = testing_service.event_tx.send(TestEvent::LoopbackCompleted {
                        channel: result.channel,
                        result: result.clone(),
                    });

                    // Move to completed tests
                    let test_id = format!("loopback-{}-{}", result.channel, 
                                        result.start_time.elapsed().as_secs());
                    {
                        let mut completed = testing_service.completed_loopback_tests.write().await;
                        completed.insert(test_id, result.clone());
                        
                        // Limit completed test history
                        if completed.len() > 1000 {
                            let oldest_key = completed.keys().next().unwrap().clone();
                            completed.remove(&oldest_key);
                        }
                    }
                },
                Err(e) => {
                    let _ = testing_service.event_tx.send(TestEvent::LoopbackFailed {
                        channel,
                        error: e.to_string(),
                    });
                }
            }

            // Remove from active tests
            {
                let mut active_tests = testing_service.active_loopback_tests.write().await;
                active_tests.remove(&channel);
            }
        });

        Ok(())
    }

    /// Stop a loopback test
    pub async fn stop_loopback_test(&self, channel: u16) -> Result<()> {
        {
            let mut active_tests = self.active_loopback_tests.write().await;
            if let Some(mut result) = active_tests.remove(&channel) {
                result.status = TestStatus::Stopped;
                result.end_time = Some(Instant::now());
                result.duration = result.start_time.elapsed();
                
                info!("Stopped loopback test on channel {}", channel);
                
                let _ = self.event_tx.send(TestEvent::TestStopped {
                    channel,
                    test_type: "loopback".to_string(),
                });
            } else {
                return Err(Error::invalid_state(format!(
                    "No active loopback test on channel {}", channel
                )));
            }
        }

        Ok(())
    }

    /// Start a BERT test
    pub async fn start_bert_test(&self, config: BertConfig) -> Result<()> {
        if !self.config.bert_enabled {
            return Err(Error::not_supported("BERT testing is disabled"));
        }

        // Check if channel is already being tested
        {
            let active_tests = self.active_bert_tests.read().await;
            if active_tests.contains_key(&config.channel) {
                return Err(Error::invalid_state(format!(
                    "BERT test already running on channel {}", config.channel
                )));
            }

            // Check concurrent test limit
            if active_tests.len() >= self.config.bert_max_concurrent as usize {
                return Err(Error::internal("Maximum concurrent BERT tests reached"));
            }
        }

        info!("Starting BERT test on channel {} with pattern {:?} for {:?}", 
              config.channel, config.pattern, config.duration);

        // Initialize test result
        let result = BertResult {
            channel: config.channel,
            pattern: config.pattern.clone(),
            status: TestStatus::Running,
            start_time: Instant::now(),
            end_time: None,
            duration: Duration::from_secs(0),
            bit_rate: config.bit_rate,
            bits_transmitted: 0,
            bits_received: 0,
            error_bits: 0,
            error_seconds: 0,
            severely_error_seconds: 0,
            unavailable_seconds: 0,
            bit_error_rate: 0.0,
            error_free_seconds: 0,
            sync_time: Duration::from_millis(150), // Typical sync time
            loss_of_sync_count: 0,
            pattern_sync: true,
            signal_level_db: -12.3, // Typical signal level
            jitter_us: 2.1, // Typical jitter
            success: false,
            error_message: None,
        };

        // Add to active tests
        {
            let mut active_tests = self.active_bert_tests.write().await;
            active_tests.insert(config.channel, result.clone());
        }

        // Send start event
        let _ = self.event_tx.send(TestEvent::BertStarted {
            channel: config.channel,
            config: config.clone(),
        });

        // Spawn test task
        let testing_service = self.clone();
        let test_config = config.clone();
        let channel = config.channel;
        tokio::spawn(async move {
            let test_result = testing_service.run_bert_test(test_config).await;
            
            match test_result {
                Ok(result) => {
                    let _ = testing_service.event_tx.send(TestEvent::BertCompleted {
                        channel: result.channel,
                        result: result.clone(),
                    });

                    // Move to completed tests
                    let test_id = format!("bert-{}-{}", result.channel, 
                                        result.start_time.elapsed().as_secs());
                    {
                        let mut completed = testing_service.completed_bert_tests.write().await;
                        completed.insert(test_id, result.clone());
                        
                        // Limit completed test history
                        if completed.len() > 1000 {
                            let oldest_key = completed.keys().next().unwrap().clone();
                            completed.remove(&oldest_key);
                        }
                    }
                },
                Err(e) => {
                    let _ = testing_service.event_tx.send(TestEvent::BertFailed {
                        channel,
                        error: e.to_string(),
                    });
                }
            }

            // Remove from active tests
            {
                let mut active_tests = testing_service.active_bert_tests.write().await;
                active_tests.remove(&channel);
            }
        });

        Ok(())
    }

    /// Stop a BERT test
    pub async fn stop_bert_test(&self, channel: u16) -> Result<()> {
        {
            let mut active_tests = self.active_bert_tests.write().await;
            if let Some(mut result) = active_tests.remove(&channel) {
                result.status = TestStatus::Stopped;
                result.end_time = Some(Instant::now());
                result.duration = result.start_time.elapsed();
                
                info!("Stopped BERT test on channel {}", channel);
                
                let _ = self.event_tx.send(TestEvent::TestStopped {
                    channel,
                    test_type: "bert".to_string(),
                });
            } else {
                return Err(Error::invalid_state(format!(
                    "No active BERT test on channel {}", channel
                )));
            }
        }

        Ok(())
    }

    /// Get active loopback tests
    pub async fn get_active_loopback_tests(&self) -> HashMap<u16, LoopbackResult> {
        let active_tests = self.active_loopback_tests.read().await;
        active_tests.clone()
    }

    /// Get active BERT tests
    pub async fn get_active_bert_tests(&self) -> HashMap<u16, BertResult> {
        let active_tests = self.active_bert_tests.read().await;
        active_tests.clone()
    }

    /// Get completed loopback test results
    pub async fn get_loopback_results(&self, test_id: &str) -> Option<LoopbackResult> {
        let completed = self.completed_loopback_tests.read().await;
        completed.get(test_id).cloned()
    }

    /// Get completed BERT test results
    pub async fn get_bert_results(&self, test_id: &str) -> Option<BertResult> {
        let completed = self.completed_bert_tests.read().await;
        completed.get(test_id).cloned()
    }

    /// Get BERT results for a channel (latest)
    pub async fn get_bert_results_for_channel(&self, channel: u16) -> Option<BertResult> {
        let completed = self.completed_bert_tests.read().await;
        completed.values()
            .filter(|result| result.channel == channel)
            .max_by_key(|result| result.start_time)
            .cloned()
    }

    async fn run_loopback_test(&self, config: LoopbackConfig) -> Result<LoopbackResult> {
        let mut result = LoopbackResult {
            channel: config.channel,
            loopback_type: config.loopback_type.clone(),
            status: TestStatus::Running,
            start_time: Instant::now(),
            end_time: None,
            duration: Duration::from_secs(0),
            packets_sent: 0,
            packets_received: 0,
            packets_lost: 0,
            round_trip_times: Vec::new(),
            min_rtt: Duration::from_millis(u64::MAX),
            max_rtt: Duration::from_secs(0),
            avg_rtt: Duration::from_secs(0),
            jitter: Duration::from_secs(0),
            success: false,
            error_message: None,
        };

        // Simulate loopback test with realistic behavior
        let test_duration = config.test_duration.unwrap_or(Duration::from_secs(5));
        let packet_interval = Duration::from_millis(100);
        let mut interval = interval(packet_interval);
        
        let mut rng = StdRng::from_entropy();
        
        let start_time = Instant::now();
        while start_time.elapsed() < test_duration {
            interval.tick().await;
            
            result.packets_sent += 1;
            
            // Simulate packet loss (1% for local, 5% for remote)
            let loss_rate = match config.loopback_type {
                LoopbackType::Local => 0.01,
                LoopbackType::Remote => 0.05,
                LoopbackType::Line => 0.02,
            };
            
            if rng.gen::<f64>() > loss_rate {
                result.packets_received += 1;
                
                // Simulate RTT variation
                let base_rtt = match config.loopback_type {
                    LoopbackType::Local => Duration::from_micros(50),
                    LoopbackType::Remote => Duration::from_millis(20),
                    LoopbackType::Line => Duration::from_micros(100),
                };
                
                let jitter_ms = rng.gen_range(-5i64..=5i64);
                let rtt = base_rtt + Duration::from_millis(jitter_ms.max(0) as u64);
                
                result.round_trip_times.push(rtt);
                
                if rtt < result.min_rtt {
                    result.min_rtt = rtt;
                }
                if rtt > result.max_rtt {
                    result.max_rtt = rtt;
                }
            }
        }

        result.packets_lost = result.packets_sent - result.packets_received;
        result.end_time = Some(Instant::now());
        result.duration = result.start_time.elapsed();

        // Calculate statistics
        if !result.round_trip_times.is_empty() {
            let total_rtt: Duration = result.round_trip_times.iter().sum();
            result.avg_rtt = total_rtt / result.round_trip_times.len() as u32;
            
            // Calculate jitter (standard deviation of RTT)
            let avg_rtt_us = result.avg_rtt.as_micros() as f64;
            let variance: f64 = result.round_trip_times.iter()
                .map(|rtt| {
                    let diff = rtt.as_micros() as f64 - avg_rtt_us;
                    diff * diff
                })
                .sum::<f64>() / result.round_trip_times.len() as f64;
            
            result.jitter = Duration::from_micros(variance.sqrt() as u64);
        }

        // Determine success
        let packet_loss_rate = result.packets_lost as f64 / result.packets_sent as f64;
        result.success = packet_loss_rate < 0.1; // Success if less than 10% packet loss
        
        if !result.success {
            result.error_message = Some(format!(
                "High packet loss: {:.1}%", packet_loss_rate * 100.0
            ));
        }

        result.status = if result.success { TestStatus::Completed } else { TestStatus::Failed };

        // Update active test
        {
            let mut active_tests = self.active_loopback_tests.write().await;
            if let Some(active_result) = active_tests.get_mut(&config.channel) {
                *active_result = result.clone();
            }
        }

        Ok(result)
    }

    async fn run_bert_test(&self, config: BertConfig) -> Result<BertResult> {
        let mut result = BertResult {
            channel: config.channel,
            pattern: config.pattern.clone(),
            status: TestStatus::Running,
            start_time: Instant::now(),
            end_time: None,
            duration: Duration::from_secs(0),
            bit_rate: config.bit_rate,
            bits_transmitted: 0,
            bits_received: 0,
            error_bits: 0,
            error_seconds: 0,
            severely_error_seconds: 0,
            unavailable_seconds: 0,
            bit_error_rate: 0.0,
            error_free_seconds: 0,
            sync_time: Duration::from_millis(150),
            loss_of_sync_count: 0,
            pattern_sync: true,
            signal_level_db: -12.3,
            jitter_us: 2.1,
            success: false,
            error_message: None,
        };

        let mut rng = StdRng::from_entropy();
        let update_interval = Duration::from_secs(1);
        let mut interval = interval(update_interval);
        
        let bits_per_second = config.bit_rate as f64;
        let bits_per_update = (bits_per_second * update_interval.as_secs_f64()) as u64;
        
        let start_time = Instant::now();
        while start_time.elapsed() < config.duration {
            interval.tick().await;
            
            result.bits_transmitted += bits_per_update;
            result.bits_received += bits_per_update;
            
            // Simulate bit errors based on pattern complexity
            let base_error_rate = match config.pattern {
                BertPattern::AllZeros | BertPattern::AllOnes => 0.000001, // Very low error rate
                BertPattern::Alternating => 0.00001,
                BertPattern::Prbs15 => 0.00005,
                BertPattern::Prbs23 => 0.0001,
                BertPattern::Prbs31 => 0.0002,
                BertPattern::Qrss => 0.0001,
            };
            
            // Add some randomness to error rate
            let actual_error_rate = base_error_rate * rng.gen_range(0.5..2.0);
            let error_bits_this_second = (bits_per_update as f64 * actual_error_rate) as u64;
            
            result.error_bits += error_bits_this_second;
            
            // Track error seconds
            if error_bits_this_second > 0 {
                result.error_seconds += 1;
                
                // Severely errored second threshold: BER > 10^-3
                if error_bits_this_second as f64 / bits_per_update as f64 > 0.001 {
                    result.severely_error_seconds += 1;
                }
            } else {
                result.error_free_seconds += 1;
            }
            
            // Calculate current BER
            result.bit_error_rate = result.error_bits as f64 / result.bits_transmitted as f64;
            result.duration = result.start_time.elapsed();
            
            // Send progress update
            let progress = result.duration.as_secs_f64() / config.duration.as_secs_f64();
            let _ = self.event_tx.send(TestEvent::BertProgress {
                channel: config.channel,
                progress,
                intermediate_result: result.clone(),
            });
            
            // Update active test
            {
                let mut active_tests = self.active_bert_tests.write().await;
                if let Some(active_result) = active_tests.get_mut(&config.channel) {
                    *active_result = result.clone();
                }
            }
        }

        result.end_time = Some(Instant::now());
        result.duration = result.start_time.elapsed();
        
        // Determine success based on error threshold
        result.success = result.bit_error_rate <= config.error_threshold;
        result.status = if result.success { TestStatus::Completed } else { TestStatus::Failed };
        
        if !result.success {
            result.error_message = Some(format!(
                "BER {:.2e} exceeds threshold {:.2e}", 
                result.bit_error_rate, config.error_threshold
            ));
        }

        Ok(result)
    }
}

// Implement Clone for TestingService to allow spawning tasks
impl Clone for TestingService {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            active_loopback_tests: Arc::clone(&self.active_loopback_tests),
            active_bert_tests: Arc::clone(&self.active_bert_tests),
            completed_loopback_tests: Arc::clone(&self.completed_loopback_tests),
            completed_bert_tests: Arc::clone(&self.completed_bert_tests),
            event_tx: self.event_tx.clone(),
            event_rx: None, // Don't clone the receiver
        }
    }
}

impl ToString for LoopbackType {
    fn to_string(&self) -> String {
        match self {
            LoopbackType::Local => "local".to_string(),
            LoopbackType::Remote => "remote".to_string(),
            LoopbackType::Line => "line".to_string(),
        }
    }
}

impl ToString for BertPattern {
    fn to_string(&self) -> String {
        match self {
            BertPattern::Prbs15 => "prbs_15".to_string(),
            BertPattern::Prbs23 => "prbs_23".to_string(),
            BertPattern::Prbs31 => "prbs_31".to_string(),
            BertPattern::AllZeros => "all_zeros".to_string(),
            BertPattern::AllOnes => "all_ones".to_string(),
            BertPattern::Alternating => "alternating".to_string(),
            BertPattern::Qrss => "qrss".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_testing_service_creation() {
        let config = TestingConfig::default();
        let service = TestingService::new(config);
        
        assert!(service.config.loopback_enabled);
        assert!(service.config.bert_enabled);
    }

    #[tokio::test] 
    async fn test_loopback_test() {
        let config = TestingConfig::default();
        let service = TestingService::new(config);
        
        let loopback_config = LoopbackConfig {
            channel: 1,
            loopback_type: LoopbackType::Local,
            timeout: Duration::from_secs(5),
            test_duration: Some(Duration::from_millis(500)),
        };

        service.start_loopback_test(loopback_config).await.unwrap();
        
        // Wait for test to start
        sleep(Duration::from_millis(100)).await;
        
        let active_tests = service.get_active_loopback_tests().await;
        assert!(active_tests.contains_key(&1));
    }

    #[tokio::test]
    async fn test_bert_test() {
        let config = TestingConfig::default();
        let service = TestingService::new(config);
        
        let bert_config = BertConfig {
            channel: 1,
            pattern: BertPattern::Prbs23,
            duration: Duration::from_millis(500),
            bit_rate: 64000,
            error_threshold: 0.001,
        };

        service.start_bert_test(bert_config).await.unwrap();
        
        // Wait for test to start
        sleep(Duration::from_millis(100)).await;
        
        let active_tests = service.get_active_bert_tests().await;
        assert!(active_tests.contains_key(&1));
    }
}