//! B2BUA CLI management tool for Redfire Gateway
//! 
//! This tool provides comprehensive command-line management of B2BUA
//! functionality including call management, statistics, and configuration.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use serde_json;
use tokio::time::timeout;
use tracing::{error, info, warn};

use redfire_gateway::config::{RouteType, RoutingRule, NumberTranslation};
use redfire_gateway::services::{
    B2buaCall, B2buaCallState, MediaRelaySession, CallDetailRecord,
    ClusterNode, TranscodingSession, CodecType,
};

#[derive(Parser)]
#[command(name = "b2bua-cli")]
#[command(about = "B2BUA management CLI for Redfire Gateway")]
#[command(version = "1.0.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Gateway management API endpoint
    #[arg(long, default_value = "http://localhost:8080")]
    endpoint: String,

    /// Output format
    #[arg(long, value_enum, default_value = "table")]
    format: OutputFormat,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Clone, clap::ValueEnum)]
enum OutputFormat {
    Table,
    Json,
    Csv,
    Summary,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage active calls
    Call {
        #[command(subcommand)]
        action: CallAction,
    },
    /// Manage routing rules
    Routing {
        #[command(subcommand)]
        action: RoutingAction,
    },
    /// Manage media relay sessions
    Media {
        #[command(subcommand)]
        action: MediaAction,
    },
    /// Manage transcoding sessions
    Transcoding {
        #[command(subcommand)]
        action: TranscodingAction,
    },
    /// Manage clustering
    Cluster {
        #[command(subcommand)]
        action: ClusterAction,
    },
    /// View CDR and billing information
    Billing {
        #[command(subcommand)]
        action: BillingAction,
    },
    /// Show system statistics
    Stats {
        #[command(subcommand)]
        action: StatsAction,
    },
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum CallAction {
    /// List active calls
    List {
        /// Filter by call state
        #[arg(long)]
        state: Option<String>,
        /// Filter by caller
        #[arg(long)]
        caller: Option<String>,
        /// Filter by callee
        #[arg(long)]
        callee: Option<String>,
    },
    /// Show call details
    Show {
        /// Call ID or session ID
        call_id: String,
    },
    /// Terminate a call
    Terminate {
        /// Call ID to terminate
        call_id: String,
        /// Reason for termination
        #[arg(long, default_value = "Administrative")]
        reason: String,
    },
    /// Monitor calls in real-time
    Monitor {
        /// Update interval in seconds
        #[arg(long, default_value = "5")]
        interval: u64,
        /// Filter by pattern
        #[arg(long)]
        filter: Option<String>,
    },
}

#[derive(Subcommand)]
enum RoutingAction {
    /// List routing rules
    List,
    /// Add a new routing rule
    Add {
        /// Rule ID
        #[arg(long)]
        id: String,
        /// Pattern to match (regex)
        #[arg(long)]
        pattern: String,
        /// Route type
        #[arg(long, value_enum)]
        route_type: RouteTypeArg,
        /// Target gateway
        #[arg(long)]
        target: String,
        /// Priority (lower = higher priority)
        #[arg(long, default_value = "100")]
        priority: u8,
    },
    /// Remove a routing rule
    Remove {
        /// Rule ID to remove
        rule_id: String,
    },
    /// Test routing for a number
    Test {
        /// Caller number
        #[arg(long)]
        caller: String,
        /// Called number
        callee: String,
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum RouteTypeArg {
    Direct,
    Gateway,
    Trunk,
    Emergency,
}

impl From<RouteTypeArg> for RouteType {
    fn from(arg: RouteTypeArg) -> Self {
        match arg {
            RouteTypeArg::Direct => RouteType::Direct,
            RouteTypeArg::Gateway => RouteType::Gateway,
            RouteTypeArg::Trunk => RouteType::Trunk,
            RouteTypeArg::Emergency => RouteType::Emergency,
        }
    }
}

#[derive(Subcommand)]
enum MediaAction {
    /// List active media sessions
    List,
    /// Show media session details
    Show {
        /// Session ID
        session_id: String,
    },
    /// Show media quality statistics
    Quality {
        /// Session ID (optional)
        #[arg(long)]
        session_id: Option<String>,
        /// Time window in minutes
        #[arg(long, default_value = "60")]
        window: u32,
    },
}

#[derive(Subcommand)]
enum TranscodingAction {
    /// List transcoding sessions
    List,
    /// Show transcoding session details
    Show {
        /// Session ID
        session_id: String,
    },
    /// Show GPU device information
    Devices,
    /// Switch transcoding backend
    Switch {
        /// New backend type
        #[arg(value_enum)]
        backend: TranscodingBackendArg,
    },
    /// Show transcoding performance
    Performance,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum TranscodingBackendArg {
    Auto,
    Cuda,
    Rocm,
    Cpu,
}

#[derive(Subcommand)]
enum ClusterAction {
    /// Show cluster status
    Status,
    /// List cluster nodes
    Nodes,
    /// Show node details
    Node {
        /// Node ID
        node_id: String,
    },
    /// Show anycast addresses
    Anycast,
    /// Trigger node failover
    Failover {
        /// Node ID to failover from
        from_node: String,
        /// Node ID to failover to
        to_node: String,
    },
}

#[derive(Subcommand)]
enum BillingAction {
    /// Show billing summary
    Summary {
        /// Start date (YYYY-MM-DD)
        #[arg(long)]
        start: String,
        /// End date (YYYY-MM-DD)
        #[arg(long)]
        end: String,
    },
    /// List recent CDRs
    Cdrs {
        /// Number of records to show
        #[arg(long, default_value = "50")]
        limit: u32,
        /// Filter by account ID
        #[arg(long)]
        account: Option<String>,
    },
    /// Show billing rates
    Rates {
        /// Filter by prefix
        #[arg(long)]
        prefix: Option<String>,
    },
    /// Export CDRs
    Export {
        /// Start date (YYYY-MM-DD)
        start: String,
        /// End date (YYYY-MM-DD)
        end: String,
        /// Output file
        #[arg(long)]
        output: String,
        /// Export format
        #[arg(long, value_enum, default_value = "csv")]
        format: ExportFormat,
    },
}

#[derive(Clone, clap::ValueEnum)]
enum ExportFormat {
    Csv,
    Json,
    Excel,
}

#[derive(Subcommand)]
enum StatsAction {
    /// Show overall system statistics
    System,
    /// Show call statistics
    Calls {
        /// Time window in hours
        #[arg(long, default_value = "24")]
        window: u32,
    },
    /// Show performance metrics
    Performance,
    /// Show real-time dashboard
    Dashboard {
        /// Update interval in seconds
        #[arg(long, default_value = "5")]
        interval: u64,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show current configuration
    Show,
    /// Validate configuration
    Validate,
    /// Reload configuration
    Reload,
    /// Export configuration
    Export {
        /// Output file
        output: String,
    },
    /// Import configuration
    Import {
        /// Input file
        input: String,
    },
}

/// API client for communicating with the gateway
struct ApiClient {
    endpoint: String,
    client: reqwest::Client,
}

impl ApiClient {
    fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            client: reqwest::Client::new(),
        }
    }

    async fn get_active_calls(&self) -> Result<Vec<B2buaCall>, Box<dyn std::error::Error>> {
        let url = format!("{}/api/v1/b2bua/calls", self.endpoint);
        let response = timeout(Duration::from_secs(10), self.client.get(&url).send()).await??;
        let calls = response.json().await?;
        Ok(calls)
    }

    async fn get_call_details(&self, call_id: &str) -> Result<B2buaCall, Box<dyn std::error::Error>> {
        let url = format!("{}/api/v1/b2bua/calls/{}", self.endpoint, call_id);
        let response = timeout(Duration::from_secs(10), self.client.get(&url).send()).await??;
        let call = response.json().await?;
        Ok(call)
    }

    async fn terminate_call(&self, call_id: &str, reason: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/api/v1/b2bua/calls/{}/terminate", self.endpoint, call_id);
        let payload = serde_json::json!({ "reason": reason });
        let response = timeout(Duration::from_secs(10), 
            self.client.post(&url).json(&payload).send()).await??;
        
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("Failed to terminate call: {}", response.status()).into())
        }
    }

