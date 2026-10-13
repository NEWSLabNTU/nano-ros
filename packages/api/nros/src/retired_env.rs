//! phase-484 W8c — the RETIRED runtime env aliases and their canonical names.
//!
//! ONE table for both hosted env readers (`crate::env` and the `getenv` twin
//! `crate::host_env`), so the two cannot disagree about which names refuse.
//! A set retired name is [`BootConfigError::RetiredEnvVar`], never a silent
//! skip: an ignored `ZENOH_LOCATOR` dials the backend's DEFAULT router, which
//! reads as "no delivery" rather than as a configuration mistake.
//!
//! The gate `check-retired-runtime-aliases` refuses any other tracked reader
//! of these names, so this file is the one place they are spelled in code.
//!
//! [`BootConfigError::RetiredEnvVar`]: nros_node::BootConfigError::RetiredEnvVar

/// `(retired, replacement)` pairs, in the order the readers check them.
pub const RETIRED_ENV_VARS: &[(&str, &str)] = &[
    ("ZENOH_LOCATOR", "NROS_LOCATOR"),
    ("ZENOH_MODE", "NROS_SESSION_MODE"),
];
