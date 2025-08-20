//! Interface testing service for TDMoE loopback and cross-port wiring tests

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, sleep};
use tracing::{debug, info};
use uuid::Uuid;

use crate::{Error, Result};

/// Test types for interface testing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InterfaceTestType {
    /// TDMoE loopback - sends data that should come back to itself
    TdmoeLoopback,
    /// Cross-port wiring - sends data from one port to another
    CrossPortWiring,
    /// End-to-end call test through the gateway
    EndToEndCall,
    /// Timing and synchronization test
    TimingSyncTest,
    /// Protocol stack validation
    ProtocolStackTest,
}

/// Test pattern types for generating test data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TestPattern {
    /// Pseudo-random binary sequence
    Prbs15,
    Prbs23,
    Prbs31,
    /// Fixed patterns
    AllZeros,
    AllOnes,
    Alternating,
    /// Custom sequence
    Custom(Vec<u8>),
    /// Voice simulation patterns
    ToneGeneration(f64), // Frequency in Hz
    /// Signaling patterns
    Q931Setup,
    Q931Release,
    LapdFrames,
}

/// Interface configuration for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceTestConfig {
    pub test_id: Uuid,
    pub test_type: InterfaceTestType,
    pub source_span: u32,
    pub source_channel: Option<u8>, // None for all channels
    pub dest_span: Option<u32>,     // None for loopback
    pub dest_channel: Option<u8>,
    pub pattern: TestPattern,
    pub duration: Duration,
    pub data_rate: u64, // bits per second
    pub frame_size: usize,
    pub expected_delay: Duration,
    pub tolerance: Duration,
    pub success_threshold: f64, // percentage of successful frames
}

/// Real-time test statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceTestStats {
    pub test_id: Uuid,
    pub start_time: DateTime<Utc>,
    pub elapsed_time: Duration,
    pub frames_sent: u64,
    pub frames_received: u64,
    pub frames_lost: u64,
    pub frames_corrupted: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub min_delay: Duration,
    pub max_delay: Duration,
    pub avg_delay: Duration,
    pub jitter: Duration,
    pub bit_error_rate: f64,
    pub frame_error_rate: f64,
    pub current_throughput: f64, // Mbps
    pub timing_errors: u64,
    pub sync_losses: u64,
}

/// Test result with detailed analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceTestResult {
    pub config: InterfaceTestConfig,
    pub stats: InterfaceTestStats,
    pub success: bool,
    pub completion_time: DateTime<Utc>,
    pub error_analysis: Vec<String>,
    pub recommendations: Vec<String>,
    pub raw_measurements: Vec<FrameMeasurement>,
}

/// Individual frame measurement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameMeasurement {
    pub sequence_number: u64,
    pub send_time: DateTime<Utc>,
    pub receive_time: Option<DateTime<Utc>>,
    pub round_trip_delay: Option<Duration>,
    pub corrupted: bool,
    pub error_bits: u32,
    pub signal_quality: f64,
}

/// Test events for monitoring
#[derive(Debug, Clone)]
pub enum InterfaceTestEvent {
    TestStarted {
        test_id: Uuid,
        config: InterfaceTestConfig,
    },
    TestProgress {
        test_id: Uuid,
        progress: f64,
        stats: InterfaceTestStats,
    },
    TestCompleted {
        test_id: Uuid,
        result: InterfaceTestResult,
    },
    TestFailed {
        test_id: Uuid,
        error: String,
    },
    FrameReceived {
        test_id: Uuid,
        measurement: FrameMeasurement,
    },
    SyncLost {
        test_id: Uuid,
        span: u32,
    },
    SyncRestored {
        test_id: Uuid,
        span: u32,
    },
}

/// Active test state
#[derive(Debug)]
#[allow(dead_code)]
struct ActiveTest {
    config: InterfaceTestConfig,
    stats: Arc<RwLock<InterfaceTestStats>>,
    measurements: Arc<RwLock<Vec<FrameMeasurement>>>,
    start_time: Instant,
    frame_sender: mpsc::UnboundedSender<Bytes>,
    cancel_tx: mpsc::UnboundedSender<()>,
}

