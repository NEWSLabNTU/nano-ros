//! Collectors — one per backend-native artifact format.
//!
//! Each collector discovers artifact files under a project directory and parses
//! them into [`RawUnit`]s plus a backend hint and non-fatal notes. A collector
//! that finds nothing returns an empty [`Collected`] (never an error) so a
//! partial profile still renders.

pub mod cargo;
pub mod ninja;

use crate::model::{Backend, RawUnit};

/// Output of a single collector run.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Collected {
    pub units: Vec<RawUnit>,
    /// Backend identity if this collector could determine it.
    pub backend: Option<Backend>,
    /// `true` when per-unit timing (not just wall-clock) was captured.
    pub deep: bool,
    pub notes: Vec<String>,
}

impl Collected {
    /// `true` when no artifacts were found.
    pub fn is_empty(&self) -> bool {
        self.units.is_empty() && self.backend.is_none()
    }
}
