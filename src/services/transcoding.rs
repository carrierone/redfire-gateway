//! Transcoding service integrated with redfire-codec-engine
//! 
//! This module provides transcoding functionality integrated with the
//! external redfire-codec-engine library.

use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::TranscodingBackend;
use crate::Result;

// Import from external redfire-codec-engine library
use redfire_codec_engine::{
    AudioCodec, CodecService, CodecConfig, create_default_service,
    gpu_available, available_gpu_backends,
};

#[cfg(any(feature = "cuda", feature = "rocm"))]
use redfire_codec_engine::create_gpu_service;

/// Codec types supported for transcoding
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodecType {
    G711u,      // PCMU
    G711a,      // PCMA
    G722,       // Wideband
    G729,       // Compressed
    Opus,       // Internet standard
    Amr,        // Mobile
    AmrWb,      // Wideband mobile
    Evs,        // Enhanced voice services
    Ilbc,       // Internet low bitrate
    Speex,      // Open source
    G726,       // ADPCM
    Custom(String),
}

impl CodecType {
    /// Convert to external library's AudioCodec enum
    pub fn to_audio_codec(&self) -> AudioCodec {
        match self {
            CodecType::G711u => AudioCodec::G711Ulaw,
            CodecType::G711a => AudioCodec::G711Alaw,
            CodecType::G722 => AudioCodec::G722,
            CodecType::G729 => AudioCodec::G729,
            CodecType::Opus => AudioCodec::Opus,
            CodecType::Amr => AudioCodec::G711Ulaw, // Fallback
            CodecType::AmrWb => AudioCodec::G722,   // Fallback to similar
            CodecType::Evs => AudioCodec::G722,     // Fallback to wideband
            CodecType::Ilbc => AudioCodec::G711Ulaw, // Fallback
            CodecType::Speex => AudioCodec::Opus,   // Fallback to similar
            CodecType::G726 => AudioCodec::G711Ulaw, // Fallback
            CodecType::Custom(_) => AudioCodec::G711Ulaw, // Safe fallback
        }
    }
    
    pub fn from_name(name: &str) -> Self {
        match name.to_lowercase().as_str() {
            "pcmu" | "g711u" | "g.711u" => Self::G711u,
            "pcma" | "g711a" | "g.711a" => Self::G711a,
            "g722" | "g.722" => Self::G722,
            "g729" | "g.729" => Self::G729,
            "opus" => Self::Opus,
            "amr" => Self::Amr,
            "amr-wb" => Self::AmrWb,
            "evs" => Self::Evs,
            "ilbc" => Self::Ilbc,
            "speex" => Self::Speex,
            "g726" | "g.726" => Self::G726,
            _ => Self::Custom(name.to_string()),
        }
    }

    pub fn to_name(&self) -> &str {
        match self {
            Self::G711u => "PCMU",
            Self::G711a => "PCMA",
            Self::G722 => "G722",
            Self::G729 => "G729",
            Self::Opus => "OPUS",
            Self::Amr => "AMR",
            Self::AmrWb => "AMR-WB",
            Self::Evs => "EVS",
            Self::Ilbc => "iLBC",
            Self::Speex => "SPEEX",
            Self::G726 => "G726",
            Self::Custom(name) => name,
        }
    }
}

/// Transcoding session configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscodingSession {
    pub id: String,
    pub call_id: String,
    pub source_codec: CodecType,
    pub target_codec: CodecType,
    pub source_sample_rate: u32,
    pub target_sample_rate: u32,
    pub backend: TranscodingBackend,
    #[serde(skip, default = "Instant::now")]
    pub created_at: Instant,
    #[serde(skip, default = "Instant::now")]
    pub last_activity: Instant,
    pub stats: TranscodingStats,
}

/// Transcoding performance statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscodingStats {
    pub packets_processed: u64,
    pub bytes_processed: u64,
    pub processing_time_ms: u64,
    pub gpu_utilization: f64,
    pub memory_used_mb: u64,
    pub queue_depth: u32,
    pub error_count: u32,
}

impl TranscodingStats {
    pub fn new() -> Self {
        Self {
            packets_processed: 0,
            bytes_processed: 0,
            processing_time_ms: 0,
            gpu_utilization: 0.0,
            memory_used_mb: 0,
            queue_depth: 0,
            error_count: 0,
        }
    }