/// Interface testing service
pub struct InterfaceTestingService {
    active_tests: Arc<RwLock<HashMap<Uuid, ActiveTest>>>,
    completed_tests: Arc<RwLock<HashMap<Uuid, InterfaceTestResult>>>,
    event_tx: mpsc::UnboundedSender<InterfaceTestEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<InterfaceTestEvent>>,
    rng: Arc<RwLock<StdRng>>,
}

impl InterfaceTestingService {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        Self {
            active_tests: Arc::new(RwLock::new(HashMap::new())),
            completed_tests: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            event_rx: Some(event_rx),
            rng: Arc::new(RwLock::new(StdRng::from_entropy())),
        }
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<InterfaceTestEvent>> {
        self.event_rx.take()
    }

    /// Start a TDMoE loopback test
    pub async fn start_tdmoe_loopback_test(
        &self,
        span: u32,
        channels: Option<Vec<u8>>,
        pattern: TestPattern,
        duration: Duration,
    ) -> Result<Uuid> {
        let test_id = Uuid::new_v4();
        
        let config = InterfaceTestConfig {
            test_id,
            test_type: InterfaceTestType::TdmoeLoopback,
            source_span: span,
            source_channel: None, // Will test all specified channels
            dest_span: None,      // Loopback to same span
            dest_channel: None,
            pattern,
            duration,
            data_rate: 2_048_000, // E1 rate
            frame_size: 32,       // Standard TDM frame
            expected_delay: Duration::from_micros(125), // Frame time for E1
            tolerance: Duration::from_micros(50),
            success_threshold: 99.9, // High threshold for loopback
        };

        info!("Starting TDMoE loopback test for span {} (test_id: {})", span, test_id);
        self.start_test(config, channels).await
    }

    /// Start a cross-port wiring test
    pub async fn start_cross_port_test(
        &self,
        source_span: u32,
        dest_span: u32,
        _channel_mapping: Option<HashMap<u8, u8>>, // source -> dest channel mapping
        pattern: TestPattern,
        duration: Duration,
    ) -> Result<Uuid> {
        let test_id = Uuid::new_v4();
        
        let config = InterfaceTestConfig {
            test_id,
            test_type: InterfaceTestType::CrossPortWiring,
            source_span,
            source_channel: None,
            dest_span: Some(dest_span),
            dest_channel: None,
            pattern,
            duration,
            data_rate: 2_048_000,
            frame_size: 32,
            expected_delay: Duration::from_millis(1), // Network delay
            tolerance: Duration::from_millis(5),
            success_threshold: 99.0, // Slightly lower for cross-port
        };

        info!("Starting cross-port wiring test: span {} -> span {} (test_id: {})", 
              source_span, dest_span, test_id);
        self.start_test(config, None).await
    }

    /// Start an end-to-end call test
    pub async fn start_end_to_end_test(
        &self,
        calling_span: u32,
        called_span: u32,
        duration: Duration,
    ) -> Result<Uuid> {
        let test_id = Uuid::new_v4();
        
        let config = InterfaceTestConfig {
            test_id,
            test_type: InterfaceTestType::EndToEndCall,
            source_span: calling_span,
            source_channel: Some(1), // Use B-channel 1
            dest_span: Some(called_span),
            dest_channel: Some(1),
            pattern: TestPattern::ToneGeneration(1000.0), // 1kHz tone
            duration,
            data_rate: 64_000,  // Single B-channel
            frame_size: 8,      // Smaller frames for voice
            expected_delay: Duration::from_millis(20), // Voice delay
            tolerance: Duration::from_millis(100),
            success_threshold: 95.0, // Voice quality threshold
        };

        info!("Starting end-to-end call test: span {} -> span {} (test_id: {})", 
              calling_span, called_span, test_id);
        self.start_test(config, Some(vec![1])).await
    }

