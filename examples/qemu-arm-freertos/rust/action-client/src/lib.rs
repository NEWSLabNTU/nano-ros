//! FreeRTOS QEMU MPS2-AN385 Fibonacci action client —
//! Phase 212.L Node pkg.
//!
//! Phase 212.M.5.b — declarative-metadata-only.
//! Service-client runtime body deferred to M-F.4 (TickCtx call() seam) —
//! the same dependency applies to action-client send_goal /
//! feedback-stream wiring. The declarative metadata below is the stable
//! contract; the generated runtime will own the imperative driver once
//! the seam ships.

#![no_std]

// Phase 214.S.5.b — host-build shim. The Component pkg's `crate-type =
// ["rlib", "staticlib"]` triggers a panic-handler + global-allocator
// resolution at compile time. On the embedded FreeRTOS target
// (`thumbv7m-none-eabi`, `target_os = "none"`) those are supplied by the
// linked Entry pkg + board / nros-platform-freertos. On the host (Linux
// / macOS) — where `cargo check --features rmw-cyclonedds` runs as a
// build-sanity probe — neither is present, so the staticlib fails to
// resolve. Emit minimal abort stubs only when the target is a hosted OS
// so the standalone host build can complete; the firmware build path is
// unaffected (the symbols are `#[cfg]`-elided on the embedded target).
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod host_shim {
    use core::alloc::{GlobalAlloc, Layout};

    #[panic_handler]
    fn panic(_info: &core::panic::PanicInfo) -> ! {
        loop {
            core::hint::spin_loop();
        }
    }

    struct AbortAllocator;

    unsafe impl GlobalAlloc for AbortAllocator {
        unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
            core::ptr::null_mut()
        }
        unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
    }

    #[global_allocator]
    static HOST_ALLOC: AbortAllocator = AbortAllocator;
}

use example_interfaces::action::Fibonacci;
use nros::{
    CallbackCtx, CallbackId, EntityId, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
};

pub struct FibonacciClient;

impl Node for FibonacciClient {
    const NAME: &'static str = "fibonacci_action_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("fibonacci_action_client"))?;
        let _client =
            node.create_action_client::<Fibonacci>(EntityId::new("client_fib"), "/fibonacci")?;
        Ok(())
    }
}

impl ExecutableNode for FibonacciClient {
    type State = ();

    fn init() -> Self::State {}

    fn on_callback(
        _state: &mut Self::State,
        _callback: CallbackId<'_>,
        _ctx: &mut CallbackCtx<'_>,
    ) {
        // Phase 212.M.5.b — declarative-metadata-only.
        // Service-client runtime body deferred to M-F.4
        // (TickCtx call() seam). Codegen-system will own the imperative
        // driver once the seam ships.
    }
}

nros::node!(FibonacciClient);