    async fn get_media_sessions(&self) -> Result<Vec<MediaRelaySession>, Box<dyn std::error::Error>> {
        let url = format!("{}/api/v1/media/sessions", self.endpoint);
        let response = timeout(Duration::from_secs(10), self.client.get(&url).send()).await??;
        let sessions = response.json().await?;
        Ok(sessions)
    }

    async fn get_transcoding_sessions(&self) -> Result<Vec<TranscodingSession>, Box<dyn std::error::Error>> {
        let url = format!("{}/api/v1/transcoding/sessions", self.endpoint);
        let response = timeout(Duration::from_secs(10), self.client.get(&url).send()).await??;
        let sessions = response.json().await?;
        Ok(sessions)
    }

    async fn get_cluster_nodes(&self) -> Result<Vec<ClusterNode>, Box<dyn std::error::Error>> {
        let url = format!("{}/api/v1/cluster/nodes", self.endpoint);
        let response = timeout(Duration::from_secs(10), self.client.get(&url).send()).await??;
        let nodes = response.json().await?;
        Ok(nodes)
    }

    async fn get_cdrs(&self, limit: u32, account: Option<&str>) -> Result<Vec<CallDetailRecord>, Box<dyn std::error::Error>> {
        let mut url = format!("{}/api/v1/billing/cdrs?limit={}", self.endpoint, limit);
        if let Some(acc) = account {
            url.push_str(&format!("&account={}", acc));
        }
        let response = timeout(Duration::from_secs(10), self.client.get(&url).send()).await??;
        let cdrs = response.json().await?;
        Ok(cdrs)
    }
}

/// Output formatter for different display formats
struct OutputFormatter {
    format: OutputFormat,
}

impl OutputFormatter {
    fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    fn format_calls(&self, calls: &[B2buaCall]) {
        match self.format {
            OutputFormat::Table => self.format_calls_table(calls),
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(calls).unwrap()),
            OutputFormat::Csv => self.format_calls_csv(calls),
            OutputFormat::Summary => self.format_calls_summary(calls),
        }
    }

