//! Physical CCD Glitch Art - library crate.
//!
//! Provides the CCD simulation pipeline and SPICE circuit modules
//! for use by the main application and test binaries.

pub mod ccd;
pub mod color;
pub mod glitch;
pub mod image_io;
pub mod pipeline;

#[cfg(feature = "spice")]
pub mod spice;