    /// Generic test starter
    async fn start_test(
        &self,
        config: InterfaceTestConfig,
        channels: Option<Vec<u8>>,
    ) -> Result<Uuid> {
        let test_id = config.test_id;

        // Check if test already exists
        {
            let active_tests = self.active_tests.read().await;
            if active_tests.contains_key(&test_id) {
                return Err(Error::invalid_state(format!("Test {} already running", test_id)));
            }
        }

        // Initialize test statistics
        let stats = Arc::new(RwLock::new(InterfaceTestStats {
            test_id,
            start_time: Utc::now(),
            elapsed_time: Duration::from_secs(0),
            frames_sent: 0,
            frames_received: 0,
            frames_lost: 0,
            frames_corrupted: 0,
            bytes_sent: 0,
            bytes_received: 0,
            min_delay: Duration::from_secs(u64::MAX),
            max_delay: Duration::from_secs(0),
            avg_delay: Duration::from_secs(0),
            jitter: Duration::from_secs(0),
            bit_error_rate: 0.0,
            frame_error_rate: 0.0,
            current_throughput: 0.0,
            timing_errors: 0,
            sync_losses: 0,
        }));

        let measurements = Arc::new(RwLock::new(Vec::new()));
        let (frame_sender, frame_receiver) = mpsc::unbounded_channel();
        let (cancel_tx, cancel_rx) = mpsc::unbounded_channel();

        // Create active test entry
        let active_test = ActiveTest {
            config: config.clone(),
            stats: Arc::clone(&stats),
            measurements: Arc::clone(&measurements),
            start_time: Instant::now(),
            frame_sender,
            cancel_tx,
        };

        // Add to active tests
        {
            let mut active_tests = self.active_tests.write().await;
            active_tests.insert(test_id, active_test);
        }

        // Send start event
        let _ = self.event_tx.send(InterfaceTestEvent::TestStarted {
            test_id,
            config: config.clone(),
        });

        // Spawn test execution task
        let service = self.clone();
        tokio::spawn(async move {
            let result = service.execute_test(
                config,
                channels,
                frame_receiver,
                cancel_rx,
                stats,
                measurements,
            ).await;

            match result {
                Ok(test_result) => {
                    let _ = service.event_tx.send(InterfaceTestEvent::TestCompleted {
                        test_id,
                        result: test_result.clone(),
                    });

                    // Move to completed tests
                    {
                        let mut completed = service.completed_tests.write().await;
                        completed.insert(test_id, test_result);
                        
                        // Limit history
                        if completed.len() > 100 {
                            let oldest_key = *completed.keys().next().unwrap();
                            completed.remove(&oldest_key);
                        }
                    }
                },
                Err(e) => {
                    let _ = service.event_tx.send(InterfaceTestEvent::TestFailed {
                        test_id,
                        error: e.to_string(),
                    });
                },
            }

            // Remove from active tests
            {
                let mut active_tests = service.active_tests.write().await;
                active_tests.remove(&test_id);
            }
        });

        Ok(test_id)
    }

