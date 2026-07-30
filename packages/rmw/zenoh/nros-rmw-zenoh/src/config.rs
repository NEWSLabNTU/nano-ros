//! Build-time configurable constants.
//!
//! Values are set via environment variables at build time.
//! See build.rs for env var names and defaults.

include!(concat!(env!("OUT_DIR"), "/buffer_config.rs"));