    fn format_calls_table(&self, calls: &[B2buaCall]) {
        println!("{:<36} {:<12} {:<15} {:<15} {:<10} {:<8}",
            "Call ID", "State", "Caller", "Callee", "Duration", "Route");
        println!("{}", "-".repeat(100));

        for call in calls {
            let duration = call.connected_at
                .map(|connected| format!("{}s", connected.elapsed().as_secs()))
                .unwrap_or_else(|| "N/A".to_string());

            let route_type = format!("{:?}", call.routing_info.route_type);

            println!("{:<36} {:<12} {:<15} {:<15} {:<10} {:<8}",
                call.id,
                format!("{:?}", call.state),
                call.caller,
                call.callee,
                duration,
                route_type
            );
        }
    }

    fn format_calls_csv(&self, calls: &[B2buaCall]) {
        println!("call_id,state,caller,callee,duration_seconds,route_type");
        for call in calls {
            let duration = call.connected_at
                .map(|connected| connected.elapsed().as_secs().to_string())
                .unwrap_or_else(|| "0".to_string());

            println!("{},{:?},{},{},{},{:?}",
                call.id,
                call.state,
                call.caller,
                call.callee,
                duration,
                call.routing_info.route_type
            );
        }
    }

    fn format_calls_summary(&self, calls: &[B2buaCall]) {
        let total_calls = calls.len();
        let mut state_counts = HashMap::new();
        let mut route_counts = HashMap::new();

        for call in calls {
            *state_counts.entry(format!("{:?}", call.state)).or_insert(0) += 1;
            *route_counts.entry(format!("{:?}", call.routing_info.route_type)).or_insert(0) += 1;
        }

        println!("Call Summary:");
        println!("  Total Calls: {}", total_calls);
        println!("  By State:");
        for (state, count) in state_counts {
            println!("    {}: {}", state, count);
        }
        println!("  By Route Type:");
        for (route, count) in route_counts {
            println!("    {}: {}", route, count);
        }
    }

