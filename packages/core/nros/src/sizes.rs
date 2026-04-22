//! Single source of truth for FFI opaque-storage sizes.
//!
//! Each `export_size!` invocation produces two artefacts:
//!
//! * `pub const FOO_SIZE: usize = core::mem::size_of::<T>();` — a normal
//!   Rust const suitable for in-crate `const _: () = assert!(...)` checks and
//!   direct use by `no_std` consumers.
//! * `pub static __NROS_SIZE_FOO: [u8; FOO_SIZE] = [0; FOO_SIZE];` — an
//!   array-sized static whose *symbol storage size* in the compiled rlib
//!   equals `FOO_SIZE`. `nros-c`/`nros-cpp` build scripts read the sizes out
//!   via [`nros_sizes_build::extract_sizes`](../../../nros-sizes-build/index.html)
//!   to derive opaque-storage macros for the generated C/C++ headers.
//!
//! Feature gating follows the rest of the crate: the statics only exist when
//! an RMW backend (`rmw-zenoh` / `rmw-xrce` / `rmw-dds` / `rmw-cffi`) is
//! active, which is exactly the condition under which the `Rmw*` type
//! aliases resolve. Workspace-level `cargo check` without any RMW feature
//! sees this module as empty.

#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
mod rmw_sizes {
    use crate::internals::{
        RmwPublisher, RmwServiceClient, RmwServiceServer, RmwSession, RmwSubscriber,
    };

    macro_rules! export_size {
        ($vis:vis $name:ident = $ty:ty) => {
            $vis const $name: usize = core::mem::size_of::<$ty>();
            paste::paste! {
                #[used]
                #[unsafe(no_mangle)]
                #[doc(hidden)]
                pub static [<__NROS_SIZE_ $name>]: [u8; $name] = [0u8; $name];
            }
        };
    }

    export_size!(pub SESSION_SIZE        = RmwSession);
    export_size!(pub PUBLISHER_SIZE      = RmwPublisher);
    export_size!(pub SUBSCRIBER_SIZE     = RmwSubscriber);
    export_size!(pub SERVICE_CLIENT_SIZE = RmwServiceClient);
    export_size!(pub SERVICE_SERVER_SIZE = RmwServiceServer);
    export_size!(pub EXECUTOR_SIZE       = nros_node::Executor);
    export_size!(pub GUARD_CONDITION_SIZE = nros_node::GuardConditionHandle);
    export_size!(pub LIFECYCLE_CTX_SIZE  = nros_node::lifecycle::LifecyclePollingNodeCtx);

    // Layout-mirror struct for `nros_c::action::ActionServerInternal`.
    // ActionServerInternal lives in the `nros-c` crate (it embeds C-API
    // pointer types like `*mut nros_action_server_t`), so it can't be
    // referenced from `nros` directly. This mirror has the same `#[repr(C)]`
    // field shape — `*mut c_void` and `unsafe extern "C" fn(*mut c_void, ...)`
    // pointer slots — and therefore the same byte size, since fn-pointer
    // size is independent of parameter types. nros-c asserts at compile
    // time that `size_of::<ActionServerInternal>() ==
    // size_of::<ActionServerInternalLayout>()`.
    use core::ffi::c_void;
    type CGoalCallbackLayout =
        unsafe extern "C" fn(*mut c_void, *const c_void, *const u8, usize, *mut c_void) -> i32;
    type CCancelCallbackLayout =
        Option<unsafe extern "C" fn(*const c_void, i32, *mut c_void) -> i32>;
    type CAcceptedCallbackLayout =
        Option<unsafe extern "C" fn(*mut c_void, *const c_void, *mut c_void)>;

    #[repr(C)]
    #[doc(hidden)]
    pub struct ActionServerInternalLayout {
        pub handle: nros_node::ActionServerRawHandle,
        pub executor_ptr: *mut c_void,
        pub c_goal_callback: CGoalCallbackLayout,
        pub c_cancel_callback: CCancelCallbackLayout,
        pub c_accepted_callback: CAcceptedCallbackLayout,
        pub c_context: *mut c_void,
        pub server_ptr: *mut c_void,
    }
    export_size!(pub ACTION_SERVER_INTERNAL_SIZE = ActionServerInternalLayout);

    // Layout-mirrors for nros-cpp's `CppActionServer` and `CppActionClient`.
    //
    // Same approach as `ActionServerInternalLayout` above: nros-cpp's
    // wrapper structs live in a downstream crate but their byte sizes can
    // be reconstructed from the field shape. This eliminates the
    // hand-math in `nros-cpp/build.rs` (was Phase 87.11).
    //
    // The C++-side `nros::ActionServer<A>` / `nros::ActionClient<A>`
    // classes hold opaque storage sized to these probe values. nros-cpp
    // asserts `size_of::<CppActionServer>() == size_of::<CppActionServerLayout>()`
    // (and the same for CppActionClient) so any field-shape drift in the
    // real wrapper trips the build immediately.

    type CppGoalCallbackLayout =
        unsafe extern "C" fn(*const [u8; 16], *const u8, usize, *mut c_void) -> i32;
    type CppCancelCallbackLayout = unsafe extern "C" fn(*const [u8; 16], *mut c_void) -> i32;

    // Phase 87.6 thin-wrapper: `action_name` / `type_name` / `type_hash`
    // buffers moved to the C++ `nros::ActionServer<A>` class.
    #[repr(C)]
    #[doc(hidden)]
    pub struct CppActionServerLayout {
        pub handle: Option<nros_node::ActionServerRawHandle>,
        pub goal_cb: Option<CppGoalCallbackLayout>,
        pub cancel_cb: Option<CppCancelCallbackLayout>,
        pub cb_ctx: *mut c_void,
    }
    export_size!(pub CPP_ACTION_SERVER_SIZE = CppActionServerLayout);

    type CppActionGoalResponseCb = Option<unsafe extern "C" fn(bool, *const [u8; 16], *mut c_void)>;
    type CppActionFeedbackCb =
        Option<unsafe extern "C" fn(*const [u8; 16], *const u8, usize, *mut c_void)>;
    type CppActionResultCb =
        Option<unsafe extern "C" fn(*const [u8; 16], i32, *const u8, usize, *mut c_void)>;

    #[repr(C)]
    #[doc(hidden)]
    pub struct CppActionClientCallbacksLayout {
        pub goal_response: CppActionGoalResponseCb,
        pub feedback: CppActionFeedbackCb,
        pub result: CppActionResultCb,
        pub context: *mut c_void,
    }

    // Phase 87.6 thin-wrapper: `action_name` buffer moved to the C++
    // `nros::ActionClient<A>` class.
    #[repr(C)]
    #[doc(hidden)]
    pub struct CppActionClientLayout {
        pub callbacks: CppActionClientCallbacksLayout,
        pub arena_entry_index: i32,
        pub executor_ptr: *mut c_void,
    }
    export_size!(pub CPP_ACTION_CLIENT_SIZE = CppActionClientLayout);
}

#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub use rmw_sizes::*;
