//! Network interfaces for the Redfire Gateway

pub mod tdmoe;
pub mod freetdm;

pub use tdmoe::TdmoeInterface;
pub use freetdm::FreeTdmInterface;