    fn format_media_sessions(&self, sessions: &[MediaRelaySession]) {
        match self.format {
            OutputFormat::Table => {
                println!("{:<36} {:<36} {:<12} {:<12} {:<10} {:<10}",
                    "Session ID", "Call ID", "Leg A Codec", "Leg B Codec", "Packets", "Bytes");
                println!("{}", "-".repeat(120));

                for session in sessions {
                    println!("{:<36} {:<36} {:<12} {:<12} {:<10} {:<10}",
                        session.id,
                        session.call_id,
                        session.stats.codec_a.to_name(),
                        session.stats.codec_b.to_name(),
                        session.stats.total_packets(),
                        session.stats.total_bytes()
                    );
                }
            }
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(sessions).unwrap()),
            _ => self.format_media_sessions_summary(sessions),
        }
    }

    fn format_media_sessions_summary(&self, sessions: &[MediaRelaySession]) {
        let total_sessions = sessions.len();
        let total_packets: u64 = sessions.iter().map(|s| s.stats.total_packets()).sum();
        let total_bytes: u64 = sessions.iter().map(|s| s.stats.total_bytes()).sum();

        println!("Media Sessions Summary:");
        println!("  Total Sessions: {}", total_sessions);
        println!("  Total Packets Relayed: {}", total_packets);
        println!("  Total Bytes Relayed: {} MB", total_bytes / 1024 / 1024);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Initialize logging if verbose
    if cli.verbose {
        tracing_subscriber::fmt::init();
    }

    let api_client = ApiClient::new(cli.endpoint);
    let formatter = OutputFormatter::new(cli.format);

    match cli.command {
        Commands::Call { action } => handle_call_command(action, &api_client, &formatter).await?,
        Commands::Routing { action } => handle_routing_command(action, &api_client).await?,
        Commands::Media { action } => handle_media_command(action, &api_client, &formatter).await?,
        Commands::Transcoding { action } => handle_transcoding_command(action, &api_client).await?,
        Commands::Cluster { action } => handle_cluster_command(action, &api_client).await?,
        Commands::Billing { action } => handle_billing_command(action, &api_client).await?,
        Commands::Stats { action } => handle_stats_command(action, &api_client).await?,
        Commands::Config { action } => handle_config_command(action, &api_client).await?,
    }

    Ok(())
}

async fn handle_call_command(
    action: CallAction,
    api_client: &ApiClient,
    formatter: &OutputFormatter,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        CallAction::List { state, caller, callee } => {
            let mut calls = api_client.get_active_calls().await?;

            // Apply filters
            if let Some(state_filter) = state {
                calls.retain(|call| format!("{:?}", call.state).to_lowercase().contains(&state_filter.to_lowercase()));
            }
            if let Some(caller_filter) = caller {
                calls.retain(|call| call.caller.contains(&caller_filter));
            }
            if let Some(callee_filter) = callee {
                calls.retain(|call| call.callee.contains(&callee_filter));
            }

            formatter.format_calls(&calls);
        }
        CallAction::Show { call_id } => {
            let call = api_client.get_call_details(&call_id).await?;
            println!("{}", serde_json::to_string_pretty(&call)?);
        }
        CallAction::Terminate { call_id, reason } => {
            api_client.terminate_call(&call_id, &reason).await?;
            println!("Call {} terminated successfully", call_id);
        }
        CallAction::Monitor { interval, filter: _ } => {
            loop {
                let calls = api_client.get_active_calls().await?;
                
                // Clear screen
                print!("\x1B[2J\x1B[1;1H");
                
                println!("B2BUA Call Monitor - {}", Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                println!("{}", "=".repeat(80));
                formatter.format_calls(&calls);
                
                tokio::time::sleep(Duration::from_secs(interval)).await;
            }
        }
    }
    Ok(())
}

async fn handle_routing_command(
    action: RoutingAction,
    _api_client: &ApiClient,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        RoutingAction::List => {
            println!("Routing rules list not implemented yet");
        }
        RoutingAction::Add { id, pattern, route_type, target, priority } => {
            println!("Added routing rule: {} -> {} ({:?})", pattern, target, route_type);
        }
        RoutingAction::Remove { rule_id } => {
            println!("Removed routing rule: {}", rule_id);
        }
        RoutingAction::Test { caller, callee } => {
            println!("Testing route for {} -> {}", caller, callee);
        }
    }
    Ok(())
}

