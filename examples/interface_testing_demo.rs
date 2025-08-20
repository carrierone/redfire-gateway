//! Demonstration of the Interface Testing System
//! 
//! This example shows how to use the TDMoE loopback and cross-port wiring testing
//! capabilities of the Redfire Gateway.

use std::sync::Arc;
use std::time::Duration;

use redfire_gateway::services::{
    InterfaceTestingService, TestAutomationService, TestScenario, TestPattern,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    println!("🚀 Redfire Gateway Interface Testing Demo");
    println!("==========================================");

    // Create the interface testing service
    let interface_service = Arc::new(InterfaceTestingService::new());
    
    // Create the test automation service
    let automation_service = TestAutomationService::new(Arc::clone(&interface_service));
    
    // Demo 1: Basic TDMoE Loopback Test
    println!("\n📋 Demo 1: TDMoE Loopback Test");
    println!("------------------------------");
    
    let loopback_test_id = interface_service.start_tdmoe_loopback_test(
        1,                              // Span 1
        Some(vec![1, 2, 3, 4, 5]),     // Test channels 1-5
        TestPattern::Prbs15,            // PRBS-15 test pattern
        Duration::from_secs(10),        // 10 second test
    ).await?;
    
    println!("✅ Started TDMoE loopback test: {}", loopback_test_id);
    
    // Monitor test progress
    for i in 0..12 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        if let Some(stats) = interface_service.get_test_status(loopback_test_id).await {
            println!("   Progress: {} seconds | {} frames sent | {} frames received | {:.3}% loss",
                     stats.elapsed_time.as_secs(),
                     stats.frames_sent,
                     stats.frames_received,
                     stats.frame_error_rate * 100.0);
        } else {
            break; // Test completed
        }
    }
    
    // Get final results
    if let Some(result) = interface_service.get_test_result(loopback_test_id).await {
        println!("📊 Loopback Test Results:");
        println!("   Status: {}", if result.success { "✅ PASSED" } else { "❌ FAILED" });
        println!("   Frames: {} sent, {} received", result.stats.frames_sent, result.stats.frames_received);
        println!("   BER: {:.2e}", result.stats.bit_error_rate);
        println!("   Average Delay: {:?}", result.stats.avg_delay);
        println!("   Jitter: {:?}", result.stats.jitter);
    }
    
    // Demo 2: Cross-Port Wiring Test
    println!("\n📋 Demo 2: Cross-Port Wiring Test");
    println!("----------------------------------");
    
    let cross_port_test_id = interface_service.start_cross_port_test(
        1,                              // Source span 1
        2,                              // Destination span 2
        None,                           // Default channel mapping
        TestPattern::Alternating,       // Alternating pattern
        Duration::from_secs(15),        // 15 second test
    ).await?;
    
    println!("✅ Started cross-port wiring test: {}", cross_port_test_id);
    
    // Monitor test progress
    for i in 0..17 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        if let Some(stats) = interface_service.get_test_status(cross_port_test_id).await {
            println!("   Progress: {} seconds | Throughput: {:.2} Mbps | Sync losses: {}",
                     stats.elapsed_time.as_secs(),
                     stats.current_throughput,
                     stats.sync_losses);
        } else {
            break; // Test completed
        }
    }
    
    // Get final results
    if let Some(result) = interface_service.get_test_result(cross_port_test_id).await {
        println!("📊 Cross-Port Test Results:");
        println!("   Status: {}", if result.success { "✅ PASSED" } else { "❌ FAILED" });
        println!("   Success Rate: {:.2}%", 
                 (result.stats.frames_received as f64 / result.stats.frames_sent as f64) * 100.0);
        println!("   Throughput: {:.2} Mbps", result.stats.current_throughput);
        
        if !result.recommendations.is_empty() {
            println!("   Recommendations:");
            for rec in &result.recommendations {
                println!("     • {}", rec);
            }
        }
    }
    
    // Demo 3: End-to-End Call Test
    println!("\n📋 Demo 3: End-to-End Call Test");
    println!("--------------------------------");
    
    let call_test_id = interface_service.start_end_to_end_test(
        1,                              // Calling span 1
        2,                              // Called span 2
        Duration::from_secs(20),        // 20 second call
    ).await?;
    
    println!("✅ Started end-to-end call test: {}", call_test_id);
    
    // Monitor test progress
    for i in 0..22 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        if let Some(stats) = interface_service.get_test_status(call_test_id).await {
            println!("   Call duration: {} seconds | Voice quality: {:.1} MOS (estimated)",
                     stats.elapsed_time.as_secs(),
                     4.5 - (stats.jitter.as_millis() as f64 * 0.01)); // Simple MOS estimation
        } else {
            break; // Test completed
        }
    }
    
    // Get final results
    if let Some(result) = interface_service.get_test_result(call_test_id).await {
        println!("📊 Call Test Results:");
        println!("   Status: {}", if result.success { "✅ PASSED" } else { "❌ FAILED" });
        println!("   Call Quality: {:.1} MOS", 
                 4.5 - (result.stats.jitter.as_millis() as f64 * 0.01));
        println!("   Latency: {:?}", result.stats.avg_delay);
    }
    
    // Demo 4: Automated Test Suite
    println!("\n📋 Demo 4: Automated Test Suite");
    println!("--------------------------------");
    
    let scenario = TestScenario::BasicConnectivity {
        spans: vec![1, 2, 3],
    };
    
    let session_id = automation_service.start_session(scenario).await?;
    println!("✅ Started automated test session: {}", session_id);
    
    // Monitor session progress
    while let Some(session) = automation_service.get_session_status(session_id).await {
        println!("   Session progress: {}/{} tests completed", 
                 session.current_step, session.total_steps);
        
        match session.status {
            redfire_gateway::services::test_automation::SessionStatus::Completed => break,
            redfire_gateway::services::test_automation::SessionStatus::Failed => {
                println!("❌ Session failed");
                break;
            },
            _ => {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
    
    // Get final session summary
    if let Some(summary) = automation_service.get_session_summary(session_id).await {
        println!("📊 Test Suite Summary:");
        println!("   Overall Status: {}", if summary.overall_success { "✅ PASSED" } else { "❌ FAILED" });
        println!("   Tests: {} passed, {} failed", summary.passed_tests, summary.failed_tests);
        println!("   Duration: {:?}", summary.duration);
        
        if !summary.critical_issues.is_empty() {
            println!("   Critical Issues:");
            for issue in &summary.critical_issues {
                println!("     • {}", issue);
            }
        }
        
        if !summary.recommendations.is_empty() {
            println!("   Recommendations:");
            for rec in &summary.recommendations {
                println!("     • {}", rec);
            }
        }
    }
    
    println!("\n🎉 Interface Testing Demo Complete!");
    println!("====================================");
    println!();
    println!("📚 Available CLI Commands:");
    println!("   cargo run --bin interface-test loopback --span 1 --duration 30");
    println!("   cargo run --bin interface-test cross-port --source-span 1 --dest-span 2");
    println!("   cargo run --bin interface-test call-test --calling-span 1 --called-span 2");
    println!("   cargo run --bin interface-test suite \"1,2,3\" --loopback --cross-port --duration 60");
    println!("   cargo run --bin interface-test monitor");
    println!("   cargo run --bin interface-test results --detailed");
    
    Ok(())
}