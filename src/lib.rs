//! Redfire Gateway - TDMoE to SIP Gateway
//! 
//! A comprehensive telecommunications gateway that bridges legacy TDM infrastructure
//! with modern SIP/VoIP systems, supporting various protocols and standards.
//!
//! **Sponsored by [Carrier One Inc](https://carrierone.com) - Professional Telecommunications Solutions**

pub mod config;
pub mod core;
pub mod protocols;
pub mod interfaces;
pub mod services;
pub mod testing;
pub mod error;
pub mod utils;

pub use error::{Error, Result};

/// Gateway version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NAME: &str = env!("CARGO_PKG_NAME");
pub const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");