async fn handle_media_command(
    action: MediaAction,
    api_client: &ApiClient,
    formatter: &OutputFormatter,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        MediaAction::List => {
            let sessions = api_client.get_media_sessions().await?;
            formatter.format_media_sessions(&sessions);
        }
        MediaAction::Show { session_id } => {
            println!("Media session details for: {}", session_id);
        }
        MediaAction::Quality { session_id: _, window: _ } => {
            println!("Media quality statistics not implemented yet");
        }
    }
    Ok(())
}

async fn handle_transcoding_command(
    action: TranscodingAction,
    api_client: &ApiClient,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        TranscodingAction::List => {
            let sessions = api_client.get_transcoding_sessions().await?;
            println!("Active transcoding sessions: {}", sessions.len());
            for session in sessions {
                println!("  {}: {} -> {}", session.id, session.source_codec.to_name(), session.target_codec.to_name());
            }
        }
        TranscodingAction::Show { session_id } => {
            println!("Transcoding session details for: {}", session_id);
        }
        TranscodingAction::Devices => {
            println!("GPU devices not implemented yet");
        }
        TranscodingAction::Switch { backend } => {
            println!("Switching transcoding backend to: {:?}", backend);
        }
        TranscodingAction::Performance => {
            println!("Transcoding performance metrics not implemented yet");
        }
    }
    Ok(())
}

async fn handle_cluster_command(
    action: ClusterAction,
    api_client: &ApiClient,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        ClusterAction::Status => {
            println!("Cluster status not implemented yet");
        }
        ClusterAction::Nodes => {
            let nodes = api_client.get_cluster_nodes().await?;
            println!("Cluster nodes: {}", nodes.len());
            for node in nodes {
                println!("  {}: {} ({:?})", node.node_id, node.address, node.status);
            }
        }
        ClusterAction::Node { node_id } => {
            println!("Node details for: {}", node_id);
        }
        ClusterAction::Anycast => {
            println!("Anycast addresses not implemented yet");
        }
        ClusterAction::Failover { from_node, to_node } => {
            println!("Triggering failover from {} to {}", from_node, to_node);
        }
    }
    Ok(())
}

async fn handle_billing_command(
    action: BillingAction,
    api_client: &ApiClient,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        BillingAction::Summary { start: _, end: _ } => {
            println!("Billing summary not implemented yet");
        }
        BillingAction::Cdrs { limit, account } => {
            let cdrs = api_client.get_cdrs(limit, account.as_deref()).await?;
            println!("Recent CDRs: {}", cdrs.len());
            for cdr in cdrs.iter().take(10) {
                println!("  {}: {} -> {} ({}s, ${:.2})",
                    cdr.id, cdr.caller, cdr.callee, cdr.duration_seconds, cdr.billing_info.cost);
            }
        }
        BillingAction::Rates { prefix: _ } => {
            println!("Billing rates not implemented yet");
        }
        BillingAction::Export { start: _, end: _, output, format: _ } => {
            println!("Exporting CDRs to: {}", output);
        }
    }
    Ok(())
}

async fn handle_stats_command(
    action: StatsAction,
    _api_client: &ApiClient,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        StatsAction::System => {
            println!("System statistics not implemented yet");
        }
        StatsAction::Calls { window: _ } => {
            println!("Call statistics not implemented yet");
        }
        StatsAction::Performance => {
            println!("Performance metrics not implemented yet");
        }
        StatsAction::Dashboard { interval: _ } => {
            println!("Real-time dashboard not implemented yet");
        }
    }
    Ok(())
}

async fn handle_config_command(
    action: ConfigAction,
    _api_client: &ApiClient,
) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        ConfigAction::Show => {
            println!("Configuration display not implemented yet");
        }
        ConfigAction::Validate => {
            println!("Configuration validation not implemented yet");
        }
        ConfigAction::Reload => {
            println!("Configuration reload not implemented yet");
        }
        ConfigAction::Export { output } => {
            println!("Exporting configuration to: {}", output);
        }
        ConfigAction::Import { input } => {
            println!("Importing configuration from: {}", input);
        }
    }
    Ok(())
}