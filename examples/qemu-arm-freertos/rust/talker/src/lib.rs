//! FreeRTOS QEMU MPS2-AN385 Talker — Phase 212.L Node pkg.
//!
//! Publishes `std_msgs/Int32` on `/chatter` once per second.
//!
//! Node pkg shape: `register()` declares node + publisher + timer;
//! `ExecutableNode::on_callback("on_tick")` runs the timer body
//! (bump counter, publish). The BSP-generated runtime (M.5.a.3+4 owns
//! `nros::init`, executor open, RMW registration, and the spin loop.
//! The user authors *only* the declarative + body bits.

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

use nros::{
    CallbackCtx, CallbackId, EntityId, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};
use std_msgs::msg::Int32;

/// Talker component — counter state + chatter publish on every tick.
pub struct Talker;

impl Node for Talker {
    const NAME: &'static str = "talker";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("talker"))?;
        let _pub = node.create_publisher::<Int32>(EntityId::new("pub_chatter"), "/chatter")?;
        let _timer = node.create_timer(
            EntityId::new("timer_tick"),
            CallbackId::new("on_tick"),
            TimerDuration::from_millis(1000),
        )?;
        node.callback(CallbackId::new("on_tick"))
            .publishes(EntityId::new("pub_chatter"))?;
        Ok(())
    }
}

impl ExecutableNode for Talker {
    /// Monotonic counter — the next int32 to publish.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_tick" {
            let msg = Int32 { data: *state };
            let _ = ctx.publish::<Int32, 64>(EntityId::new("pub_chatter"), &msg);
            *state = state.wrapping_add(1);
        }
    }
}

nros::node!(Talker);