    /// Execute the actual test
    async fn execute_test(
        &self,
        config: InterfaceTestConfig,
        _channels: Option<Vec<u8>>,
        frame_receiver: mpsc::UnboundedReceiver<Bytes>,
        mut cancel_rx: mpsc::UnboundedReceiver<()>,
        stats: Arc<RwLock<InterfaceTestStats>>,
        measurements: Arc<RwLock<Vec<FrameMeasurement>>>,
    ) -> Result<InterfaceTestResult> {
        let test_start = Instant::now();
        let frame_interval = Duration::from_nanos(
            (1_000_000_000 * config.frame_size as u64 * 8) / config.data_rate
        );

        // Spawn frame generation task
        let generator_stats = Arc::clone(&stats);
        let generator_config = config.clone();
        let generator_rng = Arc::clone(&self.rng);
        let generator_event_tx = self.event_tx.clone();
        
        tokio::spawn(async move {
            Self::generate_test_frames(
                generator_config,
                generator_rng,
                generator_stats,
                generator_event_tx,
                frame_interval,
            ).await;
        });

        // Spawn frame reception simulation task  
        let receiver_stats = Arc::clone(&stats);
        let receiver_measurements = Arc::clone(&measurements);
        let receiver_config = config.clone();
        let receiver_event_tx = self.event_tx.clone();
        
        tokio::spawn(async move {
            Self::simulate_frame_reception(
                receiver_config,
                receiver_stats,
                receiver_measurements,
                receiver_event_tx,
                frame_receiver,
            ).await;
        });

        // Main test monitoring loop
        let mut progress_interval = interval(Duration::from_secs(1));
        
        loop {
            tokio::select! {
                _ = progress_interval.tick() => {
                    let elapsed = test_start.elapsed();
                    
                    // Update elapsed time in stats
                    {
                        let mut stats_guard = stats.write().await;
                        stats_guard.elapsed_time = elapsed;
                    }
                    
                    // Send progress update
                    let progress = elapsed.as_secs_f64() / config.duration.as_secs_f64();
                    if progress <= 1.0 {
                        let current_stats = stats.read().await.clone();
                        let _ = self.event_tx.send(InterfaceTestEvent::TestProgress {
                            test_id: config.test_id,
                            progress,
                            stats: current_stats,
                        });
                    }
                },
                _ = cancel_rx.recv() => {
                    debug!("Test {} cancelled", config.test_id);
                    break;
                },
                _ = sleep(config.duration) => {
                    debug!("Test {} completed normally", config.test_id);
                    break;
                }
            }

            if test_start.elapsed() >= config.duration {
                break;
            }
        }

        // Generate final results
        self.generate_test_result(config, stats, measurements).await
    }

    /// Generate test frames according to the specified pattern
    async fn generate_test_frames(
        config: InterfaceTestConfig,
        rng: Arc<RwLock<StdRng>>,
        stats: Arc<RwLock<InterfaceTestStats>>,
        event_tx: mpsc::UnboundedSender<InterfaceTestEvent>,
        frame_interval: Duration,
    ) {
        let mut interval = interval(frame_interval);
        let start_time = Instant::now();
        let mut sequence_number = 0u64;

        while start_time.elapsed() < config.duration {
            interval.tick().await;
            
            let frame_data = Self::generate_frame_data(&config.pattern, config.frame_size, &rng).await;
            let send_time = Utc::now();
            
            // Update stats
            {
                let mut stats_guard = stats.write().await;
                stats_guard.frames_sent += 1;
                stats_guard.bytes_sent += frame_data.len() as u64;
            }

            // Simulate frame transmission (in real implementation, this would send to hardware)
            Self::simulate_frame_transmission(
                config.test_id,
                sequence_number,
                frame_data,
                send_time,
                &config,
                &event_tx,
            ).await;

            sequence_number += 1;
        }
    }

