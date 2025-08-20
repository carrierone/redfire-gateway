//! Interface Testing CLI Tool for TDMoE and Cross-Port Testing

use std::collections::HashMap;
use std::time::Duration;

// Removed unused import
use clap::{Parser, Subcommand};
use colored::*;
use tokio::time::sleep;
use uuid::Uuid;

use redfire_gateway::services::interface_testing::{
    InterfaceTestingService, TestPattern,
};

#[derive(Parser)]
#[command(name = "interface-test")]
#[command(about = "Redfire Gateway Interface Testing Tool")]
#[command(version = redfire_gateway::VERSION)]
struct TestCli {
    #[command(subcommand)]
    command: TestCommands,
    
    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
    
    /// Output format (text, json)
    #[arg(short, long, default_value = "text")]
    format: String,
}

#[derive(Subcommand)]
enum TestCommands {
    /// TDMoE loopback testing
    Loopback {
        /// Span ID to test
        span: u32,
        
        /// Specific channels to test (comma-separated, e.g., "1,2,3")
        #[arg(short, long)]
        channels: Option<String>,
        
        /// Test pattern
        #[arg(short, long, default_value = "prbs15")]
        pattern: String,
        
        /// Test duration in seconds
        #[arg(short, long, default_value = "30")]
        duration: u64,
        
        /// Run test continuously until stopped
        #[arg(long)]
        continuous: bool,
    },
    
    /// Cross-port wiring tests
    CrossPort {
        /// Source span ID
        #[arg(short, long)]
        source_span: u32,
        
        /// Destination span ID
        #[arg(short, long)]
        dest_span: u32,
        
        /// Channel mapping (format: "1:1,2:2,3:3")
        #[arg(short, long)]
        mapping: Option<String>,
        
        /// Test pattern
        #[arg(short, long, default_value = "alternating")]
        pattern: String,
        
        /// Test duration in seconds
        #[arg(short, long, default_value = "60")]
        duration: u64,
    },
    
    /// End-to-end call testing
    CallTest {
        /// Calling span ID
        #[arg(short, long)]
        calling_span: u32,
        
        /// Called span ID
        #[arg(short, long)]
        called_span: u32,
        
        /// Test tone frequency in Hz
        #[arg(short, long, default_value = "1000")]
        frequency: f64,
        
        /// Call duration in seconds
        #[arg(short, long, default_value = "60")]
        duration: u64,
    },
    
    /// Comprehensive test suite
    Suite {
        /// Spans to include in test suite (comma-separated)
        spans: String,
        
        /// Include loopback tests
        #[arg(long)]
        loopback: bool,
        
        /// Include cross-port tests
        #[arg(long)]
        cross_port: bool,
        
        /// Include call tests
        #[arg(long)]
        call_tests: bool,
        
        /// Test duration for each test in seconds
        #[arg(short, long, default_value = "30")]
        duration: u64,
    },
    
    /// Monitor active tests
    Monitor {
        /// Test ID to monitor
        test_id: Option<String>,
        
        /// Refresh interval in seconds
        #[arg(short, long, default_value = "1")]
        interval: u64,
    },
    
    /// Show test results
    Results {
        /// Test ID to show results for
        test_id: Option<String>,
        
        /// Show detailed measurements
        #[arg(short, long)]
        detailed: bool,
        
        /// Export results to file
        #[arg(short, long)]
        export: Option<String>,
    },
    
    /// Stop a running test
    Stop {
        /// Test ID to stop
        test_id: String,
    },
    
