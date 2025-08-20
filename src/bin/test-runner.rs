//! Test runner for B2BUA media translation and features using external tools
//! 
//! This tool orchestrates testing using SIPp, FFmpeg, and other standard tools
//! to comprehensively test the Redfire Gateway B2BUA implementation.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::process::Command as AsyncCommand;
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

#[derive(Parser)]
#[command(name = "test-runner")]
#[command(about = "B2BUA testing framework using SIPp and external tools")]
#[command(version = "1.0.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Gateway address under test
    #[arg(long, default_value = "127.0.0.1:5060")]
    gateway: SocketAddr,

    /// Local bind address for test tools
    #[arg(long, default_value = "127.0.0.1")]
    bind_address: String,

    /// Test results output directory
    #[arg(long, default_value = "./test-results")]
    output_dir: PathBuf,

    /// SIPp executable path
    #[arg(long, default_value = "sipp")]
    sipp_path: String,

    /// FFmpeg executable path
    #[arg(long, default_value = "ffmpeg")]
    ffmpeg_path: String,

    /// Verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Test basic SIP call flow
    BasicCall {
        /// Number of calls to test
        #[arg(long, default_value = "1")]
        calls: u32,
        /// Call duration in seconds
        #[arg(long, default_value = "30")]
        duration: u32,
        /// Test codec
        #[arg(long, value_enum, default_value = "g711u")]
        codec: TestCodec,
    },
    /// Test media transcoding between different codecs
    Transcoding {
        /// Source codec
        #[arg(long, value_enum)]
        from_codec: TestCodec,
        /// Target codec
        #[arg(long, value_enum)]
        to_codec: TestCodec,
        /// Test duration in seconds
        #[arg(long, default_value = "60")]
        duration: u32,
    },
    /// Stress test with multiple concurrent calls
    Stress {
        /// Number of concurrent calls
        #[arg(long, default_value = "50")]
        concurrent: u32,
        /// Total number of calls
        #[arg(long, default_value = "500")]
        total: u32,
        /// Calls per second rate
        #[arg(long, default_value = "10")]
        rate: u32,
        /// Call duration in seconds
        #[arg(long, default_value = "60")]
        duration: u32,
    },
    /// Test DTMF relay and detection
    Dtmf {
        /// DTMF sequence to test
        #[arg(long, default_value = "123456789*0#")]
        sequence: String,
        /// DTMF method (RFC2833 or INFO)
        #[arg(long, value_enum, default_value = "rfc2833")]
        method: DtmfMethod,
    },
    /// Test media quality under various conditions
    Quality {
        /// Packet loss percentage to simulate
        #[arg(long, default_value = "0.0")]
        packet_loss: f64,
        /// Jitter in milliseconds
        #[arg(long, default_value = "0")]
        jitter: u32,
        /// Network delay in milliseconds
        #[arg(long, default_value = "0")]
        delay: u32,
    },
    /// Test codec negotiation
    Negotiation {
        /// Preferred codecs list
        #[arg(long, value_delimiter = ',')]
        codecs: Vec<TestCodec>,
    },
    /// Run comprehensive test suite
    Suite {
        /// Test suite configuration file
        #[arg(long)]
        config: Option<PathBuf>,
        /// Include stress tests
        #[arg(long)]
        include_stress: bool,
    },
    /// Generate media files for testing
    GenerateMedia {
        /// Media type to generate
        #[arg(value_enum)]
        media_type: MediaType,
        /// Output format
        #[arg(long, value_enum, default_value = "wav")]
        format: AudioFormat,
        /// Duration in seconds
        #[arg(long, default_value = "30")]
        duration: u32,
    },
    /// Analyze captured media for quality
    AnalyzeMedia {
        /// Input media file
        input: PathBuf,
        /// Reference file for comparison
        #[arg(long)]
        reference: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, ValueEnum, Serialize, Deserialize)]
enum TestCodec {
    G711u,
    G711a,
    G722,
    G729,
    Opus,
    Speex,
}

#[derive(Debug, Clone, ValueEnum)]
enum DtmfMethod {
    Rfc2833,
    Info,
    Inband,
}

#[derive(Debug, Clone, ValueEnum)]
enum MediaType {
    /// 1000Hz sine wave tone
    Tone1000,
    /// 440Hz sine wave (musical A)
    Tone440,
    /// DTMF sequence
    Dtmf,
    /// White noise
    Noise,
    /// Music sample
    Music,
    /// Voice sample
    Voice,
    /// Mixed content
    Mixed,
}

#[derive(Clone, ValueEnum)]
enum AudioFormat {
    Wav,
    Raw,
    Au,
}

#[derive(Serialize, Deserialize)]
struct TestSuite {
    name: String,
    description: String,
    tests: Vec<TestCase>,
}