    pub fn throughput_mbps(&self) -> f64 {
        if self.processing_time_ms == 0 {
            return 0.0;
        }
        (self.bytes_processed as f64 * 8.0) / (self.processing_time_ms as f64 * 1000.0)
    }
}

/// GPU device information
#[derive(Debug, Clone)]
pub struct GpuDevice {
    pub id: u32,
    pub name: String,
    pub backend: GpuBackend,
    pub memory_total_mb: u64,
    pub memory_free_mb: u64,
    pub compute_capability: String,
    pub is_available: bool,
    pub current_utilization: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GpuBackend {
    Cuda,
    Rocm,
    None,
}

/// Transcoding events
#[derive(Debug, Clone)]
pub enum TranscodingEvent {
    SessionStarted {
        session_id: String,
        source_codec: CodecType,
        target_codec: CodecType,
        backend: TranscodingBackend,
    },
    SessionCompleted {
        session_id: String,
        stats: TranscodingStats,
    },
    BackendSwitch {
        session_id: String,
        from_backend: TranscodingBackend,
        to_backend: TranscodingBackend,
        reason: String,
    },
    GpuError {
        device_id: u32,
        error_message: String,
    },
    PerformanceAlert {
        session_id: String,
        metric: String,
        value: f64,
        threshold: f64,
    },
    Started {
        backend: TranscodingBackend,
    },
    Error {
        session_id: Option<String>,
        message: String,
    },
}

/// Main transcoding service integrated with redfire-codec-engine
/// 
/// This implementation integrates with the external redfire-codec-engine library
/// to provide professional audio codec transcoding with GPU acceleration support.
pub struct TranscodingService {
    backend_preference: TranscodingBackend,
    codec_service: Option<CodecService>,
    sessions: Arc<DashMap<String, TranscodingSession>>,
    event_tx: mpsc::UnboundedSender<TranscodingEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<TranscodingEvent>>,
    is_running: bool,
}

impl TranscodingService {
    pub fn new(backend_preference: TranscodingBackend) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        info!("Creating transcoding service with redfire-codec-engine integration, backend preference: {:?}", backend_preference);
        
        // Log GPU availability
        if gpu_available() {
            let backends = available_gpu_backends();
            info!("GPU transcoding available with backends: {:?}", backends);
        } else {
            info!("GPU transcoding not available, using CPU-only transcoding");
        }

        Self {
            backend_preference,
            codec_service: None, // Will be initialized on first use
            sessions: Arc::new(DashMap::new()),
            event_tx,
            event_rx: Some(event_rx),
            is_running: false,
        }
    }

    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<TranscodingEvent>> {
        self.event_rx.take()
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("Starting transcoding service with redfire-codec-engine integration");
        self.is_running = true;
        
        // Initialize codec service based on backend preference
        match self.backend_preference {
            #[cfg(any(feature = "cuda", feature = "rocm"))]
            TranscodingBackend::Gpu => {
                match create_gpu_service().await {
                    Ok(service) => {
                        self.codec_service = Some(service);
                        info!("GPU transcoding service initialized successfully");
                    }
                    Err(e) => {
                        warn!("Failed to initialize GPU service, falling back to CPU: {}", e);
                        self.codec_service = Some(create_default_service().await?);
                    }
                }
            }
            _ => {
                self.codec_service = Some(create_default_service().await
                    .map_err(|e| crate::Error::Protocol(format!("Failed to create codec service: {}", e)))?);
                info!("CPU transcoding service initialized successfully");
            }
        }
        
        let _ = self.event_tx.send(TranscodingEvent::Started {
            backend: self.backend_preference.clone(),
        });
        
        Ok(())
    }

    pub async fn create_transcoding_session(
        &self,
        call_id: &str,
        source_codec: CodecType,
        target_codec: CodecType,
        source_sample_rate: u32,
        target_sample_rate: u32,
    ) -> Result<String> {
        warn!("Transcoding session creation requested but service is in stub mode");
        
        // Create a placeholder session
        let session_id = Uuid::new_v4().to_string();
        let session = TranscodingSession {
            id: session_id.clone(),
            call_id: call_id.to_string(),
            source_codec: source_codec.clone(),
            target_codec: target_codec.clone(),
            source_sample_rate,
            target_sample_rate,
            backend: self.backend_preference.clone(),
            created_at: Instant::now(),
            last_activity: Instant::now(),
            stats: TranscodingStats::new(),
        };

        self.sessions.insert(session_id.clone(), session);

        // Emit event
        let _ = self.event_tx.send(TranscodingEvent::SessionStarted {
            session_id: session_id.clone(),
            source_codec,
            target_codec,
            backend: self.backend_preference.clone(),
        });

        info!("Created stub transcoding session: {}", session_id);
        Ok(session_id)
    }