    /// Generate frame data according to pattern
    async fn generate_frame_data(
        pattern: &TestPattern,
        frame_size: usize,
        rng: &Arc<RwLock<StdRng>>,
    ) -> Bytes {
        match pattern {
            TestPattern::AllZeros => Bytes::from(vec![0u8; frame_size]),
            TestPattern::AllOnes => Bytes::from(vec![0xFFu8; frame_size]),
            TestPattern::Alternating => {
                let data: Vec<u8> = (0..frame_size).map(|i| if i % 2 == 0 { 0x55 } else { 0xAA }).collect();
                Bytes::from(data)
            },
            TestPattern::Prbs15 => {
                // Simple PRBS-15 implementation
                let mut data = Vec::with_capacity(frame_size);
                let mut rng_guard = rng.write().await;
                for _ in 0..frame_size {
                    data.push(rng_guard.gen());
                }
                Bytes::from(data)
            },
            TestPattern::Prbs23 | TestPattern::Prbs31 => {
                // Extended PRBS patterns
                let mut data = Vec::with_capacity(frame_size);
                let mut rng_guard = rng.write().await;
                for _ in 0..frame_size {
                    data.push(rng_guard.gen());
                }
                Bytes::from(data)
            },
            TestPattern::Custom(pattern) => {
                let mut data = Vec::with_capacity(frame_size);
                for i in 0..frame_size {
                    data.push(pattern[i % pattern.len()]);
                }
                Bytes::from(data)
            },
            TestPattern::ToneGeneration(freq) => {
                // Generate sine wave samples for voice simulation
                let sample_rate = 8000.0; // 8kHz for voice
                let samples_per_frame = frame_size / 2; // 16-bit samples
                let mut data = Vec::with_capacity(frame_size);
                
                for i in 0..samples_per_frame {
                    let t = i as f64 / sample_rate;
                    let sample = (freq * 2.0 * std::f64::consts::PI * t).sin() * 32767.0;
                    let sample_i16 = sample as i16;
                    data.extend_from_slice(&sample_i16.to_le_bytes());
                }
                
                Bytes::from(data)
            },
            TestPattern::Q931Setup => {
                // Generate Q.931 SETUP message
                let setup_msg = vec![
                    0x08, 0x01, 0x01, 0x02, 0x05, // Protocol discriminator, call reference, message type
                    0x04, 0x03, 0x80, 0x90, 0xA2, // Bearer capability
                    0x6C, 0x04, 0x21, 0x83, 0x00, 0x00, // Calling party number
                    0x70, 0x04, 0x21, 0x83, 0x00, 0x01, // Called party number
                ];
                let mut data = setup_msg;
                data.resize(frame_size, 0);
                Bytes::from(data)
            },
            TestPattern::Q931Release => {
                // Generate Q.931 RELEASE message
                let release_msg = vec![
                    0x08, 0x01, 0x01, 0x02, 0x4D, // Protocol discriminator, call reference, RELEASE
                    0x08, 0x02, 0x80, 0x10, // Cause IE
                ];
                let mut data = release_msg;
                data.resize(frame_size, 0);
                Bytes::from(data)
            },
            TestPattern::LapdFrames => {
                // Generate LAPD frame
                let lapd_frame = vec![
                    0x02, 0x01, // Address and control
                    0x01, 0x02, 0x03, 0x04, // Sample information field
                ];
                let mut data = lapd_frame;
                data.resize(frame_size, 0);
                Bytes::from(data)
            },
        }
    }

    /// Simulate frame transmission (in real system would interface with hardware)
    async fn simulate_frame_transmission(
        test_id: Uuid,
        sequence_number: u64,
        _frame_data: Bytes,
        send_time: DateTime<Utc>,
        config: &InterfaceTestConfig,
        event_tx: &mpsc::UnboundedSender<InterfaceTestEvent>,
    ) {
        // Simulate network/hardware delay
        let base_delay = config.expected_delay;
        let jitter = Duration::from_micros(rand::random::<u64>() % 100);
        let total_delay = base_delay + jitter;
        
        sleep(total_delay).await;
        
        // Simulate frame reception with some loss and corruption
        let loss_probability = match config.test_type {
            InterfaceTestType::TdmoeLoopback => 0.001,    // Very low loss for loopback
            InterfaceTestType::CrossPortWiring => 0.01,   // Higher loss for cross-port
            InterfaceTestType::EndToEndCall => 0.05,      // Realistic call loss
            _ => 0.01,
        };
        
        if rand::random::<f64>() > loss_probability {
            let receive_time = Utc::now();
            let round_trip_delay = (receive_time - send_time).to_std().unwrap_or(Duration::from_secs(0));
            let corrupted = rand::random::<f64>() < 0.0001; // 0.01% corruption rate
            let error_bits = if corrupted { (rand::random::<u8>() % 8) + 1 } else { 0 };
            let signal_quality = 90.0 + rand::random::<f64>() * 10.0;
            
            let measurement = FrameMeasurement {
                sequence_number,
                send_time,
                receive_time: Some(receive_time),
                round_trip_delay: Some(round_trip_delay),
                corrupted,
                error_bits: error_bits.into(),
                signal_quality,
            };
            
            let _ = event_tx.send(InterfaceTestEvent::FrameReceived {
                test_id,
                measurement,
            });
        }
    }