    /// List all tests (active and completed)
    List {
        /// Show only active tests
        #[arg(short, long)]
        active: bool,
        
        /// Show only completed tests
        #[arg(short, long)]
        completed: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let cli = TestCli::parse();
    let service = InterfaceTestingService::new();
    
    match cli.command {
        TestCommands::Loopback { span, channels, pattern, duration, continuous } => {
            run_loopback_test(&service, span, channels, &pattern, duration, continuous).await?;
        },
        TestCommands::CrossPort { source_span, dest_span, mapping, pattern, duration } => {
            run_cross_port_test(&service, source_span, dest_span, mapping, &pattern, duration).await?;
        },
        TestCommands::CallTest { calling_span, called_span, frequency, duration } => {
            run_call_test(&service, calling_span, called_span, frequency, duration).await?;
        },
        TestCommands::Suite { spans, loopback, cross_port, call_tests, duration } => {
            run_test_suite(&service, &spans, loopback, cross_port, call_tests, duration).await?;
        },
        TestCommands::Monitor { test_id, interval } => {
            monitor_tests(&service, test_id, interval).await?;
        },
        TestCommands::Results { test_id, detailed, export } => {
            show_results(&service, test_id, detailed, export).await?;
        },
        TestCommands::Stop { test_id } => {
            stop_test(&service, &test_id).await?;
        },
        TestCommands::List { active, completed } => {
            list_tests(&service, active, completed).await?;
        },
    }
    
    Ok(())
}

async fn run_loopback_test(
    service: &InterfaceTestingService,
    span: u32,
    channels: Option<String>,
    pattern: &str,
    duration: u64,
    continuous: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "🔄 TDMoE Loopback Test".bold().blue());
    println!("Span: {}", span.to_string().yellow());
    
    let channel_list = if let Some(ch_str) = channels {
        let channels: Result<Vec<u8>, _> = ch_str.split(',')
            .map(|s| s.trim().parse())
            .collect();
        Some(channels?)
    } else {
        None
    };
    
    if let Some(ref channels) = channel_list {
        println!("Channels: {}", format!("{:?}", channels).yellow());
    } else {
        println!("Channels: {}", "All".yellow());
    }
    
    println!("Pattern: {}", pattern.yellow());
    println!("Duration: {} seconds", duration.to_string().yellow());
    
    let test_pattern = parse_pattern(pattern)?;
    let test_duration = Duration::from_secs(duration);
    
    loop {
        println!("\n{}", "Starting loopback test...".green());
        
        let test_id = service.start_tdmoe_loopback_test(
            span,
            channel_list.clone(),
            test_pattern.clone(),
            test_duration,
        ).await?;
        
        println!("Test ID: {}", test_id.to_string().cyan());
        
        // Monitor test progress
        monitor_single_test(service, test_id).await?;
        
        if !continuous {
            break;
        }
        
        println!("\nWaiting 5 seconds before next test...");
        sleep(Duration::from_secs(5)).await;
    }
    
    Ok(())
}

async fn run_cross_port_test(
    service: &InterfaceTestingService,
    source_span: u32,
    dest_span: u32,
    mapping: Option<String>,
    pattern: &str,
    duration: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "🔗 Cross-Port Wiring Test".bold().blue());
    println!("Source Span: {} → Destination Span: {}", 
             source_span.to_string().yellow(), 
             dest_span.to_string().yellow());
    
    if let Some(ref map_str) = mapping {
        println!("Channel Mapping: {}", map_str.yellow());
    }
    
    println!("Pattern: {}", pattern.yellow());
    println!("Duration: {} seconds", duration.to_string().yellow());
    
    let channel_mapping = if let Some(map_str) = mapping {
        Some(parse_channel_mapping(&map_str)?)
    } else {
        None
    };
    
    let test_pattern = parse_pattern(pattern)?;
    let test_duration = Duration::from_secs(duration);
    
    println!("\n{}", "Starting cross-port test...".green());
    
    let test_id = service.start_cross_port_test(
        source_span,
        dest_span,
        channel_mapping,
        test_pattern,
        test_duration,
    ).await?;
    
    println!("Test ID: {}", test_id.to_string().cyan());
    
    // Monitor test progress
    monitor_single_test(service, test_id).await?;
    
    Ok(())
}

