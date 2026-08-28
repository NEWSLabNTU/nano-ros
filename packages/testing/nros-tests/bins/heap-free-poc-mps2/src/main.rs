//! phase-391 W5-endgame (issues 0816/0843) — the heap-free-tier image.
//!
//! Calls the REAL runtime path an embedded component image takes —
//! `Executor::open_in` over a caller-supplied static backing, then the
//! `install_node_typed_in` seam over a per-class `ComponentSlotStorage` — and
//! links with **no allocation symbol at all**: no `#[global_allocator]`, no
//! `__rust_alloc` shim, no `malloc`, no `nros_platform_alloc`. The W1 gate
//! (`scripts/check-no-alloc-image.py --tier heap-free`) asserts that on the
//! built ELF; three earlier probes passed it vacuously at `symbols read: 1`
//! because they only NAMED types — this image is the non-vacuous replacement.
//!
//! No RMW backend is linked (every in-tree transport allocates in C — zenoh's
//! `z_malloc`, XRCE's wrapper `malloc`, cyclone's `ddsrt_malloc` — so a
//! transport image is `unified` tier at best). `Executor::open_in` therefore
//! returns `NoBackend` at RUNTIME, which the image reports as its success
//! marker: the LINK property is the property under test, and the runtime
//! marker proves the code path executed to the open call rather than being
//! optimized away. The install + spin arm stays linked because the open's
//! outcome is opaque to LLVM (it reads the cffi backend registry).

#![no_std]
#![no_main]

use core::mem::MaybeUninit;

use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};
use nros::{
    BootConfig, ComponentSlotStorage, EntityBounds, ExecutorConfig, ExecutorSizing,
    node::{Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeResult, TickCtx},
};
use panic_semihosting as _;

// Force-link the per-platform export so `nros_platform_log_write` resolves
// (the logging-smoke pattern — staticlib DCE drops the rlib otherwise).
extern crate nros_platform_mps2_an385 as _;

/// A component that declares NOTHING — the seam is under test, not a
/// transport. Exact zero bounds, so its per-class cell registries and ctx
/// slabs are all zero-length: the whole static store is a few words.
struct PocNode;

impl Node for PocNode {
    const NAME: &'static str = "heap_free_poc";
    const ENTITY_BOUNDS: EntityBounds = EntityBounds::exact(0, 0, 0, 0, 0);

    fn register(_ctx: &mut NodeContext<'_>) -> NodeResult<()> {
        Ok(())
    }
}

impl ExecutableNode for PocNode {
    type State = u32;

    fn init() -> Self::State {
        0
    }

    fn on_callback(_state: &mut Self::State, _cb: Callback<'_>, _ctx: &mut CallbackCtx<'_>) {}

    fn tick(state: &mut Self::State, _ctx: &mut TickCtx<'_>) {
        *state = state.wrapping_add(1);
    }
}

/// The per-class slot storage the macro would emit — spelled by hand here so
/// the image does not need `nros::main!`'s board scaffold (whose
/// `ExecutorNodeRuntime` half is the alloc-gated DYNAMIC path).
static POC_STORE: ComponentSlotStorage<PocNode> = ComponentSlotStorage::new();

/// Caller-supplied executor backing — the `open_in` contract. `static mut`
/// touched exactly once, before the executor exists.
static mut BACKING: [MaybeUninit<u64>; ExecutorSizing::DEFAULT.u64_len()] =
    [MaybeUninit::uninit(); ExecutorSizing::DEFAULT.u64_len()];

#[entry]
fn main() -> ! {
    nros_platform_cffi::log::init_default();

    let config = ExecutorConfig::resolve(BootConfig {
        node_name: Some("heap_free_poc"),
        locator: None,
        domain_id: Some(0),
        namespace: None,
    });

    // SAFETY: `BACKING` is `'static`, exactly `u64_len()` words, and this is
    // the only reference ever taken (single-threaded `#[entry]`, before any
    // executor exists).
    let backing: &'static mut [MaybeUninit<u64>] = unsafe { &mut *(&raw mut BACKING) };

    match unsafe { nros::Executor::open_in(&config, backing, ExecutorSizing::DEFAULT) } {
        Ok(mut executor) => {
            // Unreachable on this fixture (no backend linked), but LLVM cannot
            // prove that, so the whole install + spin path is IN the image —
            // which is exactly what the link gate measures.
            let rc = unsafe {
                nros::install_node_typed_in(
                    &mut executor as *mut _ as *mut core::ffi::c_void,
                    &POC_STORE,
                )
            };
            hprintln!("HEAP-FREE-POC: unexpected open success, install rc={}", rc);
            let _ = executor.spin_once(core::time::Duration::from_millis(10));
            debug::exit(debug::EXIT_FAILURE);
        }
        Err(_) => {
            // The expected arm: selection fails (zero backends registered)
            // AFTER the executor machinery is linked and the open path ran.
            hprintln!("HEAP-FREE-POC: open refused (no RMW linked) — link property holds");
            debug::exit(debug::EXIT_SUCCESS);
        }
    }
    loop {}
}