    /// Simulate frame reception and measurement
    async fn simulate_frame_reception(
        _config: InterfaceTestConfig,
        stats: Arc<RwLock<InterfaceTestStats>>,
        _measurements: Arc<RwLock<Vec<FrameMeasurement>>>,
        _event_tx: mpsc::UnboundedSender<InterfaceTestEvent>,
        mut frame_receiver: mpsc::UnboundedReceiver<Bytes>,
    ) {
        while let Some(_frame) = frame_receiver.recv().await {
            // Process received frame (in real implementation)
            {
                let mut stats_guard = stats.write().await;
                stats_guard.frames_received += 1;
            }
        }
    }

    /// Generate comprehensive test results
    async fn generate_test_result(
        &self,
        config: InterfaceTestConfig,
        stats: Arc<RwLock<InterfaceTestStats>>,
        measurements: Arc<RwLock<Vec<FrameMeasurement>>>,
    ) -> Result<InterfaceTestResult> {
        let final_stats = {
            let mut stats_guard = stats.write().await;
            
            // Calculate final statistics
            let measurements_guard = measurements.read().await;
            if !measurements_guard.is_empty() {
                let delays: Vec<Duration> = measurements_guard.iter()
                    .filter_map(|m| m.round_trip_delay)
                    .collect();
                
                if !delays.is_empty() {
                    stats_guard.min_delay = delays.iter().min().copied().unwrap_or_default();
                    stats_guard.max_delay = delays.iter().max().copied().unwrap_or_default();
                    
                    let total_delay: Duration = delays.iter().sum();
                    stats_guard.avg_delay = total_delay / delays.len() as u32;
                    
                    // Calculate jitter (standard deviation of delays)
                    let avg_micros = stats_guard.avg_delay.as_micros() as f64;
                    let variance: f64 = delays.iter()
                        .map(|d| {
                            let diff = d.as_micros() as f64 - avg_micros;
                            diff * diff
                        })
                        .sum::<f64>() / delays.len() as f64;
                    
                    stats_guard.jitter = Duration::from_micros(variance.sqrt() as u64);
                }
                
                // Calculate error rates
                stats_guard.frames_lost = stats_guard.frames_sent - stats_guard.frames_received;
                stats_guard.frames_corrupted = measurements_guard.iter()
                    .filter(|m| m.corrupted)
                    .count() as u64;
                
                stats_guard.frame_error_rate = if stats_guard.frames_sent > 0 {
                    stats_guard.frames_lost as f64 / stats_guard.frames_sent as f64
                } else {
                    0.0
                };
                
                let total_error_bits: u32 = measurements_guard.iter()
                    .map(|m| m.error_bits)
                    .sum();
                
                stats_guard.bit_error_rate = if stats_guard.bytes_sent > 0 {
                    total_error_bits as f64 / (stats_guard.bytes_sent * 8) as f64
                } else {
                    0.0
                };
                
                // Calculate throughput
                if stats_guard.elapsed_time.as_secs() > 0 {
                    stats_guard.current_throughput = (stats_guard.bytes_received * 8) as f64 / 
                        (stats_guard.elapsed_time.as_secs_f64() * 1_000_000.0); // Mbps
                }
            }
            
            stats_guard.clone()
        };

        let raw_measurements = measurements.read().await.clone();
        
        // Analyze results
        let success_rate = if final_stats.frames_sent > 0 {
            (final_stats.frames_received as f64 / final_stats.frames_sent as f64) * 100.0
        } else {
            0.0
        };
        
        let success = success_rate >= config.success_threshold;
        
        // Generate error analysis
        let mut error_analysis = Vec::new();
        let mut recommendations = Vec::new();
        
        if !success {
            if final_stats.frame_error_rate > 0.1 {
                error_analysis.push("High frame loss detected".to_string());
                recommendations.push("Check physical connections and cable integrity".to_string());
            }
            
            if final_stats.bit_error_rate > 0.001 {
                error_analysis.push("Excessive bit errors detected".to_string());
                recommendations.push("Verify signal levels and reduce electromagnetic interference".to_string());
            }
            
            if final_stats.avg_delay > config.expected_delay + config.tolerance {
                error_analysis.push("High latency detected".to_string());
                recommendations.push("Check network congestion and processing delays".to_string());
            }
            
            if final_stats.jitter > Duration::from_millis(10) {
                error_analysis.push("High jitter detected".to_string());
                recommendations.push("Improve timing synchronization and reduce network variability".to_string());
            }
        } else {
            recommendations.push("Test passed - system operating within specifications".to_string());
        }
        
        Ok(InterfaceTestResult {
            config,
            stats: final_stats,
            success,
            completion_time: Utc::now(),
            error_analysis,
            recommendations,
            raw_measurements,
        })
    }

