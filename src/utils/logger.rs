//! Logging configuration for the Redfire Gateway

use std::path::Path;

use tracing::{info, Level};
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

use crate::config::{LoggingConfig, LogFormat};
use crate::Result;

/// Setup logging based on configuration
pub fn setup_logging(config: &LoggingConfig) -> Result<()> {
    let level = parse_log_level(&config.level)?;
    
    let env_filter = EnvFilter::builder()
        .with_default_directive(level.into())
        .from_env_lossy();

    let registry = tracing_subscriber::registry().with(env_filter);

    match &config.file {
        Some(file_path) => {
            // File logging with rotation
            let file_path = Path::new(file_path);
            let directory = file_path.parent()
                .ok_or_else(|| crate::Error::parse("Invalid log file path"))?;
            let _filename = file_path.file_name()
                .ok_or_else(|| crate::Error::parse("Invalid log filename"))?;
            
            let file_appender = rolling::RollingFileAppender::builder()
                .rotation(rolling::Rotation::DAILY)
                .filename_suffix("log")
                .build(directory)
                .map_err(|e| crate::Error::internal(format!("Failed to create file appender: {}", e)))?;
            
            let (file_writer, _file_guard) = non_blocking(file_appender);
            
            let file_layer = match config.format {
                LogFormat::Json => fmt::layer()
                    .json()
                    .with_writer(file_writer)
                    .boxed(),
                LogFormat::Compact => fmt::layer()
                    .compact()
                    .with_writer(file_writer)
                    .boxed(),
                LogFormat::Full => fmt::layer()
                    .with_writer(file_writer)
                    .boxed(),
            };
            
            // Console logging
            let console_layer = match config.format {
                LogFormat::Json => fmt::layer()
                    .json()
                    .with_writer(std::io::stdout)
                    .boxed(),
                LogFormat::Compact => fmt::layer()
                    .compact()
                    .with_writer(std::io::stdout)
                    .boxed(),
                LogFormat::Full => fmt::layer()
                    .with_writer(std::io::stdout)
                    .boxed(),
            };
            
            registry
                .with(file_layer)
                .with(console_layer)
                .init();
        }
        None => {
            // Console logging only
            let console_layer = match config.format {
                LogFormat::Json => fmt::layer().json().boxed(),
                LogFormat::Compact => fmt::layer().compact().boxed(),
                LogFormat::Full => fmt::layer().boxed(),
            };
            
            registry.with(console_layer).init();
        }
    }

    info!("Logging initialized with level: {}", config.level);
    Ok(())
}

fn parse_log_level(level: &str) -> Result<Level> {
    match level.to_lowercase().as_str() {
        "trace" => Ok(Level::TRACE),
        "debug" => Ok(Level::DEBUG),
        "info" => Ok(Level::INFO),
        "warn" => Ok(Level::WARN),
        "error" => Ok(Level::ERROR),
        _ => Err(crate::Error::parse("Invalid log level")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_log_level() {
        assert_eq!(parse_log_level("info").unwrap(), Level::INFO);
        assert_eq!(parse_log_level("DEBUG").unwrap(), Level::DEBUG);
        assert_eq!(parse_log_level("Error").unwrap(), Level::ERROR);
        assert!(parse_log_level("invalid").is_err());
    }
}