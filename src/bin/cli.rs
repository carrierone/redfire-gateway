//! Redfire Gateway CLI tool

use clap::{Parser, Subcommand};
use colored::*;

#[derive(Parser)]
#[command(name = "redfire-cli")]
#[command(about = "Redfire Gateway CLI Management Tool")]
#[command(version = redfire_gateway::VERSION)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// Gateway host to connect to
    #[arg(short, long, default_value = "localhost")]
    host: String,
    
    /// Management port
    #[arg(short, long, default_value = "8080")]
    port: u16,
}

#[derive(Subcommand)]
enum Commands {
    /// Show system status
    Status,
    /// Show interface information
    Interfaces,
    /// Show active calls
    Calls,
    /// Show performance metrics
    Performance,
    /// Start loopback test
    LoopbackStart {
        /// Channel to test
        channel: u16,
        /// Loopback type (local, remote, line)
        #[arg(default_value = "local")]
        loopback_type: String,
    },
    /// Stop loopback test
    LoopbackStop {
        /// Channel to stop testing
        channel: u16,
    },
    /// Start BERT test
    BertStart {
        /// Channel to test
        channel: u16,
        /// Test pattern
        #[arg(default_value = "prbs_23")]
        pattern: String,
        /// Test duration in seconds
        #[arg(default_value = "60")]
        duration: u32,
    },
    /// Stop BERT test
    BertStop {
        /// Channel to stop testing
        channel: u16,
    },
    /// Show BERT test results
    BertResults {
        /// Channel to show results for
        channel: u16,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Status => show_status(&cli).await,
        Commands::Interfaces => show_interfaces(&cli).await,
        Commands::Calls => show_calls(&cli).await,
        Commands::Performance => show_performance(&cli).await,
        Commands::LoopbackStart { channel, ref loopback_type } => {
            start_loopback(&cli, channel, loopback_type).await
        }
        Commands::LoopbackStop { channel } => stop_loopback(&cli, channel).await,
        Commands::BertStart { channel, ref pattern, duration } => {
            start_bert(&cli, channel, pattern, duration).await
        }
        Commands::BertStop { channel } => stop_bert(&cli, channel).await,
        Commands::BertResults { channel } => show_bert_results(&cli, channel).await,
    }
}

async fn show_status(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "Redfire Gateway Status".bold().blue());
    println!("{}://{}:{}", "http".dimmed(), cli.host, cli.port);
    println!();
    
    // In a real implementation, this would make HTTP requests to the gateway
    println!("{}: {}", "Status".bold(), "Running".green());
    println!("{}: {}", "Uptime".bold(), "2d 5h 23m");
    println!("{}: {}", "Version".bold(), redfire_gateway::VERSION);
    println!();
    
    println!("{}", "Interfaces:".bold());
    println!("  TDMoE:   {}", "UP".green());
    println!("  FreeTDM: {}", "DISABLED".yellow());
    println!();
    
    println!("{}", "Protocols:".bold());
    println!("  SIP:     {}", "RUNNING".green());
    println!("  RTP:     {}", "RUNNING".green());
    println!();
    
    println!("{}", "Sessions:".bold());
    println!("  Active Calls: {}", "3".bold());
    println!("  SIP Sessions: {}", "5".bold());
    println!("  RTP Sessions: {}", "3".bold());
    
    Ok(())
}

async fn show_interfaces(_cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "Interface Status".bold().blue());
    println!();
    
    // Mock interface data
    println!("{:<15} {:<10} {:<15} {:<10}", "Interface".bold(), "Status".bold(), "Type".bold(), "Channels".bold());
    println!("{}", "─".repeat(60));
    println!("{:<15} {:<10} {:<15} {:<10}", "TDMoE-1", "UP".green(), "E1", "30/30");
    println!("{:<15} {:<10} {:<15} {:<10}", "FreeTDM-1", "DOWN".red(), "T1", "0/24");
    
    Ok(())
}

async fn show_calls(_cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "Active Calls".bold().blue());
    println!();
    
    // Mock call data
    println!("{:<20} {:<15} {:<15} {:<10} {:<10}", "Call ID".bold(), "From".bold(), "To".bold(), "Duration".bold(), "Status".bold());
    println!("{}", "─".repeat(80));
    println!("{:<20} {:<15} {:<15} {:<10} {:<10}", "call-001", "+1234567890", "+0987654321", "00:02:45", "ACTIVE".green());
    println!("{:<20} {:<15} {:<15} {:<10} {:<10}", "call-002", "+5555551234", "+1111119876", "00:00:12", "RINGING".yellow());
    
    Ok(())
}

async fn show_performance(_cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "Performance Metrics".bold().blue());
    println!();
    
    // Mock performance data
    println!("{}", "System:".bold());
    println!("  CPU Usage:    {}%", "15".green());
    println!("  Memory Usage: {}%", "42".green());
    println!("  Load Average: {}", "0.85".green());
    println!();
    
    println!("{}", "Network:".bold());
    println!("  Packets/sec:  {}", "1,234");
    println!("  Errors/sec:   {}", "0".green());
    println!("  Bytes In:     {} MB", "145.2");
    println!("  Bytes Out:    {} MB", "134.8");
    
    Ok(())
}

async fn start_loopback(cli: &Cli, channel: u16, loopback_type: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting {} loopback test on channel {} ({}:{})", 
        loopback_type, channel, cli.host, cli.port);
    println!("{}", "Loopback test started successfully".green());
    Ok(())
}

async fn stop_loopback(cli: &Cli, channel: u16) -> Result<(), Box<dyn std::error::Error>> {
    println!("Stopping loopback test on channel {} ({}:{})", 
        channel, cli.host, cli.port);
    println!("{}", "Loopback test stopped".green());
    Ok(())
}

async fn start_bert(cli: &Cli, channel: u16, pattern: &str, duration: u32) -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting BERT test on channel {} ({}:{})", channel, cli.host, cli.port);
    println!("  Pattern:  {}", pattern);
    println!("  Duration: {} seconds", duration);
    println!("{}", "BERT test started successfully".green());
    Ok(())
}

async fn stop_bert(cli: &Cli, channel: u16) -> Result<(), Box<dyn std::error::Error>> {
    println!("Stopping BERT test on channel {} ({}:{})", channel, cli.host, cli.port);
    println!("{}", "BERT test stopped".green());
    Ok(())
}

async fn show_bert_results(cli: &Cli, channel: u16) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", format!("BERT Test Results - Channel {}", channel).bold().blue());
    println!("Gateway: {}:{}", cli.host, cli.port);
    println!();
    
    // Mock BERT results
    println!("{}", "Test Summary:".bold());
    println!("  Pattern:        {}", "PRBS-23");
    println!("  Duration:       {}", "60 seconds");
    println!("  Bits Transmitted: {}", "15,360,000");
    println!("  Bits Received: {}", "15,359,987");
    println!("  Error Bits:     {}", "13");
    println!("  Error Rate:     {} ({})", "8.46e-7".green(), "PASS".green());
    println!();
    
    println!("{}", "Detailed Results:".bold());
    println!("  Sync Time:      {} ms", "145");
    println!("  Signal Level:   {} dB", "-12.3");
    println!("  Jitter:         {} μs", "2.1");
    println!("  Pattern Sync:   {}", "LOCKED".green());
    
    Ok(())
}