    /// Stop a running test
    pub async fn stop_test(&self, test_id: Uuid) -> Result<()> {
        let active_tests = self.active_tests.read().await;
        if let Some(test) = active_tests.get(&test_id) {
            let _ = test.cancel_tx.send(());
            info!("Stopping test {}", test_id);
            Ok(())
        } else {
            Err(Error::invalid_state(format!("Test {} not found", test_id)))
        }
    }

    /// Get active test status
    pub async fn get_test_status(&self, test_id: Uuid) -> Option<InterfaceTestStats> {
        let active_tests = self.active_tests.read().await;
        if let Some(test) = active_tests.get(&test_id) {
            Some(test.stats.read().await.clone())
        } else {
            None
        }
    }

    /// Get all active tests
    pub async fn get_active_tests(&self) -> Vec<Uuid> {
        let active_tests = self.active_tests.read().await;
        active_tests.keys().copied().collect()
    }

    /// Get completed test results
    pub async fn get_test_result(&self, test_id: Uuid) -> Option<InterfaceTestResult> {
        let completed_tests = self.completed_tests.read().await;
        completed_tests.get(&test_id).cloned()
    }

    /// Get all completed test results
    pub async fn get_all_results(&self) -> Vec<InterfaceTestResult> {
        let completed_tests = self.completed_tests.read().await;
        completed_tests.values().cloned().collect()
    }
}

impl Clone for InterfaceTestingService {
    fn clone(&self) -> Self {
        Self {
            active_tests: Arc::clone(&self.active_tests),
            completed_tests: Arc::clone(&self.completed_tests),
            event_tx: self.event_tx.clone(),
            event_rx: None, // Don't clone receiver
            rng: Arc::clone(&self.rng),
        }
    }
}

impl Default for InterfaceTestingService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_service_creation() {
        let service = InterfaceTestingService::new();
        assert!(service.get_active_tests().await.is_empty());
    }

    #[tokio::test]
    async fn test_loopback_test_start() {
        let service = InterfaceTestingService::new();
        let test_id = service.start_tdmoe_loopback_test(
            1,
            Some(vec![1, 2, 3]),
            TestPattern::Prbs15,
            Duration::from_millis(100),
        ).await.unwrap();

        assert!(service.get_active_tests().await.contains(&test_id));
        
        // Wait for test to complete
        timeout(Duration::from_millis(500), async {
            while service.get_active_tests().await.contains(&test_id) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.ok();

        let result = service.get_test_result(test_id).await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_cross_port_test() {
        let service = InterfaceTestingService::new();
        let test_id = service.start_cross_port_test(
            1,
            2,
            None,
            TestPattern::Alternating,
            Duration::from_millis(100),
        ).await.unwrap();

        assert!(service.get_active_tests().await.contains(&test_id));
    }

    #[tokio::test]
    async fn test_pattern_generation() {
        let rng = Arc::new(RwLock::new(StdRng::from_entropy()));
        
        let zeros = InterfaceTestingService::generate_frame_data(&TestPattern::AllZeros, 10, &rng).await;
        assert_eq!(zeros.len(), 10);
        assert!(zeros.iter().all(|&b| b == 0));

        let ones = InterfaceTestingService::generate_frame_data(&TestPattern::AllOnes, 10, &rng).await;
        assert_eq!(ones.len(), 10);
        assert!(ones.iter().all(|&b| b == 0xFF));

        let alt = InterfaceTestingService::generate_frame_data(&TestPattern::Alternating, 4, &rng).await;
        assert_eq!(alt.len(), 4);
    }
}