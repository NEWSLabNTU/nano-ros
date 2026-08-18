//! Phase 172.H — runtime parameter-override persistence.
//!
//! A [`ParamStore`] persists parameter values set at runtime (via the
//! `set_parameters` service) so they survive a restart. At boot the generated
//! runtime declares the plan's compile-time defaults, then [`ParamStore::load`]
//! overlays any persisted overrides; after a successful runtime set the
//! executor flushes the full parameter set back via [`ParamStore::save`].
//!
//! [`NullParamStore`] is the no-op default (no persistence). [`FileParamStore`]
//! (`std` only) persists scalars to a text file — the hosted backend. Flash /
//! NVS backends for embedded targets are future work.

use crate::types::ParameterValue;

/// Error from a [`ParamStore`] backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamStoreError {
    /// The backend (filesystem, flash, …) reported a failure.
    Backend,
    /// Persisted data was malformed.
    Format,
}

/// Backend that persists runtime parameter overrides across restarts (172.H).
///
/// Object-safe so the executor can hold a `Box<dyn ParamStore>`.
pub trait ParamStore {
    /// Apply each persisted `(name, value)` via `apply`. Called once at boot
    /// after defaults are declared, so persisted values override them.
    fn load(&self, apply: &mut dyn FnMut(&str, ParameterValue));

    /// Persist the full current parameter set. `params` yields `(name, value)`
    /// for every declared parameter; called after a runtime set changes a
    /// value. Non-scalar values (arrays, `NotSet`) are backend-defined and may
    /// be skipped.
    fn save(
        &mut self,
        params: &mut dyn Iterator<Item = (&str, &ParameterValue)>,
    ) -> Result<(), ParamStoreError>;
}

/// No-op store: the default when persistence is disabled.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullParamStore;

impl ParamStore for NullParamStore {
    fn load(&self, _apply: &mut dyn FnMut(&str, ParameterValue)) {}

    fn save(
        &mut self,
        _params: &mut dyn Iterator<Item = (&str, &ParameterValue)>,
    ) -> Result<(), ParamStoreError> {
        Ok(())
    }
}

// phase-359 W10 / issue 0080 — `FileParamStore` is GONE, and with it this
// crate's last need for `std`.
//
// It was the hosted backend of a persistence seam that issue 0080 ruled a
// NON-GOAL on 2026-07-10: nano-ros does not persist parameters on-device, and
// launch-baked defaults are the supported model. 0080 lists this type in its
// own follow-up cleanup. It had no constructor call anywhere in the tree —
// `FileParamStore::new` was never invoked outside its tests — so what stood
// here was a filesystem-backed implementation of a feature nobody could reach
// and nobody intends to finish.
//
// The SEAM stays: `ParamStore` and `NullParamStore` are still what the
// executor holds (`store: Box<dyn ParamStore>`, defaulting to the no-op), and
// removing those reaches the executor's public API and the dormant codegen
// path — the rest of 0080's optional cleanup, deliberately not bundled here.