async fn run_call_test(
    service: &InterfaceTestingService,
    calling_span: u32,
    called_span: u32,
    frequency: f64,
    duration: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "📞 End-to-End Call Test".bold().blue());
    println!("Calling Span: {} → Called Span: {}", 
             calling_span.to_string().yellow(), 
             called_span.to_string().yellow());
    println!("Test Tone: {} Hz", frequency.to_string().yellow());
    println!("Duration: {} seconds", duration.to_string().yellow());
    
    let test_duration = Duration::from_secs(duration);
    
    println!("\n{}", "Starting call test...".green());
    
    let test_id = service.start_end_to_end_test(
        calling_span,
        called_span,
        test_duration,
    ).await?;
    
    println!("Test ID: {}", test_id.to_string().cyan());
    
    // Monitor test progress
    monitor_single_test(service, test_id).await?;
    
    Ok(())
}

async fn run_test_suite(
    service: &InterfaceTestingService,
    spans: &str,
    loopback: bool,
    cross_port: bool,
    call_tests: bool,
    duration: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "🧪 Comprehensive Test Suite".bold().blue());
    
    let span_list: Result<Vec<u32>, _> = spans.split(',')
        .map(|s| s.trim().parse())
        .collect();
    let span_list = span_list?;
    
    println!("Spans: {:?}", span_list);
    println!("Test Duration: {} seconds each", duration);
    
    let test_duration = Duration::from_secs(duration);
    let mut all_tests = Vec::new();
    
    // Run loopback tests
    if loopback {
        println!("\n{}", "🔄 Running Loopback Tests".bold().green());
        for &span in &span_list {
            println!("Starting loopback test for span {}", span);
            let test_id = service.start_tdmoe_loopback_test(
                span,
                None,
                TestPattern::Prbs15,
                test_duration,
            ).await?;
            all_tests.push(test_id);
            println!("Test ID: {}", test_id.to_string().cyan());
        }
    }
    
    // Run cross-port tests
    if cross_port && span_list.len() >= 2 {
        println!("\n{}", "🔗 Running Cross-Port Tests".bold().green());
        for i in 0..span_list.len() {
            for j in (i + 1)..span_list.len() {
                let source_span = span_list[i];
                let dest_span = span_list[j];
                println!("Starting cross-port test: span {} → span {}", source_span, dest_span);
                
                let test_id = service.start_cross_port_test(
                    source_span,
                    dest_span,
                    None,
                    TestPattern::Alternating,
                    test_duration,
                ).await?;
                all_tests.push(test_id);
                println!("Test ID: {}", test_id.to_string().cyan());
            }
        }
    }
    
    // Run call tests
    if call_tests && span_list.len() >= 2 {
        println!("\n{}", "📞 Running Call Tests".bold().green());
        for i in 0..span_list.len() {
            for j in (i + 1)..span_list.len() {
                let calling_span = span_list[i];
                let called_span = span_list[j];
                println!("Starting call test: span {} → span {}", calling_span, called_span);
                
                let test_id = service.start_end_to_end_test(
                    calling_span,
                    called_span,
                    test_duration,
                ).await?;
                all_tests.push(test_id);
                println!("Test ID: {}", test_id.to_string().cyan());
            }
        }
    }
    
    // Monitor all tests
    println!("\n{}", "📊 Monitoring All Tests".bold().yellow());
    monitor_multiple_tests(service, all_tests).await?;
    
    Ok(())
}

async fn monitor_tests(
    service: &InterfaceTestingService,
    test_id: Option<String>,
    _interval: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(id_str) = test_id {
        let test_id = Uuid::parse_str(&id_str)?;
        monitor_single_test(service, test_id).await?;
    } else {
        // Monitor all active tests
        let active_tests = service.get_active_tests().await;
        if active_tests.is_empty() {
            println!("{}", "No active tests".yellow());
            return Ok(());
        }
        
        monitor_multiple_tests(service, active_tests).await?;
    }
    
    Ok(())
}

