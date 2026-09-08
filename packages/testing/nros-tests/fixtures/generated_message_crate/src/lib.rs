//! Issue 1230 — the crate `rosidl-codegen`'s emitted message code is compiled
//! into, at the BUILD stage.
//!
//! Four tests in `packages/cli/rosidl-codegen/tests/compilation_test.rs` used
//! to build this crate by hand into a `tempfile` tree and run `cargo check` /
//! `cargo clippy` on it while the test process waited. Their four intents are
//! all still here, and two of them are now stronger, because a crate-level
//! attribute applies to every message rather than to the one the test that
//! carried it generated:
//!
//! | old test | what it asserted | where it lives now |
//! | --- | --- | --- |
//! | `test_simple_message_compiles` | `SimpleMsg` type-checks | `msgs/SimpleMsg.msg` + this crate compiling |
//! | `test_message_with_arrays_compiles` | `[5]` / `[32]` arrays type-check under serde | `msgs/ArrayMsg.msg` |
//! | `test_check_no_warnings` | emitted code is warning-free | `#![deny(warnings)]` below, over ALL four |
//! | `test_clippy_no_warnings` | emitted code is clippy-clean | builder `cargo-clippy` + `#![deny(clippy::all)]`, over ALL four |
//!
//! The old clippy test also could not fail the way its name promised: it
//! grepped clippy's stderr for the substring `"error"`, so a clippy WARNING —
//! which is what `-W clippy::all` produces and what "clippy is clean" is about
//! — passed. Denying the lints in the crate under check is what makes the
//! verdict real.
//!
//! `deny(warnings)` is deliberate here and would not be in shipped code: a new
//! rustc release can add a lint and turn this red without a repo change. That
//! is the cost of the assertion, and it is the assertion the four tests were
//! written for — the same trade every `-D warnings` gate in this repo makes.

#![deny(warnings)]
#![deny(clippy::all)]

// Stub `rosidl_runtime_rs` — the ros2_rust-shaped runtime surface the
// `generate_message_package` emitter targets. Carried over VERBATIM from
// `compilation_test.rs::create_rosidl_runtime_stub`, which is why it declares
// traits nothing here implements: it is the shape the emitted code names, not a
// runtime. (The nros-shaped emitters — `generate_nros_message_package` and
// friends, the ones the `nros` CLI actually calls — compile against the REAL
// `nros-core`/`nros-serdes` in every workspace fixture, so they need no stub.)
pub mod rosidl_runtime_rs {
    use serde::{Deserialize, Serialize};

    pub type String = std::string::String;
    pub type WString = std::string::String;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct BoundedString<const N: usize>(std::string::String);

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct BoundedWString<const N: usize>(std::string::String);

    #[repr(C)]
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct Sequence<T>(Vec<T>);

    #[repr(C)]
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct BoundedSequence<T, const N: usize>(Vec<T>);

    impl<T> Default for Sequence<T> {
        fn default() -> Self {
            Sequence(Vec::new())
        }
    }

    impl<T, const N: usize> Default for BoundedSequence<T, N> {
        fn default() -> Self {
            BoundedSequence(Vec::new())
        }
    }

    impl<const N: usize> Default for BoundedString<N> {
        fn default() -> Self {
            BoundedString(std::string::String::new())
        }
    }

    impl<const N: usize> Default for BoundedWString<N> {
        fn default() -> Self {
            BoundedWString(std::string::String::new())
        }
    }

    // Trait definitions for ROS runtime
    pub trait SequenceElement: Sized {
        type RmwType;
    }

    pub trait SequenceAlloc: Sized {
        fn sequence_init(seq: &mut Sequence<Self>, size: usize) -> bool;
        fn sequence_fini(seq: &mut Sequence<Self>);
        fn sequence_copy(in_seq: &Sequence<Self>, out_seq: &mut Sequence<Self>) -> bool;
    }

    pub trait Message: Clone {
        type RmwMsg: Clone;
        fn into_rmw_message(
            msg_cow: std::borrow::Cow<'_, Self>,
        ) -> std::borrow::Cow<'_, Self::RmwMsg>;
        fn from_rmw_message(msg: Self::RmwMsg) -> Self;
    }

    pub trait RmwMessage: Sized {
        const TYPE_NAME: &'static str;
        fn get_type_support() -> *const std::ffi::c_void;
    }

    pub trait Service {
        type Request;
        type Response;
        fn get_type_support() -> *const std::ffi::c_void;
    }

    pub trait Action {
        type Goal;
        type Result;
        type Feedback;
        type FeedbackMessage;
        type SendGoalService;
        type CancelGoalService;
        type GetResultService;
        fn get_type_support() -> *const std::ffi::c_void;
    }
}

/// Every `msgs/*.msg`, emitted by `build.rs` into one module.
///
/// The name is not a choice: the emitted code writes `crate::msg::rmw::<Message>`
/// and `crate::rosidl_runtime_rs::<Trait>` as ABSOLUTE paths, so `msg` at the
/// crate root and the stub above are what make them resolve.
pub mod msg {
    include!(concat!(env!("OUT_DIR"), "/messages.rs"));
}