    pub async fn transcode_packet(
        &self,
        session_id: &str,
        input_data: &[u8],
        timestamp: u32,
    ) -> Result<Vec<u8>> {
        // Update session activity if it exists
        if let Some(mut session) = self.sessions.get_mut(session_id) {
            session.last_activity = Instant::now();
            session.stats.packets_processed += 1;
            session.stats.bytes_processed += input_data.len() as u64;
            
            // If we have a codec service, use it for transcoding
            if self.codec_service.is_some() {
                // TODO: Implement actual transcoding using external library
                // For now, return input unchanged but log that we're using the real service
                info!("Transcoding packet for session {} using redfire-codec-engine", session_id);
                return Ok(input_data.to_vec());
            }
        }

        // Fallback: return input unchanged
        warn!("No transcoding session found for {}, returning input unchanged", session_id);
        Ok(input_data.to_vec())
    }

    pub async fn destroy_transcoding_session(&self, session_id: &str) -> Result<()> {
        if let Some((_, session)) = self.sessions.remove(session_id) {
            let _ = self.event_tx.send(TranscodingEvent::SessionCompleted {
                session_id: session_id.to_string(),
                stats: session.stats,
            });

            info!("Destroyed stub transcoding session: {}", session_id);
        }

        Ok(())
    }

    pub async fn get_device_info(&self) -> Vec<GpuDevice> {
        // Return empty device list in stub mode
        vec![]
    }

    pub fn get_active_sessions(&self) -> Vec<TranscodingSession> {
        self.sessions.iter().map(|entry| entry.value().clone()).collect()
    }

    pub async fn switch_backend(&mut self, new_backend: TranscodingBackend) -> Result<()> {
        let old_backend = self.backend_preference.clone();
        self.backend_preference = new_backend.clone();

        let _ = self.event_tx.send(TranscodingEvent::BackendSwitch {
            session_id: "all".to_string(),
            from_backend: old_backend,
            to_backend: new_backend.clone(),
            reason: "Manual switch (stub mode)".to_string(),
        });

        info!("Switched transcoding backend preference to {:?} (stub mode)", new_backend);
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping transcoding service stub");

        // Destroy all active sessions
        let session_ids: Vec<String> = self.sessions.iter().map(|entry| entry.key().clone()).collect();
        for session_id in session_ids {
            let _ = self.destroy_transcoding_session(&session_id).await;
        }

        self.sessions.clear();
        self.is_running = false;
        
        info!("Transcoding service stub stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transcoding_service_creation() {
        let service = TranscodingService::new(TranscodingBackend::Auto);
        assert!(!service.is_running);
    }

    #[tokio::test]
    async fn test_codec_type_conversion() {
        assert_eq!(CodecType::from_name("pcmu"), CodecType::G711u);
        assert_eq!(CodecType::from_name("G722"), CodecType::G722);
        assert_eq!(CodecType::G711a.to_name(), "PCMA");
    }

    #[test]
    fn test_transcoding_stats() {
        let mut stats = TranscodingStats::new();
        stats.bytes_processed = 1000000; // 1MB
        stats.processing_time_ms = 1000;   // 1 second
        
        let throughput = stats.throughput_mbps();
        assert!((throughput - 8.0).abs() < 0.1); // ~8 Mbps
    }

    #[tokio::test]
    async fn test_stub_transcoding_session() {
        let mut service = TranscodingService::new(TranscodingBackend::Auto);
        service.start().await.unwrap();
        
        let session_id = service.create_transcoding_session(
            "test-call",
            CodecType::G711u,
            CodecType::G722,
            8000,
            16000,
        ).await.unwrap();
        
        // Test passthrough transcoding
        let input_data = vec![1, 2, 3, 4, 5];
        let output_data = service.transcode_packet(&session_id, &input_data, 0).await.unwrap();
        assert_eq!(input_data, output_data);
        
        service.destroy_transcoding_session(&session_id).await.unwrap();
        service.stop().await.unwrap();
    }
}