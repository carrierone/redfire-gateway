//! Error handling for the Redfire Gateway


pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Configuration error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("SIP error: {0}")]
    Sip(String),

    #[error("TDM error: {0}")]
    Tdm(String),

    #[error("RTP error: {0}")]
    Rtp(String),

    #[error("SNMP error: {0}")]
    Snmp(String),

    #[error("Audio codec error: {0}")]
    Codec(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Not supported: {0}")]
    NotSupported(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Performance monitoring error: {0}")]
    Performance(String),

    #[error("Alarm system error: {0}")]
    Alarm(String),

    #[error("Test failure: {0}")]
    Test(String),

    #[error("FreeTDM error: {0}")]
    FreeTdm(String),

    #[error("B2BUA error: {0}")]
    B2bua(String),

    #[error("Clustering error: {0}")]
    Clustering(String),

    #[error("Transcoding error: {0}")]
    Transcoding(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl Error {
    pub fn network<S: Into<String>>(msg: S) -> Self {
        Self::Network(msg.into())
    }

    pub fn protocol<S: Into<String>>(msg: S) -> Self {
        Self::Protocol(msg.into())
    }

    pub fn sip<S: Into<String>>(msg: S) -> Self {
        Self::Sip(msg.into())
    }

    pub fn tdm<S: Into<String>>(msg: S) -> Self {
        Self::Tdm(msg.into())
    }

    pub fn rtp<S: Into<String>>(msg: S) -> Self {
        Self::Rtp(msg.into())
    }

    pub fn timeout<S: Into<String>>(msg: S) -> Self {
        Self::Timeout(msg.into())
    }

    pub fn invalid_state<S: Into<String>>(msg: S) -> Self {
        Self::InvalidState(msg.into())
    }

    pub fn not_supported<S: Into<String>>(msg: S) -> Self {
        Self::NotSupported(msg.into())
    }

    pub fn parse<S: Into<String>>(msg: S) -> Self {
        Self::Parse(msg.into())
    }

    pub fn b2bua<S: Into<String>>(msg: S) -> Self {
        Self::B2bua(msg.into())
    }

    pub fn clustering<S: Into<String>>(msg: S) -> Self {
        Self::Clustering(msg.into())
    }

    pub fn transcoding<S: Into<String>>(msg: S) -> Self {
        Self::Transcoding(msg.into())
    }

    pub fn internal<S: Into<String>>(msg: S) -> Self {
        Self::Internal(msg.into())
    }
}