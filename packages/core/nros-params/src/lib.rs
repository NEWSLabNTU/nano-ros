//! Parameter server for nros
//!
//! This crate provides a ROS 2 compatible parameter server for embedded systems.
//! Parameters live in storage the CALLER places and lends to the server
//! ([`ParameterStorage`] / [`ParameterTable`]); [`MAX_PARAMETERS`] is only the
//! default capacity of that storage.
//!
//! # Example
//!
//! ```
//! // phase-515 R10: the store's operations are behind `ParameterServerOps`;
//! // a `nros` user reaches parameters through its `Node` instead.
//! use nros_params::{
//!     NodeKey, ParameterDescriptor, ParameterServer, ParameterServerOps as _, ParameterStorage,
//!     ParameterType, ParameterValue,
//! };
//!
//! // phase-382 W2' — storage is CALLER-OWNED: place it (a `static`, a struct
//! // field, a local), then lend it. `ParameterStorage` defaults to
//! // `MAX_PARAMETERS` slots; any length works, and the server's capacity is
//! // whatever it is handed.
//! let mut storage = ParameterStorage::<8>::new();
//! let mut server = ParameterServer::new_in(storage.as_table());
//!
//! // phase-426 W1 — every per-parameter call names the NODE it is about, so
//! // two nodes composed onto one executor never share a name.
//! let node = NodeKey::PRIMARY;
//!
//! // Declare a simple parameter
//! server.declare(node, "max_speed", ParameterValue::Double(1.0));
//!
//! // Declare a parameter with constraints
//! let desc = ParameterDescriptor::new("velocity", ParameterType::Double)
//!     .unwrap()
//!     .with_description("Maximum velocity in m/s")
//!     .with_float_range(0.0, 10.0, 0.1);
//! server.declare_with_descriptor(node, "velocity", ParameterValue::Double(5.0), Some(desc));
//!
//! // Get and set parameters
//! assert_eq!(server.get_double(node, "max_speed"), Some(1.0));
//! server.set_double(node, "max_speed", 2.0);
//! ```
//!
//! # Features
//!
//! - `alloc` - Enable heap allocation

#![no_std]

// phase-359 W10 — no `std` feature; the test harness is hosted.
#[cfg(test)]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub(crate) mod config;
// phase-359 W10 / issue 0080 — `persist` is GONE. It held the parameter
// PERSISTENCE seam (`ParamStore`, ~~`NullParamStore`~~, ~~`ParamStoreError`~~,
// ~~`FileParamStore`~~), which 0080 ruled a non-goal in July: nano-ros does not
// persist parameters on-device, and launch-baked defaults are the supported
// model. Runtime get/set/describe — the `server` module — stay.
// phase-446 W6 -- what each node's contract declares, looked up when the code
// declares a parameter.
pub mod declared;
pub mod ops;
pub mod server;
pub mod typed;
pub mod types;

// Re-export main types
pub use declared::{DeclaredParamMismatch, DeclaredParams};
pub use ops::ParameterServerOps;
pub use server::{
    DeclarationError, LegacyParameterBuilder, MAX_ON_SET_CALLBACKS, OnSetContext, OnSetParameterFn,
    OnSetParameterHandle, PARAMETER_EVENT_LOG_LEN, ParameterEventKind, ParameterServer,
    ParameterSlot, ParameterStorage, ParameterTable, PendingParameterEvent,
};
#[allow(deprecated)]
pub use typed::UndeclaredParameters;
pub use typed::{
    MandatoryParameter, OptionalParameter, ParameterBuilder, ParameterError, Parameters,
    RangeConvertible, ReadOnlyParameter,
};
/// Deprecated (phase-515): the store answers `Result<(), ParameterError>`.
#[allow(deprecated)]
pub use types::SetParameterResult;
pub use types::{
    FloatingPointRange, IMPLIED_STORE_SLOTS, IntegerRange, MAX_ARRAY_LEN, MAX_BYTE_ARRAY_LEN,
    MAX_PARAM_CONSTRAINTS_LEN, MAX_PARAM_DESCRIPTION_LEN, MAX_PARAM_NAME_LEN, MAX_PARAMETERS,
    MAX_STRING_VALUE_LEN, NodeFlags, NodeKey, PARAM_EVENTS_DEPTH, Parameter, ParameterDescriptor,
    ParameterRange, ParameterType, ParameterValue, ParameterVariant, fit_description,
};
