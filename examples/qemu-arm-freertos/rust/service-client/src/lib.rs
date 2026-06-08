//! FreeRTOS QEMU MPS2-AN385 AddTwoInts service client —
//! Phase 212.L Node pkg.
//!
//! Phase 212.M.5.b — declarative-metadata-only.
//! Service-client runtime body deferred to M-F.4 (TickCtx call() seam).
//!
//! The component model expresses *what* entities exist; the imperative
//! call sequencing (issue request → await reply) currently has no
//! `TickCtx` seam — service-client invocation is a follow-up wave for
//! the generated runtime. The body is a declarative no-op stub.

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

use example_interfaces::srv::AddTwoInts;
use nros::{
    CallbackCtx, CallbackId, EntityId, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};

pub struct AddTwoIntsClient;

impl Node for AddTwoIntsClient {
    const NAME: &'static str = "add_two_ints_client";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("add_two_ints_client"))?;
        let _client =
            node.create_service_client::<AddTwoInts>(EntityId::new("client_add"), "/add_two_ints")?;
        let _timer = node.create_timer(
            EntityId::new("timer_call"),
            CallbackId::new("issue_call"),
            TimerDuration::from_secs(1),
        )?;
        Ok(())
    }
}

impl ExecutableNode for AddTwoIntsClient {
    /// Index into the canned test-case table for the next call.
    type State = u8;

    fn init() -> Self::State {
        0
    }

    fn on_callback(
        _state: &mut Self::State,
        _callback: CallbackId<'_>,
        _ctx: &mut CallbackCtx<'_>,
    ) {
        // Phase 212.M.5.b — declarative-metadata-only.
        // Service-client runtime body deferred to M-F.4
        // (TickCtx call() seam). Codegen-system will wire the imperative
        // call loop here once the seam ships; the declarative metadata
        // above is the stable contract.
    }
}

nros::node!(AddTwoIntsClient);
