//! Build-time configurable constants.
//!
//! Values are set via environment variables at build time.
//! See build.rs for env var names and defaults.

include!(concat!(env!("OUT_DIR"), "/zpico_smoltcp_config.rs"));