#[derive(Serialize, Deserialize)]
struct TestCase {
    name: String,
    test_type: String,
    parameters: HashMap<String, serde_json::Value>,
    expected_results: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct TestResult {
    test_name: String,
    success: bool,
    duration: Duration,
    metrics: HashMap<String, f64>,
    errors: Vec<String>,
    warnings: Vec<String>,
}

struct TestRunner {
    gateway: SocketAddr,
    bind_address: String,
    output_dir: PathBuf,
    sipp_path: String,
    ffmpeg_path: String,
    results: Vec<TestResult>,
}

impl TestRunner {
    fn new(
        gateway: SocketAddr,
        bind_address: String,
        output_dir: PathBuf,
        sipp_path: String,
        ffmpeg_path: String,
    ) -> Self {
        Self {
            gateway,
            bind_address,
            output_dir,
            sipp_path,
            ffmpeg_path,
            results: Vec::new(),
        }
    }

    async fn setup(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Create output directory
        fs::create_dir_all(&self.output_dir).await?;
        
        // Create subdirectories
        fs::create_dir_all(self.output_dir.join("scenarios")).await?;
        fs::create_dir_all(self.output_dir.join("media")).await?;
        fs::create_dir_all(self.output_dir.join("logs")).await?;
        fs::create_dir_all(self.output_dir.join("captures")).await?;

        // Check tool availability
        self.check_tools().await?;

        // Generate SIPp scenarios
        self.generate_sipp_scenarios().await?;

        info!("Test environment setup complete");
        Ok(())
    }

    async fn check_tools(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Check SIPp
        let sipp_check = Command::new(&self.sipp_path)
            .arg("-v")
            .output();

        match sipp_check {
            Ok(output) => {
                if output.status.success() {
                    info!("SIPp found: {}", String::from_utf8_lossy(&output.stdout));
                } else {
                    warn!("SIPp may not be properly installed");
                }
            }
            Err(_) => {
                error!("SIPp not found at: {}", self.sipp_path);
                return Err("SIPp not available".into());
            }
        }

        // Check FFmpeg
        let ffmpeg_check = Command::new(&self.ffmpeg_path)
            .arg("-version")
            .output();

        match ffmpeg_check {
            Ok(output) => {
                if output.status.success() {
                    info!("FFmpeg found");
                } else {
                    warn!("FFmpeg may not be properly installed");
                }
            }
            Err(_) => {
                warn!("FFmpeg not found at: {} (media generation will be limited)", self.ffmpeg_path);
            }
        }

        Ok(())
    }

    async fn generate_sipp_scenarios(&self) -> Result<(), Box<dyn std::error::Error>> {
        let scenarios_dir = self.output_dir.join("scenarios");

        // Basic UAC scenario with media
        let uac_scenario = self.create_uac_scenario().await?;
        fs::write(scenarios_dir.join("uac_basic.xml"), uac_scenario).await?;

        // Basic UAS scenario
        let uas_scenario = self.create_uas_scenario().await?;
        fs::write(scenarios_dir.join("uas_basic.xml"), uas_scenario).await?;

        // DTMF testing scenario
        let dtmf_scenario = self.create_dtmf_scenario().await?;
        fs::write(scenarios_dir.join("dtmf_test.xml"), dtmf_scenario).await?;

        // Codec negotiation scenario
        let codec_scenario = self.create_codec_scenario().await?;
        fs::write(scenarios_dir.join("codec_test.xml"), codec_scenario).await?;

        info!("SIPp scenarios generated");
        Ok(())
    }