async fn monitor_single_test(
    service: &InterfaceTestingService,
    test_id: Uuid,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    
    loop {
        interval.tick().await;
        
        if let Some(stats) = service.get_test_status(test_id).await {
            // Clear screen and move cursor to top
            print!("\x1B[2J\x1B[1;1H");
            
            println!("{}", "📊 Test Progress".bold().green());
            println!("Test ID: {}", test_id.to_string().cyan());
            println!("Elapsed: {} seconds", stats.elapsed_time.as_secs());
            println!();
            
            println!("{}", "Frame Statistics:".bold());
            println!("  Sent:      {}", format_number(stats.frames_sent));
            println!("  Received:  {}", format_number(stats.frames_received));
            println!("  Lost:      {} ({:.2}%)", 
                     format_number(stats.frames_lost),
                     stats.frame_error_rate * 100.0);
            println!("  Corrupted: {}", format_number(stats.frames_corrupted));
            println!();
            
            println!("{}", "Timing Statistics:".bold());
            println!("  Min Delay:  {:?}", stats.min_delay);
            println!("  Max Delay:  {:?}", stats.max_delay);
            println!("  Avg Delay:  {:?}", stats.avg_delay);
            println!("  Jitter:     {:?}", stats.jitter);
            println!();
            
            println!("{}", "Quality Metrics:".bold());
            println!("  BER:        {:.2e}", stats.bit_error_rate);
            println!("  FER:        {:.2e}", stats.frame_error_rate);
            println!("  Throughput: {:.2} Mbps", stats.current_throughput);
            println!("  Sync Losses: {}", stats.sync_losses);
            println!();
            
            // Progress bar
            let progress = if stats.elapsed_time.as_secs() > 0 {
                (stats.elapsed_time.as_secs_f64() / 60.0).min(1.0) // Assume 60s test
            } else {
                0.0
            };
            
            print!("Progress: [");
            let filled = (progress * 40.0) as usize;
            for i in 0..40 {
                if i < filled {
                    print!("{}", "█".green());
                } else {
                    print!("{}", "░".dimmed());
                }
            }
            println!("] {:.1}%", progress * 100.0);
            
            println!("\nPress Ctrl+C to stop monitoring");
        } else {
            // Test completed or not found
            if let Some(result) = service.get_test_result(test_id).await {
                println!("\n{}", "✅ Test Completed".bold().green());
                display_test_result(&result);
                break;
            } else {
                println!("{}", "Test not found or completed".red());
                break;
            }
        }
    }
    
    Ok(())
}

async fn monitor_multiple_tests(
    service: &InterfaceTestingService,
    test_ids: Vec<Uuid>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    let mut completed_tests = Vec::new();
    
    loop {
        interval.tick().await;
        
        // Clear screen and move cursor to top
        print!("\x1B[2J\x1B[1;1H");
        
        println!("{}", "📊 Multiple Test Monitor".bold().green());
        println!("Monitoring {} tests", test_ids.len());
        println!("{}", "─".repeat(80));
        
        let mut active_count = 0;
        
        for &test_id in &test_ids {
            if completed_tests.contains(&test_id) {
                continue;
            }
            
            if let Some(stats) = service.get_test_status(test_id).await {
                active_count += 1;
                
                let short_id = &test_id.to_string()[..8];
                let progress = (stats.elapsed_time.as_secs_f64() / 60.0).min(1.0);
                let status = if progress >= 1.0 { "COMPLETING" } else { "RUNNING" };
                
                println!("{} | {} | {:.1}% | {} fps | {:.2}% loss",
                         short_id.cyan(),
                         status.yellow(),
                         progress * 100.0,
                         stats.frames_sent / stats.elapsed_time.as_secs().max(1),
                         stats.frame_error_rate * 100.0);
            } else if service.get_test_result(test_id).await.is_some() {
                completed_tests.push(test_id);
                let short_id = &test_id.to_string()[..8];
                println!("{} | {} | Complete", short_id.cyan(), "DONE".green());
            }
        }
        
        if active_count == 0 {
            println!("\n{}", "All tests completed!".bold().green());
            break;
        }
        
        println!("\n{} active, {} completed", active_count, completed_tests.len());
        println!("Press Ctrl+C to stop monitoring");
    }
    
    Ok(())
}

