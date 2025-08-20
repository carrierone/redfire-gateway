//! Redfire Gateway main application

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tokio::signal;
use tracing::{error, info};

use redfire_gateway::{
    config::GatewayConfig,
    core::RedFireGateway,
    utils::setup_logging,
    Result,
};

#[derive(Parser)]
#[command(name = "redfire-gateway")]
#[command(about = "TDMoE to SIP Gateway")]
#[command(version = redfire_gateway::VERSION)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Configuration file path
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Run as daemon
    #[arg(short, long)]
    daemon: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the gateway
    Start,
    /// Stop the gateway
    Stop,
    /// Check gateway status
    Status,
    /// Validate configuration
    ValidateConfig,
    /// Generate default configuration
    GenerateConfig {
        /// Output file path
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let config = load_configuration(&cli).await?;
    
    // Setup logging
    setup_logging(&config.logging)?;

    info!("Starting {} v{}", redfire_gateway::NAME, redfire_gateway::VERSION);
    info!("Description: {}", redfire_gateway::DESCRIPTION);

    // Handle commands
    match &cli.command {
        Some(Commands::Start) | None => {
            run_gateway(config, cli.daemon).await
        }
        Some(Commands::Stop) => {
            stop_gateway().await
        }
        Some(Commands::Status) => {
            show_status().await
        }
        Some(Commands::ValidateConfig) => {
            validate_configuration(&config).await
        }
        Some(Commands::GenerateConfig { output }) => {
            generate_default_config(output.clone()).await
        }
    }
}

async fn load_configuration(cli: &Cli) -> Result<GatewayConfig> {
    let config = if let Some(config_path) = &cli.config {
        info!("Loading configuration from: {}", config_path.display());
        GatewayConfig::load_from_file(config_path)?
    } else {
        info!("No configuration file specified, trying environment variables");
        match GatewayConfig::load_from_env() {
            Ok(config) => config,
            Err(_) => {
                info!("No environment configuration found, using defaults");
                GatewayConfig::default_config()
            }
        }
    };

    // Validate configuration
    config.validate()?;
    info!("Configuration loaded and validated successfully");

    Ok(config)
}

async fn run_gateway(config: GatewayConfig, daemon: bool) -> Result<()> {
    info!("Initializing Redfire Gateway");

    // Create and start gateway
    let mut gateway = RedFireGateway::new(config)?;
    
    // Take the event receiver before starting
    let mut event_rx = gateway.take_event_receiver()
        .ok_or_else(|| redfire_gateway::Error::internal("Failed to get event receiver"))?;

    // Start the gateway
    gateway.start().await?;

    // Handle daemon mode
    if daemon {
        info!("Running in daemon mode");
        // In a real implementation, this would properly daemonize the process
    }

    // Set up signal handlers
    let gateway = Arc::new(tokio::sync::Mutex::new(gateway));
    let gateway_shutdown = Arc::clone(&gateway);

    // Handle events
    let event_task = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            handle_gateway_event(event).await;
        }
    });

    // Handle shutdown signals
    let shutdown_task = tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("Received Ctrl+C, shutting down gracefully");
                let mut gateway = gateway_shutdown.lock().await;
                if let Err(e) = gateway.stop().await {
                    error!("Error during shutdown: {}", e);
                }
            }
            Err(err) => {
                error!("Unable to listen for shutdown signal: {}", err);
            }
        }
    });

    // Wait for shutdown
    tokio::select! {
        _ = event_task => {
            info!("Event handling completed");
        }
        _ = shutdown_task => {
            info!("Shutdown signal received");
        }
    }

    // Final cleanup
    let mut gateway = gateway.lock().await;
    if gateway.is_running().await {
        gateway.stop().await?;
    }

    info!("Redfire Gateway shutdown complete");
    Ok(())
}

async fn handle_gateway_event(event: redfire_gateway::core::gateway::GatewayEvent) {
    use redfire_gateway::core::gateway::GatewayEvent;

    match event {
        GatewayEvent::Started => {
            info!("✓ Gateway started successfully");
        }
        GatewayEvent::Stopped => {
            info!("✓ Gateway stopped");
        }
        GatewayEvent::InterfaceUp { interface } => {
            info!("✓ Interface {} is UP", interface);
        }
        GatewayEvent::InterfaceDown { interface } => {
            error!("✗ Interface {} is DOWN", interface);
        }
        GatewayEvent::CallStarted { call_id } => {
            info!("📞 Call started: {}", call_id);
        }
        GatewayEvent::CallEnded { call_id } => {
            info!("📞 Call ended: {}", call_id);
        }
        GatewayEvent::Error { message } => {
            error!("✗ Gateway error: {}", message);
        }
    }
}

async fn stop_gateway() -> Result<()> {
    // In a real implementation, this would connect to a running instance
    // and send a shutdown signal (e.g., via Unix socket or signal)
    println!("Stop command not implemented (send SIGTERM to running process)");
    Ok(())
}

async fn show_status() -> Result<()> {
    // In a real implementation, this would connect to a running instance
    // and query its status
    println!("Status command not implemented");
    Ok(())
}

async fn validate_configuration(config: &GatewayConfig) -> Result<()> {
    info!("Validating configuration...");
    
    config.validate()?;
    
    println!("✓ Configuration is valid");
    println!("  Node ID: {}", config.general.node_id);
    println!("  SIP Port: {}", config.sip.listen_port);
    println!("  RTP Port Range: {}-{}", config.rtp.port_range.min, config.rtp.port_range.max);
    println!("  TDMoE Channels: {}", config.tdmoe.channels);
    println!("  FreeTDM Enabled: {}", config.freetdm.enabled);
    println!("  Performance Monitoring: {}", config.performance.enabled);
    println!("  SNMP Enabled: {}", config.snmp.enabled);
    
    Ok(())
}

async fn generate_default_config(output_path: Option<PathBuf>) -> Result<()> {
    let config = GatewayConfig::default_config();
    let toml_content = toml::to_string_pretty(&config)
        .map_err(|e| redfire_gateway::Error::internal(format!("Failed to serialize config: {}", e)))?;
    
    match output_path {
        Some(path) => {
            std::fs::write(&path, toml_content)?;
            println!("✓ Default configuration written to: {}", path.display());
        }
        None => {
            println!("{}", toml_content);
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_config_generation() {
        let result = generate_default_config(None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_config_validation() {
        let config = GatewayConfig::default_config();
        let result = validate_configuration(&config).await;
        assert!(result.is_ok());
    }
}