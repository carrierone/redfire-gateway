//! Test automation service for orchestrating complex test scenarios

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, sleep};
use tracing::{info, error};
use uuid::Uuid;

use crate::services::interface_testing::{
    InterfaceTestingService, TestPattern, InterfaceTestType, InterfaceTestResult
};
use crate::{Error, Result};

/// Test scenario definitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestScenario {
    /// Basic connectivity validation
    BasicConnectivity {
        spans: Vec<u32>,
    },
    
    /// Comprehensive system validation
    SystemValidation {
        spans: Vec<u32>,
        duration_per_test: u64,
        include_stress_tests: bool,
    },
    
    /// Production readiness test
    ProductionReadiness {
        spans: Vec<u32>,
        call_volume: u32,
        duration_hours: u8,
    },
    
    /// Troubleshooting scenario for specific issues
    Troubleshooting {
        problem_spans: Vec<u32>,
        suspected_issue: TroubleshootingIssue,
    },
    
    /// Custom test sequence
    Custom {
        name: String,
        test_sequence: Vec<CustomTestStep>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TroubleshootingIssue {
    HighLatency,
    PacketLoss,
    BitErrors,
    SyncIssues,
    CrossTalk,
    TimingDrift,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomTestStep {
    pub name: String,
    pub test_type: InterfaceTestType,
    pub source_span: u32,
    pub dest_span: Option<u32>,
    pub pattern: TestPattern,
    pub duration: Duration,
    pub success_criteria: SuccessCriteria,
    pub wait_before: Option<Duration>,
    pub wait_after: Option<Duration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessCriteria {
    pub min_success_rate: f64,
    pub max_bit_error_rate: f64,
    pub max_frame_error_rate: f64,
    pub max_average_delay: Duration,
    pub max_jitter: Duration,
}

impl Default for SuccessCriteria {
    fn default() -> Self {
        Self {
            min_success_rate: 99.0,
            max_bit_error_rate: 1e-6,
            max_frame_error_rate: 0.01,
            max_average_delay: Duration::from_millis(50),
            max_jitter: Duration::from_millis(10),
        }
    }
}

/// Test automation session
#[derive(Debug, Clone)]
pub struct TestSession {
    pub session_id: Uuid,
    pub scenario: TestScenario,
    pub start_time: DateTime<Utc>,
    pub status: SessionStatus,
    pub current_step: usize,
    pub total_steps: usize,
    pub test_results: Vec<Uuid>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// Test automation events
#[derive(Debug, Clone)]
pub enum AutomationEvent {
    SessionStarted {
        session_id: Uuid,
        scenario: TestScenario,
    },
    SessionStepStarted {
        session_id: Uuid,
        step: usize,
        step_name: String,
    },
    SessionStepCompleted {
        session_id: Uuid,
        step: usize,
        success: bool,
        test_id: Uuid,
    },
    SessionCompleted {
        session_id: Uuid,
        success: bool,
        summary: SessionSummary,
    },
    SessionFailed {
        session_id: Uuid,
        error: String,
    },
    SessionPaused {
        session_id: Uuid,
    },
    SessionResumed {
        session_id: Uuid,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: Uuid,
    pub scenario_name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration: Duration,
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub overall_success: bool,
    pub recommendations: Vec<String>,
    pub critical_issues: Vec<String>,
}

/// Test automation service
pub struct TestAutomationService {
    interface_testing: Arc<InterfaceTestingService>,
    active_sessions: Arc<RwLock<HashMap<Uuid, TestSession>>>,
    completed_sessions: Arc<RwLock<HashMap<Uuid, SessionSummary>>>,
    event_tx: mpsc::UnboundedSender<AutomationEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<AutomationEvent>>,
}

impl TestAutomationService {
    pub fn new(interface_testing: Arc<InterfaceTestingService>) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        Self {
            interface_testing,
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
            completed_sessions: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            event_rx: Some(event_rx),
        }
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<AutomationEvent>> {
        self.event_rx.take()
    }

    /// Start a test automation session
    pub async fn start_session(&self, scenario: TestScenario) -> Result<Uuid> {
        let session_id = Uuid::new_v4();
        let test_steps = self.build_test_steps(&scenario).await?;
        
        let session = TestSession {
            session_id,
            scenario: scenario.clone(),
            start_time: Utc::now(),
            status: SessionStatus::Pending,
            current_step: 0,
            total_steps: test_steps.len(),
            test_results: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        // Add to active sessions
        {
            let mut sessions = self.active_sessions.write().await;
            sessions.insert(session_id, session);
        }

        // Send start event
        let _ = self.event_tx.send(AutomationEvent::SessionStarted {
            session_id,
            scenario: scenario.clone(),
        });

        // Spawn execution task
        let service = self.clone();
        tokio::spawn(async move {
            let result = service.execute_session(session_id, test_steps).await;
            
            match result {
                Ok(summary) => {
                    let _ = service.event_tx.send(AutomationEvent::SessionCompleted {
                        session_id,
                        success: summary.overall_success,
                        summary: summary.clone(),
                    });
                    
                    // Move to completed sessions
                    {
                        let mut completed = service.completed_sessions.write().await;
                        completed.insert(session_id, summary);
                    }
                },
                Err(e) => {
                    let _ = service.event_tx.send(AutomationEvent::SessionFailed {
                        session_id,
                        error: e.to_string(),
                    });
                }
            }

            // Remove from active sessions
            {
                let mut sessions = service.active_sessions.write().await;
                sessions.remove(&session_id);
            }
        });

        info!("Started test automation session {}", session_id);
        Ok(session_id)
    }

    /// Build test steps from scenario
    async fn build_test_steps(&self, scenario: &TestScenario) -> Result<Vec<CustomTestStep>> {
        match scenario {
            TestScenario::BasicConnectivity { spans } => {
                let mut steps = Vec::new();
                
                // Loopback tests for each span
                for &span in spans {
                    steps.push(CustomTestStep {
                        name: format!("Loopback Test - Span {}", span),
                        test_type: InterfaceTestType::TdmoeLoopback,
                        source_span: span,
                        dest_span: None,
                        pattern: TestPattern::Prbs15,
                        duration: Duration::from_secs(30),
                        success_criteria: SuccessCriteria::default(),
                        wait_before: Some(Duration::from_secs(2)),
                        wait_after: Some(Duration::from_secs(1)),
                    });
                }
                
                // Cross-port tests between spans
                for i in 0..spans.len() {
                    for j in (i + 1)..spans.len() {
                        steps.push(CustomTestStep {
                            name: format!("Cross-Port Test - Span {} to {}", spans[i], spans[j]),
                            test_type: InterfaceTestType::CrossPortWiring,
                            source_span: spans[i],
                            dest_span: Some(spans[j]),
                            pattern: TestPattern::Alternating,
                            duration: Duration::from_secs(45),
                            success_criteria: SuccessCriteria {
                                min_success_rate: 98.0,
                                ..Default::default()
                            },
                            wait_before: Some(Duration::from_secs(2)),
                            wait_after: Some(Duration::from_secs(1)),
                        });
                    }
                }
                
                Ok(steps)
            },
            
            TestScenario::SystemValidation { spans, duration_per_test, include_stress_tests } => {
                let mut steps = Vec::new();
                let test_duration = Duration::from_secs(*duration_per_test);
                
                // Phase 1: Basic connectivity
                for &span in spans {
                    steps.push(CustomTestStep {
                        name: format!("System Validation - Loopback Span {}", span),
                        test_type: InterfaceTestType::TdmoeLoopback,
                        source_span: span,
                        dest_span: None,
                        pattern: TestPattern::Prbs23,
                        duration: test_duration,
                        success_criteria: SuccessCriteria::default(),
                        wait_before: Some(Duration::from_secs(3)),
                        wait_after: Some(Duration::from_secs(2)),
                    });
                }
                
                // Phase 2: Cross-port validation
                for i in 0..spans.len() {
                    for j in (i + 1)..spans.len() {
                        steps.push(CustomTestStep {
                            name: format!("System Validation - Cross-Port {} to {}", spans[i], spans[j]),
                            test_type: InterfaceTestType::CrossPortWiring,
                            source_span: spans[i],
                            dest_span: Some(spans[j]),
                            pattern: TestPattern::Prbs15,
                            duration: test_duration,
                            success_criteria: SuccessCriteria::default(),
                            wait_before: Some(Duration::from_secs(3)),
                            wait_after: Some(Duration::from_secs(2)),
                        });
                    }
                }
                
                // Phase 3: Protocol stack tests
                for &span in spans {
                    steps.push(CustomTestStep {
                        name: format!("System Validation - Protocol Stack Span {}", span),
                        test_type: InterfaceTestType::ProtocolStackTest,
                        source_span: span,
                        dest_span: None,
                        pattern: TestPattern::Q931Setup,
                        duration: test_duration,
                        success_criteria: SuccessCriteria::default(),
                        wait_before: Some(Duration::from_secs(5)),
                        wait_after: Some(Duration::from_secs(2)),
                    });
                }
                
                // Phase 4: End-to-end call tests
                for i in 0..spans.len() {
                    for j in (i + 1)..spans.len() {
                        steps.push(CustomTestStep {
                            name: format!("System Validation - E2E Call {} to {}", spans[i], spans[j]),
                            test_type: InterfaceTestType::EndToEndCall,
                            source_span: spans[i],
                            dest_span: Some(spans[j]),
                            pattern: TestPattern::ToneGeneration(1000.0),
                            duration: test_duration,
                            success_criteria: SuccessCriteria {
                                min_success_rate: 95.0,
                                max_average_delay: Duration::from_millis(100),
                                ..Default::default()
                            },
                            wait_before: Some(Duration::from_secs(5)),
                            wait_after: Some(Duration::from_secs(3)),
                        });
                    }
                }
                
                // Phase 5: Stress tests (if enabled)
                if *include_stress_tests {
                    for &span in spans {
                        steps.push(CustomTestStep {
                            name: format!("System Validation - Stress Test Span {}", span),
                            test_type: InterfaceTestType::TdmoeLoopback,
                            source_span: span,
                            dest_span: None,
                            pattern: TestPattern::Prbs31,
                            duration: Duration::from_secs(test_duration.as_secs() * 3), // Longer stress test
                            success_criteria: SuccessCriteria {
                                min_success_rate: 97.0, // Lower threshold for stress test
                                ..Default::default()
                            },
                            wait_before: Some(Duration::from_secs(10)),
                            wait_after: Some(Duration::from_secs(5)),
                        });
                    }
                }
                
                Ok(steps)
            },
            
            TestScenario::ProductionReadiness { spans, call_volume: _, duration_hours } => {
                let mut steps = Vec::new();
                let base_duration = Duration::from_secs((*duration_hours as u64) * 3600 / spans.len() as u64);
                
                // Long-running stability tests
                for &span in spans {
                    steps.push(CustomTestStep {
                        name: format!("Production Readiness - Stability Test Span {}", span),
                        test_type: InterfaceTestType::TdmoeLoopback,
                        source_span: span,
                        dest_span: None,
                        pattern: TestPattern::Prbs23,
                        duration: base_duration,
                        success_criteria: SuccessCriteria {
                            min_success_rate: 99.5,
                            max_bit_error_rate: 1e-9,
                            max_frame_error_rate: 0.001,
                            ..Default::default()
                        },
                        wait_before: Some(Duration::from_secs(30)),
                        wait_after: Some(Duration::from_secs(10)),
                    });
                }
                
                // High-volume call simulation
                for i in 0..spans.len() {
                    for j in (i + 1)..spans.len() {
                        steps.push(CustomTestStep {
                            name: format!("Production Readiness - High Volume {} to {}", spans[i], spans[j]),
                            test_type: InterfaceTestType::EndToEndCall,
                            source_span: spans[i],
                            dest_span: Some(spans[j]),
                            pattern: TestPattern::ToneGeneration(800.0),
                            duration: base_duration / 2,
                            success_criteria: SuccessCriteria {
                                min_success_rate: 99.0,
                                max_average_delay: Duration::from_millis(150),
                                max_jitter: Duration::from_millis(20),
                                ..Default::default()
                            },
                            wait_before: Some(Duration::from_secs(60)),
                            wait_after: Some(Duration::from_secs(30)),
                        });
                    }
                }
                
                Ok(steps)
            },
            
            TestScenario::Troubleshooting { problem_spans, suspected_issue } => {
                let mut steps = Vec::new();
                
                match suspected_issue {
                    TroubleshootingIssue::HighLatency => {
                        for &span in problem_spans {
                            // Latency measurement test
                            steps.push(CustomTestStep {
                                name: format!("Troubleshoot Latency - Span {}", span),
                                test_type: InterfaceTestType::TimingSyncTest,
                                source_span: span,
                                dest_span: None,
                                pattern: TestPattern::Custom(vec![0x55, 0xAA]), // Fast alternating pattern
                                duration: Duration::from_secs(60),
                                success_criteria: SuccessCriteria {
                                    max_average_delay: Duration::from_micros(200), // Very strict
                                    max_jitter: Duration::from_micros(50),
                                    ..Default::default()
                                },
                                wait_before: Some(Duration::from_secs(5)),
                                wait_after: Some(Duration::from_secs(2)),
                            });
                        }
                    },
                    
                    TroubleshootingIssue::PacketLoss => {
                        for &span in problem_spans {
                            // High-frequency test to detect packet loss
                            steps.push(CustomTestStep {
                                name: format!("Troubleshoot Packet Loss - Span {}", span),
                                test_type: InterfaceTestType::TdmoeLoopback,
                                source_span: span,
                                dest_span: None,
                                pattern: TestPattern::Prbs15,
                                duration: Duration::from_secs(120),
                                success_criteria: SuccessCriteria {
                                    min_success_rate: 99.99,
                                    max_frame_error_rate: 0.0001,
                                    ..Default::default()
                                },
                                wait_before: Some(Duration::from_secs(3)),
                                wait_after: Some(Duration::from_secs(2)),
                            });
                        }
                    },
                    
                    TroubleshootingIssue::BitErrors => {
                        for &span in problem_spans {
                            // Comprehensive bit error analysis
                            for pattern in [TestPattern::AllZeros, TestPattern::AllOnes, TestPattern::Prbs31] {
                                steps.push(CustomTestStep {
                                    name: format!("Troubleshoot Bit Errors - Span {} Pattern {:?}", span, pattern),
                                    test_type: InterfaceTestType::TdmoeLoopback,
                                    source_span: span,
                                    dest_span: None,
                                    pattern,
                                    duration: Duration::from_secs(90),
                                    success_criteria: SuccessCriteria {
                                        max_bit_error_rate: 1e-12,
                                        ..Default::default()
                                    },
                                    wait_before: Some(Duration::from_secs(5)),
                                    wait_after: Some(Duration::from_secs(3)),
                                });
                            }
                        }
                    },
                    
                    TroubleshootingIssue::SyncIssues => {
                        for &span in problem_spans {
                            steps.push(CustomTestStep {
                                name: format!("Troubleshoot Sync - Span {}", span),
                                test_type: InterfaceTestType::TimingSyncTest,
                                source_span: span,
                                dest_span: None,
                                pattern: TestPattern::LapdFrames,
                                duration: Duration::from_secs(180),
                                success_criteria: SuccessCriteria {
                                    max_jitter: Duration::from_micros(10),
                                    ..Default::default()
                                },
                                wait_before: Some(Duration::from_secs(10)),
                                wait_after: Some(Duration::from_secs(5)),
                            });
                        }
                    },
                    
                    TroubleshootingIssue::CrossTalk => {
                        // Test adjacent spans for crosstalk
                        for i in 0..problem_spans.len() {
                            for j in (i + 1)..problem_spans.len() {
                                if (problem_spans[j] as i32 - problem_spans[i] as i32).abs() <= 1 {
                                    steps.push(CustomTestStep {
                                        name: format!("Troubleshoot Crosstalk - Span {} vs {}", 
                                                    problem_spans[i], problem_spans[j]),
                                        test_type: InterfaceTestType::CrossPortWiring,
                                        source_span: problem_spans[i],
                                        dest_span: Some(problem_spans[j]),
                                        pattern: TestPattern::Alternating,
                                        duration: Duration::from_secs(300),
                                        success_criteria: SuccessCriteria {
                                            max_bit_error_rate: 1e-9,
                                            ..Default::default()
                                        },
                                        wait_before: Some(Duration::from_secs(10)),
                                        wait_after: Some(Duration::from_secs(5)),
                                    });
                                }
                            }
                        }
                    },
                    
                    TroubleshootingIssue::TimingDrift => {
                        for &span in problem_spans {
                            steps.push(CustomTestStep {
                                name: format!("Troubleshoot Timing Drift - Span {}", span),
                                test_type: InterfaceTestType::TimingSyncTest,
                                source_span: span,
                                dest_span: None,
                                pattern: TestPattern::Custom(vec![0x00, 0xFF]), // Timing pattern
                                duration: Duration::from_secs(600), // Long test for drift detection
                                success_criteria: SuccessCriteria {
                                    max_jitter: Duration::from_micros(5),
                                    max_average_delay: Duration::from_micros(125), // Strict timing
                                    ..Default::default()
                                },
                                wait_before: Some(Duration::from_secs(30)),
                                wait_after: Some(Duration::from_secs(10)),
                            });
                        }
                    },
                }
                
                Ok(steps)
            },
            
            TestScenario::Custom { name: _, test_sequence } => {
                Ok(test_sequence.clone())
            },
        }
    }

    /// Execute a test automation session
    async fn execute_session(
        &self,
        session_id: Uuid,
        test_steps: Vec<CustomTestStep>,
    ) -> Result<SessionSummary> {
        info!("Executing test session {}", session_id);
        
        // Update session status
        {
            let mut sessions = self.active_sessions.write().await;
            if let Some(session) = sessions.get_mut(&session_id) {
                session.status = SessionStatus::Running;
            }
        }

        let mut passed_tests = 0;
        let mut failed_tests = 0;
        let mut test_results = Vec::new();
        let mut critical_issues = Vec::new();
        let mut recommendations = Vec::new();

        for (step_index, step) in test_steps.iter().enumerate() {
            // Send step start event
            let _ = self.event_tx.send(AutomationEvent::SessionStepStarted {
                session_id,
                step: step_index,
                step_name: step.name.clone(),
            });

            // Wait before test if specified
            if let Some(wait_time) = step.wait_before {
                sleep(wait_time).await;
            }

            // Execute the test
            let test_result = match step.test_type {
                InterfaceTestType::TdmoeLoopback => {
                    self.interface_testing.start_tdmoe_loopback_test(
                        step.source_span,
                        None,
                        step.pattern.clone(),
                        step.duration,
                    ).await
                },
                InterfaceTestType::CrossPortWiring => {
                    if let Some(dest_span) = step.dest_span {
                        self.interface_testing.start_cross_port_test(
                            step.source_span,
                            dest_span,
                            None,
                            step.pattern.clone(),
                            step.duration,
                        ).await
                    } else {
                        return Err(Error::invalid_state("Cross-port test requires destination span"));
                    }
                },
                InterfaceTestType::EndToEndCall => {
                    if let Some(dest_span) = step.dest_span {
                        self.interface_testing.start_end_to_end_test(
                            step.source_span,
                            dest_span,
                            step.duration,
                        ).await
                    } else {
                        return Err(Error::invalid_state("End-to-end test requires destination span"));
                    }
                },
                _ => {
                    // For other test types, use loopback as fallback
                    self.interface_testing.start_tdmoe_loopback_test(
                        step.source_span,
                        None,
                        step.pattern.clone(),
                        step.duration,
                    ).await
                },
            };

            match test_result {
                Ok(test_id) => {
                    info!("Started test {} for session {} step {}", test_id, session_id, step_index);
                    
                    // Wait for test completion
                    let result = self.wait_for_test_completion(test_id).await?;
                    test_results.push(test_id);
                    
                    // Evaluate test success
                    let success = self.evaluate_test_success(&result, &step.success_criteria).await;
                    
                    if success {
                        passed_tests += 1;
                    } else {
                        failed_tests += 1;
                        
                        // Analyze failure and generate recommendations
                        let (issues, recs) = self.analyze_test_failure(&result, &step.success_criteria).await;
                        critical_issues.extend(issues);
                        recommendations.extend(recs);
                    }
                    
                    // Send step completion event
                    let _ = self.event_tx.send(AutomationEvent::SessionStepCompleted {
                        session_id,
                        step: step_index,
                        success,
                        test_id,
                    });
                    
                    // Update session
                    {
                        let mut sessions = self.active_sessions.write().await;
                        if let Some(session) = sessions.get_mut(&session_id) {
                            session.current_step = step_index + 1;
                            session.test_results.push(test_id);
                        }
                    }
                },
                Err(e) => {
                    error!("Failed to start test for session {} step {}: {}", session_id, step_index, e);
                    failed_tests += 1;
                    critical_issues.push(format!("Failed to start test step '{}': {}", step.name, e));
                }
            }

            // Wait after test if specified
            if let Some(wait_time) = step.wait_after {
                sleep(wait_time).await;
            }
        }

        // Generate overall recommendations
        if failed_tests == 0 {
            recommendations.push("All tests passed successfully - system is ready for operation".to_string());
        } else if failed_tests < passed_tests {
            recommendations.push("Some tests failed - investigate specific issues before production deployment".to_string());
        } else {
            recommendations.push("Majority of tests failed - system requires significant troubleshooting".to_string());
        }

        let summary = SessionSummary {
            session_id,
            scenario_name: "Automated Test Session".to_string(),
            start_time: Utc::now() - Duration::from_secs(600), // Approximate
            end_time: Utc::now(),
            duration: Duration::from_secs(600), // Approximate
            total_tests: test_steps.len(),
            passed_tests,
            failed_tests,
            overall_success: failed_tests == 0,
            recommendations,
            critical_issues,
        };

        info!("Completed test session {}: {} passed, {} failed", 
              session_id, passed_tests, failed_tests);

        Ok(summary)
    }

    /// Wait for a test to complete and return results
    async fn wait_for_test_completion(&self, test_id: Uuid) -> Result<InterfaceTestResult> {
        let mut check_interval = interval(Duration::from_secs(2));
        
        loop {
            check_interval.tick().await;
            
            if let Some(result) = self.interface_testing.get_test_result(test_id).await {
                return Ok(result);
            }
            
            // Check if test is still active
            let active_tests = self.interface_testing.get_active_tests().await;
            if !active_tests.contains(&test_id) {
                // Test completed but no result - might be an error
                return Err(Error::internal("Test completed but no result available"));
            }
        }
    }

    /// Evaluate if a test meets the success criteria
    async fn evaluate_test_success(
        &self,
        result: &InterfaceTestResult,
        criteria: &SuccessCriteria,
    ) -> bool {
        if !result.success {
            return false;
        }

        let success_rate = if result.stats.frames_sent > 0 {
            (result.stats.frames_received as f64 / result.stats.frames_sent as f64) * 100.0
        } else {
            0.0
        };

        success_rate >= criteria.min_success_rate
            && result.stats.bit_error_rate <= criteria.max_bit_error_rate
            && result.stats.frame_error_rate <= criteria.max_frame_error_rate
            && result.stats.avg_delay <= criteria.max_average_delay
            && result.stats.jitter <= criteria.max_jitter
    }

    /// Analyze test failure and generate recommendations
    async fn analyze_test_failure(
        &self,
        result: &InterfaceTestResult,
        criteria: &SuccessCriteria,
    ) -> (Vec<String>, Vec<String>) {
        let mut issues = Vec::new();
        let mut recommendations = Vec::new();

        let success_rate = if result.stats.frames_sent > 0 {
            (result.stats.frames_received as f64 / result.stats.frames_sent as f64) * 100.0
        } else {
            0.0
        };

        if success_rate < criteria.min_success_rate {
            issues.push(format!("Low success rate: {:.2}% (required: {:.2}%)", 
                              success_rate, criteria.min_success_rate));
            recommendations.push("Check physical connections and cable integrity".to_string());
        }

        if result.stats.bit_error_rate > criteria.max_bit_error_rate {
            issues.push(format!("High bit error rate: {:.2e} (max: {:.2e})", 
                              result.stats.bit_error_rate, criteria.max_bit_error_rate));
            recommendations.push("Verify signal levels and reduce electromagnetic interference".to_string());
        }

        if result.stats.frame_error_rate > criteria.max_frame_error_rate {
            issues.push(format!("High frame error rate: {:.2e} (max: {:.2e})", 
                              result.stats.frame_error_rate, criteria.max_frame_error_rate));
            recommendations.push("Check framing configuration and synchronization".to_string());
        }

        if result.stats.avg_delay > criteria.max_average_delay {
            issues.push(format!("High average delay: {:?} (max: {:?})", 
                              result.stats.avg_delay, criteria.max_average_delay));
            recommendations.push("Optimize processing pipeline and reduce buffering delays".to_string());
        }

        if result.stats.jitter > criteria.max_jitter {
            issues.push(format!("High jitter: {:?} (max: {:?})", 
                              result.stats.jitter, criteria.max_jitter));
            recommendations.push("Improve timing synchronization and reduce network variability".to_string());
        }

        (issues, recommendations)
    }

    /// Get session status
    pub async fn get_session_status(&self, session_id: Uuid) -> Option<TestSession> {
        let sessions = self.active_sessions.read().await;
        sessions.get(&session_id).cloned()
    }

    /// Get completed session summary
    pub async fn get_session_summary(&self, session_id: Uuid) -> Option<SessionSummary> {
        let completed = self.completed_sessions.read().await;
        completed.get(&session_id).cloned()
    }

    /// Cancel a running session
    pub async fn cancel_session(&self, session_id: Uuid) -> Result<()> {
        let mut sessions = self.active_sessions.write().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.status = SessionStatus::Cancelled;
            
            // Stop all active tests for this session
            for &test_id in &session.test_results {
                let _ = self.interface_testing.stop_test(test_id).await;
            }
            
            info!("Cancelled test session {}", session_id);
            Ok(())
        } else {
            Err(Error::invalid_state(format!("Session {} not found", session_id)))
        }
    }

    /// Get all active sessions
    pub async fn get_active_sessions(&self) -> Vec<TestSession> {
        let sessions = self.active_sessions.read().await;
        sessions.values().cloned().collect()
    }

    /// Get all completed session summaries
    pub async fn get_completed_sessions(&self) -> Vec<SessionSummary> {
        let completed = self.completed_sessions.read().await;
        completed.values().cloned().collect()
    }
}

impl Clone for TestAutomationService {
    fn clone(&self) -> Self {
        Self {
            interface_testing: Arc::clone(&self.interface_testing),
            active_sessions: Arc::clone(&self.active_sessions),
            completed_sessions: Arc::clone(&self.completed_sessions),
            event_tx: self.event_tx.clone(),
            event_rx: None, // Don't clone receiver
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_service_creation() {
        let interface_service = Arc::new(InterfaceTestingService::new());
        let automation_service = TestAutomationService::new(interface_service);
        
        assert!(automation_service.get_active_sessions().await.is_empty());
    }

    #[tokio::test]
    async fn test_basic_connectivity_scenario() {
        let interface_service = Arc::new(InterfaceTestingService::new());
        let automation_service = TestAutomationService::new(interface_service);
        
        let scenario = TestScenario::BasicConnectivity {
            spans: vec![1, 2],
        };
        
        let session_id = automation_service.start_session(scenario).await.unwrap();
        
        // Wait a bit for session to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let status = automation_service.get_session_status(session_id).await;
        assert!(status.is_some());
    }
}