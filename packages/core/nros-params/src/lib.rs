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
//! use nros_params::{
//!     ParameterDescriptor, ParameterServer, ParameterStorage, ParameterType, ParameterValue,
//! };
//!
//! // phase-382 W2' — storage is CALLER-OWNED: place it (a `static`, a struct
//! // field, a local), then lend it. `ParameterStorage` defaults to
//! // `MAX_PARAMETERS` slots; any length works, and the server's capacity is
//! // whatever it is handed.
//! let mut storage = ParameterStorage::<8>::new();
//! let mut server = ParameterServer::new_in(storage.as_table());
//!
//! // Declare a simple parameter
//! server.declare("max_speed", ParameterValue::Double(1.0));
//!
//! // Declare a parameter with constraints
//! let desc = ParameterDescriptor::new("velocity", ParameterType::Double)
//!     .unwrap()
//!     .with_description("Maximum velocity in m/s")
//!     .with_float_range(0.0, 10.0, 0.1);
//! server.declare_with_descriptor("velocity", ParameterValue::Double(5.0), Some(desc));
//!
//! // Get and set parameters
//! assert_eq!(server.get_double("max_speed"), Some(1.0));
//! server.set_double("max_speed", 2.0);
//! ```
//!
//! # Features
//!
//! - `std` - Enable standard library support
//! - `alloc` - Enable heap allocation

#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub(crate) mod config;
// phase-359 W10 / issue 0080 — `persist` is GONE. It held the parameter
// PERSISTENCE seam (`ParamStore`, `NullParamStore`, `ParamStoreError`,
// `FileParamStore`), which 0080 ruled a non-goal in July: nano-ros does not
// persist parameters on-device, and launch-baked defaults are the supported
// model. Runtime get/set/describe — the `server` module — stay.
pub mod server;
pub mod typed;
pub mod types;

// Re-export main types
pub use server::{LegacyParameterBuilder, ParameterServer, ParameterStorage, ParameterTable};
pub use typed::{
    MandatoryParameter, OptionalParameter, ParameterBuilder, ParameterError, RangeConvertible,
    ReadOnlyParameter, UndeclaredParameters,
};
pub use types::{
    FloatingPointRange, IntegerRange, MAX_ARRAY_LEN, MAX_BYTE_ARRAY_LEN, MAX_PARAM_NAME_LEN,
    MAX_PARAMETERS, MAX_STRING_VALUE_LEN, Parameter, ParameterDescriptor, ParameterRange,
    ParameterType, ParameterValue, ParameterVariant, SetParameterResult,
};
