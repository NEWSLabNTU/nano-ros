//! FreeRTOS QEMU MPS2-AN385 Listener — Phase 212.L Node pkg.
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` and tracks the last seen
//! value. The BSP-generated runtime owns init / executor / spin.

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

use nros::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult};
use std_msgs::msg::Int32;

pub struct Listener;

impl Node for Listener {
    const NAME: &'static str = "listener";

    fn register(ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        let mut node = ctx.create_node(NodeOptions::new("listener"))?;
        let _sub = node.create_subscription_for_callback_name::<Int32>("on_chatter", "/chatter")?;
        Ok(())
    }
}

impl ExecutableNode for Listener {
    /// Last value seen on `/chatter`.
    type State = i32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
        if callback.as_str() == "on_chatter" {
            if let Ok(msg) = ctx.message::<Int32>() {
                *state = msg.data;
            }
        }
    }
}

nros::node!(Listener);
