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
}

#[cfg(any(
    feature = "rmw-zenoh",
    feature = "rmw-xrce",
    feature = "rmw-dds",
    feature = "rmw-cffi"
))]
pub use rmw_sizes::*;