async fn show_results(
    service: &InterfaceTestingService,
    test_id: Option<String>,
    detailed: bool,
    _export: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(id_str) = test_id {
        let test_id = Uuid::parse_str(&id_str)?;
        if let Some(result) = service.get_test_result(test_id).await {
            display_test_result(&result);
            
            if detailed {
                println!("\n{}", "📋 Detailed Measurements".bold().blue());
                for (i, measurement) in result.raw_measurements.iter().take(20).enumerate() {
                    println!("Frame {}: {:?} -> {:?} | RTT: {:?} | Errors: {} | Quality: {:.1}%",
                             measurement.sequence_number,
                             measurement.send_time.format("%H:%M:%S%.3f"),
                             measurement.receive_time.map(|t| t.format("%H:%M:%S%.3f").to_string())
                                 .unwrap_or_else(|| "LOST".to_string()),
                             measurement.round_trip_delay.unwrap_or_default(),
                             measurement.error_bits,
                             measurement.signal_quality);
                    
                    if i >= 19 && result.raw_measurements.len() > 20 {
                        println!("... and {} more measurements", result.raw_measurements.len() - 20);
                        break;
                    }
                }
            }
        } else {
            println!("{}", "Test result not found".red());
        }
    } else {
        // Show all results
        let results = service.get_all_results().await;
        if results.is_empty() {
            println!("{}", "No test results available".yellow());
            return Ok(());
        }
        
        println!("{}", "📋 All Test Results".bold().blue());
        println!("{}", "─".repeat(100));
        
        for result in results {
            let short_id = &result.config.test_id.to_string()[..8];
            let status = if result.success { "PASS".green() } else { "FAIL".red() };
            let test_type = format!("{:?}", result.config.test_type);
            
            println!("{} | {} | {} | {:.2}% success | {} frames",
                     short_id.cyan(),
                     status,
                     test_type.yellow(),
                     (result.stats.frames_received as f64 / result.stats.frames_sent as f64 * 100.0),
                     format_number(result.stats.frames_sent));
        }
    }
    
    Ok(())
}

async fn stop_test(
    service: &InterfaceTestingService,
    test_id_str: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let test_id = Uuid::parse_str(test_id_str)?;
    service.stop_test(test_id).await?;
    println!("{} Test {} stopped", "✅".green(), test_id_str.cyan());
    Ok(())
}

async fn list_tests(
    service: &InterfaceTestingService,
    active_only: bool,
    completed_only: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !completed_only {
        let active_tests = service.get_active_tests().await;
        if !active_tests.is_empty() {
            println!("{}", "🔄 Active Tests".bold().green());
            println!("{}", "─".repeat(50));
            for test_id in active_tests {
                if let Some(stats) = service.get_test_status(test_id).await {
                    let short_id = &test_id.to_string()[..8];
                    println!("{} | {} seconds | {} fps",
                             short_id.cyan(),
                             stats.elapsed_time.as_secs(),
                             stats.frames_sent / stats.elapsed_time.as_secs().max(1));
                }
            }
        } else {
            println!("{}", "No active tests".yellow());
        }
    }
    
    if !active_only {
        let results = service.get_all_results().await;
        if !results.is_empty() {
            println!("\n{}", "✅ Completed Tests".bold().blue());
            println!("{}", "─".repeat(70));
            for result in results {
                let short_id = &result.config.test_id.to_string()[..8];
                let status = if result.success { "PASS".green() } else { "FAIL".red() };
                let test_type = format!("{:?}", result.config.test_type);
                
                println!("{} | {} | {} | {} | {:.1}% success",
                         short_id.cyan(),
                         status,
                         test_type.yellow(),
                         result.completion_time.format("%H:%M:%S"),
                         (result.stats.frames_received as f64 / result.stats.frames_sent as f64 * 100.0));
            }
        } else {
            println!("{}", "No completed tests".yellow());
        }
    }
    
    Ok(())
}