    async fn create_uac_scenario(&self) -> Result<String, Box<dyn std::error::Error>> {
        Ok(r#"<?xml version="1.0" encoding="ISO-8859-1"?>
<!DOCTYPE scenario SYSTEM "sipp.dtd">

<scenario name="B2BUA UAC Test">
  <send retrans="500">
    <![CDATA[
      INVITE sip:[field0]@[remote_ip]:[remote_port] SIP/2.0
      Via: SIP/2.0/UDP [local_ip]:[local_port];branch=[branch]
      From: <sip:[field1]@[local_ip]:[local_port]>;tag=[pid]SIPpTag00[call_number]
      To: <sip:[field0]@[remote_ip]:[remote_port]>
      Call-ID: [call_id]
      CSeq: 1 INVITE
      Contact: <sip:[field1]@[local_ip]:[local_port]>
      Max-Forwards: 70
      Subject: B2BUA Test Call
      Content-Type: application/sdp
      Content-Length: [len]

      v=0
      o=user1 53655765 2353687637 IN IP4 [local_ip]
      s=-
      c=IN IP4 [local_ip]
      t=0 0
      m=audio [media_port] RTP/AVP 0 8 18
      a=rtpmap:0 PCMU/8000
      a=rtpmap:8 PCMA/8000
      a=rtpmap:18 G729/8000
      a=ptime:20
      a=sendrecv
    ]]>
  </send>

  <recv response="100" optional="true" />
  <recv response="180" optional="true" />
  <recv response="183" optional="true" />

  <recv response="200" rtd="true">
    <action>
      <ereg regexp="m=audio ([0-9]+)" search_in="body" check_it="true" assign_to="remote_media_port" />
    </action>
  </recv>

  <send>
    <![CDATA[
      ACK sip:[field0]@[remote_ip]:[remote_port] SIP/2.0
      Via: SIP/2.0/UDP [local_ip]:[local_port];branch=[branch]
      From: <sip:[field1]@[local_ip]:[local_port]>;tag=[pid]SIPpTag00[call_number]
      To: <sip:[field0]@[remote_ip]:[remote_port]>[peer_tag_param]
      Call-ID: [call_id]
      CSeq: 1 ACK
      Contact: <sip:[field1]@[local_ip]:[local_port]>
      Max-Forwards: 70
      Content-Length: 0
    ]]>
  </send>

  <!-- Start RTP media -->
  <nop>
    <action>
      <exec rtp_stream="[media_port],[remote_ip:$remote_media_port],0" />
    </action>
  </nop>

  <pause milliseconds="[$duration]000" />

  <send retrans="500">
    <![CDATA[
      BYE sip:[field0]@[remote_ip]:[remote_port] SIP/2.0
      Via: SIP/2.0/UDP [local_ip]:[local_port];branch=[branch]
      From: <sip:[field1]@[local_ip]:[local_port]>;tag=[pid]SIPpTag00[call_number]
      To: <sip:[field0]@[remote_ip]:[remote_port]>[peer_tag_param]
      Call-ID: [call_id]
      CSeq: 2 BYE
      Contact: <sip:[field1]@[local_ip]:[local_port]>
      Max-Forwards: 70
      Content-Length: 0
    ]]>
  </send>

  <recv response="200" crlf="true" />
</scenario>"#.to_string())
    }

    async fn create_uas_scenario(&self) -> Result<String, Box<dyn std::error::Error>> {
        Ok(r#"<?xml version="1.0" encoding="ISO-8859-1"?>
<!DOCTYPE scenario SYSTEM "sipp.dtd">

<scenario name="B2BUA UAS Test">
  <recv request="INVITE" crlf="true">
    <action>
      <ereg regexp="m=audio ([0-9]+)" search_in="body" check_it="true" assign_to="remote_media_port" />
    </action>
  </recv>

  <send>
    <![CDATA[
      SIP/2.0 180 Ringing
      [last_Via:]
      [last_From:]
      [last_To:];tag=[pid]SIPpTag01[call_number]
      [last_Call-ID:]
      [last_CSeq:]
      Contact: <sip:[local_ip]:[local_port];transport=[transport]>
      Content-Length: 0
    ]]>
  </send>

  <pause milliseconds="1000" />

  <send retrans="500">
    <![CDATA[
      SIP/2.0 200 OK
      [last_Via:]
      [last_From:]
      [last_To:];tag=[pid]SIPpTag01[call_number]
      [last_Call-ID:]
      [last_CSeq:]
      Contact: <sip:[local_ip]:[local_port];transport=[transport]>
      Content-Type: application/sdp
      Content-Length: [len]

      v=0
      o=user1 53655765 2353687637 IN IP4 [local_ip]
      s=-
      c=IN IP4 [local_ip]
      t=0 0
      m=audio [media_port] RTP/AVP 0 8
      a=rtpmap:0 PCMU/8000
      a=rtpmap:8 PCMA/8000
      a=ptime:20
      a=sendrecv
    ]]>
  </send>

  <recv request="ACK" rtd="true" crlf="true" />

  <!-- Start RTP media -->
  <nop>
    <action>
      <exec rtp_stream="[media_port],[remote_ip:$remote_media_port],0" />
    </action>
  </nop>

  <recv request="BYE" />

  <send>
    <![CDATA[
      SIP/2.0 200 OK
      [last_Via:]
      [last_From:]
      [last_To:]
      [last_Call-ID:]
      [last_CSeq:]
      Content-Length: 0
    ]]>
  </send>
</scenario>"#.to_string())
    }

    async fn create_dtmf_scenario(&self) -> Result<String, Box<dyn std::error::Error>> {
        Ok(r#"<?xml version="1.0" encoding="ISO-8859-1"?>
<!DOCTYPE scenario SYSTEM "sipp.dtd">

<scenario name="DTMF Test">
  <!-- Standard call setup -->
  <send retrans="500">
    <![CDATA[
      INVITE sip:[field0]@[remote_ip]:[remote_port] SIP/2.0
      Via: SIP/2.0/UDP [local_ip]:[local_port];branch=[branch]
      From: <sip:[field1]@[local_ip]:[local_port]>;tag=[pid]SIPpTag00[call_number]
      To: <sip:[field0]@[remote_ip]:[remote_port]>
      Call-ID: [call_id]
      CSeq: 1 INVITE
      Contact: <sip:[field1]@[local_ip]:[local_port]>
      Content-Type: application/sdp
      Content-Length: [len]

      v=0
      o=user1 53655765 2353687637 IN IP4 [local_ip]
      s=-
      c=IN IP4 [local_ip]
      t=0 0
      m=audio [media_port] RTP/AVP 0 101
      a=rtpmap:0 PCMU/8000
      a=rtpmap:101 telephone-event/8000
      a=fmtp:101 0-15
      a=ptime:20
    ]]>
  </send>

  <recv response="200" />
  <send><![CDATA[ACK sip:[field0]@[remote_ip]:[remote_port] SIP/2.0
    Via: SIP/2.0/UDP [local_ip]:[local_port];branch=[branch]
    From: <sip:[field1]@[local_ip]:[local_port]>;tag=[pid]SIPpTag00[call_number]
    To: <sip:[field0]@[remote_ip]:[remote_port]>[peer_tag_param]
    Call-ID: [call_id]
    CSeq: 1 ACK
    Content-Length: 0]]></send>

  <!-- Send DTMF via RTP events -->
  <nop>
    <action>
      <exec rtp_stream="[media_port],[remote_ip:$remote_media_port],101" />
    </action>
  </nop>

  <pause milliseconds="10000" />

  <!-- Send DTMF via SIP INFO -->
  <send>
    <![CDATA[
      INFO sip:[field0]@[remote_ip]:[remote_port] SIP/2.0
      Via: SIP/2.0/UDP [local_ip]:[local_port];branch=[branch]
      From: <sip:[field1]@[local_ip]:[local_port]>;tag=[pid]SIPpTag00[call_number]
      To: <sip:[field0]@[remote_ip]:[remote_port]>[peer_tag_param]
      Call-ID: [call_id]
      CSeq: 2 INFO
      Content-Type: application/dtmf-relay
      Content-Length: [len]

      Signal=1
      Duration=100
    ]]>
  </send>

  <recv response="200" />

  <pause milliseconds="5000" />

  <send>
    <![CDATA[
      BYE sip:[field0]@[remote_ip]:[remote_port] SIP/2.0
      Via: SIP/2.0/UDP [local_ip]:[local_port];branch=[branch]
      From: <sip:[field1]@[local_ip]:[local_port]>;tag=[pid]SIPpTag00[call_number]
      To: <sip:[field0]@[remote_ip]:[remote_port]>[peer_tag_param]
      Call-ID: [call_id]
      CSeq: 3 BYE
      Content-Length: 0
    ]]>
  </send>

  <recv response="200" />
</scenario>"#.to_string())
    }

    async fn create_codec_scenario(&self) -> Result<String, Box<dyn std::error::Error>> {
        Ok(r#"<?xml version="1.0" encoding="ISO-8859-1"?>
<!DOCTYPE scenario SYSTEM "sipp.dtd">

<scenario name="Codec Negotiation Test">
  <send retrans="500">
    <![CDATA[
      INVITE sip:[field0]@[remote_ip]:[remote_port] SIP/2.0
      Via: SIP/2.0/UDP [local_ip]:[local_port];branch=[branch]
      From: <sip:[field1]@[local_ip]:[local_port]>;tag=[pid]SIPpTag00[call_number]
      To: <sip:[field0]@[remote_ip]:[remote_port]>
      Call-ID: [call_id]
      CSeq: 1 INVITE
      Contact: <sip:[field1]@[local_ip]:[local_port]>
      Content-Type: application/sdp
      Content-Length: [len]

      v=0
      o=user1 53655765 2353687637 IN IP4 [local_ip]
      s=-
      c=IN IP4 [local_ip]
      t=0 0
      m=audio [media_port] RTP/AVP 0 8 9 18 96 97
      a=rtpmap:0 PCMU/8000
      a=rtpmap:8 PCMA/8000
      a=rtpmap:9 G722/8000
      a=rtpmap:18 G729/8000
      a=rtpmap:96 opus/48000/2
      a=rtpmap:97 speex/8000
      a=ptime:20
    ]]>
  </send>

  <recv response="200">
    <action>
      <ereg regexp="a=rtpmap:([0-9]+)" search_in="body" check_it="true" assign_to="negotiated_codec" />
    </action>
  </recv>

  <send>
    <![CDATA[
      ACK sip:[field0]@[remote_ip]:[remote_port] SIP/2.0
      Via: SIP/2.0/UDP [local_ip]:[local_port];branch=[branch]
      From: <sip:[field1]@[local_ip]:[local_port]>;tag=[pid]SIPpTag00[call_number]
      To: <sip:[field0]@[remote_ip]:[remote_port]>[peer_tag_param]
      Call-ID: [call_id]
      CSeq: 1 ACK
      Content-Length: 0
    ]]>
  </send>

  <pause milliseconds="5000" />

  <send>
    <![CDATA[
      BYE sip:[field0]@[remote_ip]:[remote_port] SIP/2.0
      Via: SIP/2.0/UDP [local_ip]:[local_port];branch=[branch]
      From: <sip:[field1]@[local_ip]:[local_port]>;tag=[pid]SIPpTag00[call_number]
      To: <sip:[field0]@[remote_ip]:[remote_port]>[peer_tag_param]
      Call-ID: [call_id]
      CSeq: 2 BYE
      Content-Length: 0
    ]]>
  </send>

  <recv response="200" />
</scenario>"#.to_string())
    }

    async fn run_basic_call_test(&mut self, calls: u32, duration: u32, codec: TestCodec) -> Result<(), Box<dyn std::error::Error>> {
        info!("Running basic call test: {} calls, {} seconds, codec: {:?}", calls, duration, codec);
        
        let start_time = Instant::now();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Start UAS (called party)
        let uas_port = 5080;
        let uas_cmd = AsyncCommand::new(&self.sipp_path)
            .args([
                "-sf", &self.output_dir.join("scenarios/uas_basic.xml").to_string_lossy(),
                "-p", &uas_port.to_string(),
                "-m", &calls.to_string(),
                "-bg",
            ])
            .spawn()?;

        // Wait a moment for UAS to start
        sleep(Duration::from_secs(1)).await;

        // Start UAC (calling party)
        let uac_output = AsyncCommand::new(&self.sipp_path)
            .args([
                "-sf", &self.output_dir.join("scenarios/uac_basic.xml").to_string_lossy(),
                &format!("{}:{}", self.gateway.ip(), uas_port),
                "-s", "test",
                "-p", "5070",
                "-m", &calls.to_string(),
                "-d", &(duration * 1000).to_string(), // SIPp expects milliseconds
                "-r", "1", // 1 call per second
                "-trace_msg",
                "-message_file", &self.output_dir.join("logs/sip_messages.log").to_string_lossy(),
            ])
            .output()
            .await?;

        if !uac_output.status.success() {
            let error_msg = String::from_utf8_lossy(&uac_output.stderr);
            errors.push(format!("SIPp UAC failed: {}", error_msg));
        }

        let test_duration = start_time.elapsed();
        
        // Parse SIPp statistics
        let metrics = self.parse_sipp_output(&uac_output.stdout).await?;

        let result = TestResult {
            test_name: format!("basic_call_{:?}", codec),
            success: errors.is_empty(),
            duration: test_duration,
            metrics,
            errors,
            warnings,
        };

        self.results.push(result);
        Ok(())
    }

    async fn run_transcoding_test(&mut self, from_codec: TestCodec, to_codec: TestCodec, duration: u32) -> Result<(), Box<dyn std::error::Error>> {
        info!("Running transcoding test: {:?} -> {:?}, {} seconds", from_codec, to_codec, duration);
        
        let start_time = Instant::now();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Generate test media file
        let media_file = self.generate_test_media(MediaType::Tone1000, AudioFormat::Raw, 60).await?;

        // Start UAS with target codec
        let uas_cmd = AsyncCommand::new(&self.sipp_path)
            .args([
                "-sf", &self.output_dir.join("scenarios/uas_basic.xml").to_string_lossy(),
                "-p", "5081",
                "-m", "1",
                "-mi", &self.bind_address,
                "-rtp_echo",
            ])
            .spawn()?;

        sleep(Duration::from_secs(1)).await;

        // Start UAC with source codec
        let uac_output = AsyncCommand::new(&self.sipp_path)
            .args([
                "-sf", &self.output_dir.join("scenarios/uac_basic.xml").to_string_lossy(),
                &format!("{}:5081", self.gateway.ip()),
                "-s", "transcoding_test",
                "-p", "5071",
                "-m", "1",
                "-d", &(duration * 1000).to_string(),
                "-rtp_echo",
                "-trace_msg",
            ])
            .output()
            .await?;

        if !uac_output.status.success() {
            let error_msg = String::from_utf8_lossy(&uac_output.stderr);
            errors.push(format!("Transcoding test failed: {}", error_msg));
        }

        let test_duration = start_time.elapsed();
        let metrics = self.parse_sipp_output(&uac_output.stdout).await?;

        let result = TestResult {
            test_name: format!("transcoding_{:?}_to_{:?}", from_codec, to_codec),
            success: errors.is_empty(),
            duration: test_duration,
            metrics,
            errors,
            warnings,
        };

        self.results.push(result);
        Ok(())
    }

    async fn run_stress_test(&mut self, concurrent: u32, total: u32, rate: u32, duration: u32) -> Result<(), Box<dyn std::error::Error>> {
        info!("Running stress test: {} concurrent, {} total, {} CPS, {} seconds", concurrent, total, rate, duration);
        
        let start_time = Instant::now();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Start multiple UAS instances
        let mut uas_processes = Vec::new();
        for i in 0..5 {
            let port = 5090 + i;
            let uas_cmd = AsyncCommand::new(&self.sipp_path)
                .args([
                    "-sf", &self.output_dir.join("scenarios/uas_basic.xml").to_string_lossy(),
                    "-p", &port.to_string(),
                    "-m", &(total / 5).to_string(),
                    "-mi", &self.bind_address,
                ])
                .spawn()?;
            uas_processes.push(uas_cmd);
        }

        sleep(Duration::from_secs(2)).await;

        // Start UAC with stress parameters
        let uac_output = AsyncCommand::new(&self.sipp_path)
            .args([
                "-sf", &self.output_dir.join("scenarios/uac_basic.xml").to_string_lossy(),
                &format!("{}:5090", self.gateway.ip()),
                "-s", "stress_test",
                "-p", "5072",
                "-m", &total.to_string(),
                "-l", &concurrent.to_string(), // Max simultaneous calls
                "-r", &rate.to_string(),       // Call rate
                "-d", &(duration * 1000).to_string(),
                "-trace_msg",
                "-message_file", &self.output_dir.join("logs/stress_test.log").to_string_lossy(),
            ])
            .output()
            .await?;

        if !uac_output.status.success() {
            let error_msg = String::from_utf8_lossy(&uac_output.stderr);
            errors.push(format!("Stress test failed: {}", error_msg));
        }

        let test_duration = start_time.elapsed();
        let metrics = self.parse_sipp_output(&uac_output.stdout).await?;

        let result = TestResult {
            test_name: "stress_test".to_string(),
            success: errors.is_empty(),
            duration: test_duration,
            metrics,
            errors,
            warnings,
        };

        self.results.push(result);
        Ok(())
    }

    async fn run_dtmf_test(&mut self, sequence: String, method: DtmfMethod) -> Result<(), Box<dyn std::error::Error>> {
        info!("Running DTMF test: sequence '{}', method: {:?}", sequence, method);
        
        let start_time = Instant::now();
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Start UAS
        let uas_cmd = AsyncCommand::new(&self.sipp_path)
            .args([
                "-sf", &self.output_dir.join("scenarios/uas_basic.xml").to_string_lossy(),
                "-p", "5082",
                "-m", "1",
            ])
            .spawn()?;

        sleep(Duration::from_secs(1)).await;

        // Start UAC with DTMF scenario
        let uac_output = AsyncCommand::new(&self.sipp_path)
            .args([
                "-sf", &self.output_dir.join("scenarios/dtmf_test.xml").to_string_lossy(),
                &format!("{}:5082", self.gateway.ip()),
                "-s", "dtmf_test",
                "-p", "5073",
                "-m", "1",
                "-trace_msg",
            ])
            .output()
            .await?;

        if !uac_output.status.success() {
            let error_msg = String::from_utf8_lossy(&uac_output.stderr);
            errors.push(format!("DTMF test failed: {}", error_msg));
        }

        let test_duration = start_time.elapsed();
        let metrics = self.parse_sipp_output(&uac_output.stdout).await?;

        let result = TestResult {
            test_name: format!("dtmf_{:?}", method),
            success: errors.is_empty(),
            duration: test_duration,
            metrics,
            errors,
            warnings,
        };

        self.results.push(result);
        Ok(())
    }

    async fn generate_test_media(&self, media_type: MediaType, format: AudioFormat, duration: u32) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let filename = match format {
            AudioFormat::Wav => format!("{:?}_{}.wav", media_type, duration),
            AudioFormat::Raw => format!("{:?}_{}.raw", media_type, duration),
            AudioFormat::Au => format!("{:?}_{}.au", media_type, duration),
        };
        
        let output_path = self.output_dir.join("media").join(filename);

        match media_type {
            MediaType::Tone1000 => {
                let output = AsyncCommand::new(&self.ffmpeg_path)
                    .args([
                        "-f", "lavfi",
                        "-i", &format!("sine=frequency=1000:duration={}", duration),
                        "-ar", "8000",
                        "-ac", "1",
                        "-y",
                        &output_path.to_string_lossy(),
                    ])
                    .output()
                    .await?;

                if !output.status.success() {
                    return Err(format!("FFmpeg failed: {}", String::from_utf8_lossy(&output.stderr)).into());
                }
            }
            MediaType::Tone440 => {
                let output = AsyncCommand::new(&self.ffmpeg_path)
                    .args([
                        "-f", "lavfi",
                        "-i", &format!("sine=frequency=440:duration={}", duration),
                        "-ar", "8000",
                        "-ac", "1",
                        "-y",
                        &output_path.to_string_lossy(),
                    ])
                    .output()
                    .await?;

                if !output.status.success() {
                    return Err(format!("FFmpeg failed: {}", String::from_utf8_lossy(&output.stderr)).into());
                }
            }
            MediaType::Noise => {
                let output = AsyncCommand::new(&self.ffmpeg_path)
                    .args([
                        "-f", "lavfi",
                        "-i", &format!("anoisesrc=duration={}:color=white:amplitude=0.1", duration),
                        "-ar", "8000",
                        "-ac", "1",
                        "-y",
                        &output_path.to_string_lossy(),
                    ])
                    .output()
                    .await?;

                if !output.status.success() {
                    return Err(format!("FFmpeg failed: {}", String::from_utf8_lossy(&output.stderr)).into());
                }
            }
            MediaType::Dtmf => {
                // Generate DTMF sequence
                let dtmf_sequence = "1234567890*#ABCD";
                let tone_duration = duration as f64 / dtmf_sequence.len() as f64;
                
                // This is a simplified implementation - real DTMF would use dual tones
                let output = AsyncCommand::new(&self.ffmpeg_path)
                    .args([
                        "-f", "lavfi",
                        "-i", &format!("sine=frequency=1000:duration={}", duration),
                        "-ar", "8000",
                        "-ac", "1",
                        "-y",
                        &output_path.to_string_lossy(),
                    ])
                    .output()
                    .await?;

                if !output.status.success() {
                    return Err(format!("FFmpeg failed: {}", String::from_utf8_lossy(&output.stderr)).into());
                }
            }
            _ => {
                // For other types, generate a simple tone
                let output = AsyncCommand::new(&self.ffmpeg_path)
                    .args([
                        "-f", "lavfi",
                        "-i", &format!("sine=frequency=800:duration={}", duration),
                        "-ar", "8000",
                        "-ac", "1",
                        "-y",
                        &output_path.to_string_lossy(),
                    ])
                    .output()
                    .await?;

                if !output.status.success() {
                    return Err(format!("FFmpeg failed: {}", String::from_utf8_lossy(&output.stderr)).into());
                }
            }
        }

        info!("Generated test media: {:?}", output_path);
        Ok(output_path)
    }

    async fn parse_sipp_output(&self, output: &[u8]) -> Result<HashMap<String, f64>, Box<dyn std::error::Error>> {
        let output_str = String::from_utf8_lossy(output);
        let mut metrics = HashMap::new();

        // Parse SIPp statistics
        for line in output_str.lines() {
            if line.contains("Total-time") {
                if let Some(time_str) = line.split_whitespace().last() {
                    if let Ok(time) = time_str.parse::<f64>() {
                        metrics.insert("total_time_ms".to_string(), time);
                    }
                }
            } else if line.contains("Total-calls") {
                if let Some(calls_str) = line.split_whitespace().last() {
                    if let Ok(calls) = calls_str.parse::<f64>() {
                        metrics.insert("total_calls".to_string(), calls);
                    }
                }
            } else if line.contains("Successful-calls") {
                if let Some(calls_str) = line.split_whitespace().last() {
                    if let Ok(calls) = calls_str.parse::<f64>() {
                        metrics.insert("successful_calls".to_string(), calls);
                    }
                }
            } else if line.contains("Failed-calls") {
                if let Some(calls_str) = line.split_whitespace().last() {
                    if let Ok(calls) = calls_str.parse::<f64>() {
                        metrics.insert("failed_calls".to_string(), calls);
                    }
                }
            }
        }

        // Calculate success rate
        if let (Some(total), Some(successful)) = (metrics.get("total_calls"), metrics.get("successful_calls")) {
            if *total > 0.0 {
                metrics.insert("success_rate_percent".to_string(), (*successful / *total) * 100.0);
            }
        }

        Ok(metrics)
    }

    async fn save_results(&self) -> Result<(), Box<dyn std::error::Error>> {
        let results_file = self.output_dir.join("test_results.json");
        let json = serde_json::to_string_pretty(&self.results)?;
        fs::write(results_file, json).await?;

        // Generate summary report
        self.generate_summary_report().await?;

        info!("Test results saved to: {:?}", self.output_dir);
        Ok(())
    }

    async fn generate_summary_report(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut report = String::new();
        report.push_str("# B2BUA Test Results Summary\n\n");

        let total_tests = self.results.len();
        let passed_tests = self.results.iter().filter(|r| r.success).count();
        let failed_tests = total_tests - passed_tests;

        report.push_str(&format!("**Total Tests:** {}\n", total_tests));
        report.push_str(&format!("**Passed:** {}\n", passed_tests));
        report.push_str(&format!("**Failed:** {}\n", failed_tests));
        report.push_str(&format!("**Success Rate:** {:.1}%\n\n", (passed_tests as f64 / total_tests as f64) * 100.0));

        report.push_str("## Test Details\n\n");

        for result in &self.results {
            report.push_str(&format!("### {}\n", result.test_name));
            report.push_str(&format!("- **Status:** {}\n", if result.success { "✅ PASSED" } else { "❌ FAILED" }));
            report.push_str(&format!("- **Duration:** {:.2}s\n", result.duration.as_secs_f64()));
            
            if !result.metrics.is_empty() {
                report.push_str("- **Metrics:**\n");
                for (key, value) in &result.metrics {
                    report.push_str(&format!("  - {}: {:.2}\n", key, value));
                }
            }

            if !result.errors.is_empty() {
                report.push_str("- **Errors:**\n");
                for error in &result.errors {
                    report.push_str(&format!("  - {}\n", error));
                }
            }

            report.push_str("\n");
        }

        let report_file = self.output_dir.join("summary_report.md");
        fs::write(report_file, report).await?;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Initialize logging
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("debug")
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter("info")
            .init();
    }

    let mut test_runner = TestRunner::new(
        cli.gateway,
        cli.bind_address,
        cli.output_dir,
        cli.sipp_path,
        cli.ffmpeg_path,
    );

    test_runner.setup().await?;

    match cli.command {
        Commands::BasicCall { calls, duration, codec } => {
            test_runner.run_basic_call_test(calls, duration, codec).await?;
        }
        Commands::Transcoding { from_codec, to_codec, duration } => {
            test_runner.run_transcoding_test(from_codec, to_codec, duration).await?;
        }
        Commands::Stress { concurrent, total, rate, duration } => {
            test_runner.run_stress_test(concurrent, total, rate, duration).await?;
        }
        Commands::Dtmf { sequence, method } => {
            test_runner.run_dtmf_test(sequence, method).await?;
        }
        Commands::Quality { packet_loss, jitter, delay } => {
            info!("Quality test with {}% loss, {}ms jitter, {}ms delay", packet_loss, jitter, delay);
            // Implementation would use network emulation tools like tc/netem
        }
        Commands::Negotiation { codecs } => {
            info!("Testing codec negotiation with: {:?}", codecs);
            // Run codec negotiation test
        }
        Commands::Suite { config, include_stress } => {
            info!("Running test suite (include_stress: {})", include_stress);
            
            // Run basic tests
            test_runner.run_basic_call_test(5, 30, TestCodec::G711u).await?;
            test_runner.run_transcoding_test(TestCodec::G711u, TestCodec::G711a, 30).await?;
            test_runner.run_dtmf_test("123456789*0#".to_string(), DtmfMethod::Rfc2833).await?;

            if include_stress {
                test_runner.run_stress_test(20, 100, 5, 60).await?;
            }
        }
        Commands::GenerateMedia { media_type, format, duration } => {
            let output_path = test_runner.generate_test_media(media_type, format, duration).await?;
            println!("Generated media file: {:?}", output_path);
            return Ok(());
        }
        Commands::AnalyzeMedia { input, reference } => {
            info!("Analyzing media file: {:?}", input);
            if let Some(ref_file) = reference {
                info!("Reference file: {:?}", ref_file);
            }
            // Implementation would analyze audio quality metrics
        }
    }

    test_runner.save_results().await?;
    println!("Test execution completed. Results saved to: {:?}", test_runner.output_dir);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codec_enum() {
        assert_eq!(format!("{:?}", TestCodec::G711u), "G711u");
        assert_eq!(format!("{:?}", TestCodec::Opus), "Opus");
    }

    #[tokio::test]
    async fn test_scenario_generation() {
        let runner = TestRunner::new(
            "127.0.0.1:5060".parse().unwrap(),
            "127.0.0.1".to_string(),
            PathBuf::from("/tmp/test"),
            "sipp".to_string(),
            "ffmpeg".to_string(),
        );

        let scenario = runner.create_uac_scenario().await.unwrap();
        assert!(scenario.contains("INVITE"));
        assert!(scenario.contains("BYE"));
        assert!(scenario.contains("application/sdp"));
    }
}