// Helper functions

fn parse_pattern(pattern: &str) -> Result<TestPattern, Box<dyn std::error::Error>> {
    match pattern.to_lowercase().as_str() {
        "prbs15" => Ok(TestPattern::Prbs15),
        "prbs23" => Ok(TestPattern::Prbs23),
        "prbs31" => Ok(TestPattern::Prbs31),
        "zeros" | "all_zeros" => Ok(TestPattern::AllZeros),
        "ones" | "all_ones" => Ok(TestPattern::AllOnes),
        "alternating" | "alt" => Ok(TestPattern::Alternating),
        "q931_setup" => Ok(TestPattern::Q931Setup),
        "q931_release" => Ok(TestPattern::Q931Release),
        "lapd" => Ok(TestPattern::LapdFrames),
        _ => {
            if let Ok(freq) = pattern.parse::<f64>() {
                Ok(TestPattern::ToneGeneration(freq))
            } else {
                Err(format!("Unknown pattern: {}", pattern).into())
            }
        }
    }
}

fn parse_channel_mapping(mapping: &str) -> Result<HashMap<u8, u8>, Box<dyn std::error::Error>> {
    let mut map = HashMap::new();
    for pair in mapping.split(',') {
        let parts: Vec<&str> = pair.split(':').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid mapping format: {}", pair).into());
        }
        let source: u8 = parts[0].trim().parse()?;
        let dest: u8 = parts[1].trim().parse()?;
        map.insert(source, dest);
    }
    Ok(map)
}

fn format_number(num: u64) -> String {
    if num >= 1_000_000 {
        format!("{:.1}M", num as f64 / 1_000_000.0)
    } else if num >= 1_000 {
        format!("{:.1}K", num as f64 / 1_000.0)
    } else {
        num.to_string()
    }
}

fn display_test_result(result: &redfire_gateway::services::interface_testing::InterfaceTestResult) {
    let status = if result.success { 
        "PASSED".green() 
    } else { 
        "FAILED".red() 
    };
    
    println!("\n{}", "📋 Test Results".bold().blue());
    println!("{}", "─".repeat(50));
    println!("Test ID: {}", result.config.test_id.to_string().cyan());
    println!("Type: {:?}", result.config.test_type);
    println!("Status: {}", status);
    println!("Completion: {}", result.completion_time.format("%Y-%m-%d %H:%M:%S UTC"));
    
    println!("\n{}", "Statistics:".bold());
    println!("  Duration:     {} seconds", result.stats.elapsed_time.as_secs());
    println!("  Frames Sent:  {}", format_number(result.stats.frames_sent));
    println!("  Frames Rcvd:  {}", format_number(result.stats.frames_received));
    println!("  Frame Loss:   {:.3}%", result.stats.frame_error_rate * 100.0);
    println!("  Bit Errors:   {:.2e}", result.stats.bit_error_rate);
    println!("  Avg Delay:    {:?}", result.stats.avg_delay);
    println!("  Jitter:       {:?}", result.stats.jitter);
    println!("  Throughput:   {:.2} Mbps", result.stats.current_throughput);
    
    if !result.error_analysis.is_empty() {
        println!("\n{}", "Error Analysis:".bold().red());
        for error in &result.error_analysis {
            println!("  • {}", error);
        }
    }
    
    if !result.recommendations.is_empty() {
        println!("\n{}", "Recommendations:".bold().yellow());
        for rec in &result.recommendations {
            println!("  • {}", rec);
        }
    }
}