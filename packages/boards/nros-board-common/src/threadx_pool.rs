// Issue 2624.
//! The ThreadX byte pool's derived size.
//!
//! On this port the byte pool is the whole allocator: the app thread's stack,
//! every sched task's stack, NetX Duo's packet pool and IP/ARP/BSD stacks,
//! and the RMW's working set all come out of it. It was a 4 MiB literal
//! (~~`BYTE_POOL_BASE_SIZE`~~ in `c/threadx_hooks.c`) that nothing derived.
//!
//! ```text
//! app_stack_bytes + sched_task_stack_bytes + WORKING_SET[rmw]
//! ```
//!
//! `sched_task_stack_bytes` is what the image's SPAWNED sched tasks take
//! from the pool: the stacks they state plus one app stack for each that
//! states none ([`sched_task_stack_bytes`]). The working sets were measured
//! on images that spawned none.
//!
//! Each working set is MEASURED: every image prints `nros: byte pool peak`
//! (`nros-board-threadx/src/entry.rs`), read after it has been stable with the
//! session up and traffic flowing. The working set is that peak less the one
//! app-thread stack the image allocated. Every measured image runs on
//! `threadx-qemu-riscv64`, the board whose NetX allocates its packet pool and
//! stacks from here (threadx-linux runs no NetX), on the cmake road with the
//! static executor backing, so its app stack is 512 KiB.
//!
//! The base is per RMW: one Cyclone-sized figure would hand every zenoh image
//! about 0.8 MiB it never touches.

/// The app-thread stack every measured image ran with: the RISC-V board's
/// 512 KiB (`NROS_THREADX_APP_STACK_BYTES` in its board C file).
pub const MEASURED_APP_STACK_BYTES: usize = 512 * 1024;

/// Worst measured zenoh peak: `rv-virt-threadx` rust action-server (goal
/// served), 725,088 bytes, 2026-10-01. Less its 512 KiB app stack: 200,800.
pub const ZENOH_WORST_PEAK_BYTES: usize = 725_088;

/// Everything a zenoh image's pool holds besides the app stack: zenoh-pico's
/// tasks and buffers, NetX's packet pool and stacks, nano-ros's own
/// allocations. 327,680 is 1.63x the measured 200,800.
pub const ZENOH_WORKING_SET_BYTES: usize = 327_680;

/// Worst measured Cyclone peak: `rv-virt-threadx` rust action-client, a run
/// that COMPLETED its goal (`Result received` logged before the peak line),
/// 1,317,840 bytes, 2026-11-20. Three completed runs read 1,316,728 to
/// 1,317,840. The client also prints `TIMEOUT: no completion` on some runs:
/// that is its first goal timing out and the example retrying, which it then
/// completes, so it does not lower the peak (a run that logs it still reads
/// 1,317,840). The other five roles peak at 1,042,128 to 1,136,992.
pub const CYCLONE_WORST_PEAK_BYTES: usize = 1_317_840;

/// Everything a Cyclone image's pool holds besides the app stack: the
/// participant, discovery endpoints, sample buffers and NetX. 1,179,648 is
/// 1.49x the measured 793,552.
pub const CYCLONE_WORKING_SET_BYTES: usize = 1_179_648;

// Issue 2812.
/// What one more PARAMETER-serving node costs the pool beyond its
/// six servers' buffer pairs (`nros_sizing_descriptor::service_heap`).
/// Every working set above was measured on a one-node image, so each further
/// node adds its buffer pairs plus this.
pub const PARAM_NODE_REMAINDER_BYTES: usize = 0;

// Issue 2812.
/// The same for one more LIFECYCLE state machine, beyond its
/// five servers' buffer pairs.
pub const LIFECYCLE_MACHINE_REMAINDER_BYTES: usize = 0;

/// Which working set an image's pool carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoolRmw {
    /// zenoh-pico (and the default when nothing names a DDS backend).
    Zenoh,
    /// CycloneDDS, and every backend that is not zenoh (XRCE's working set is
    /// unmeasured): the larger measured working set.
    Cyclone,
}

/// The working set for `rmw`.
#[must_use]
pub const fn working_set_bytes(rmw: PoolRmw) -> usize {
    match rmw {
        PoolRmw::Zenoh => ZENOH_WORKING_SET_BYTES,
        PoolRmw::Cyclone => CYCLONE_WORKING_SET_BYTES,
    }
}

// Issue 2732.
/// The bytes an image's spawned sched tasks take from the pool:
/// `stated_bytes` (the sum of the stacks they state, `[image]
/// spawned_sched_task_stack_bytes`) plus `app_stack_bytes` for each of the
/// `on_app_stack` that state none (`nros_threadx_create_task` gives such a
/// task the app thread's stack).
#[must_use]
pub const fn sched_task_stack_bytes(
    stated_bytes: usize,
    on_app_stack: usize,
    app_stack_bytes: usize,
) -> usize {
    stated_bytes + on_app_stack * app_stack_bytes
}

/// The derived byte pool for an image whose app thread takes
/// `app_stack_bytes` and whose spawned sched tasks take
/// `sched_task_stack_bytes` ([`sched_task_stack_bytes`]).
#[must_use]
pub const fn default_byte_pool_bytes(
    app_stack_bytes: usize,
    sched_task_stack_bytes: usize,
    rmw: PoolRmw,
) -> usize {
    app_stack_bytes + sched_task_stack_bytes + working_set_bytes(rmw)
}

/// The least a stated pool may be: the app stack plus the MEASURED working
/// set, with no margin. A statement below it is refused at build time,
/// naming the knob.
#[must_use]
pub const fn least_byte_pool_bytes(
    app_stack_bytes: usize,
    sched_task_stack_bytes: usize,
    rmw: PoolRmw,
) -> usize {
    app_stack_bytes
        + sched_task_stack_bytes
        + match rmw {
            PoolRmw::Zenoh => ZENOH_WORST_PEAK_BYTES - MEASURED_APP_STACK_BYTES,
            PoolRmw::Cyclone => CYCLONE_WORST_PEAK_BYTES - MEASURED_APP_STACK_BYTES,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_working_set_carries_its_stated_margin_over_the_measured_peak() {
        for (rmw, peak, tenths) in [
            (PoolRmw::Zenoh, ZENOH_WORST_PEAK_BYTES, 16),
            (PoolRmw::Cyclone, CYCLONE_WORST_PEAK_BYTES, 14),
        ] {
            let measured = peak - MEASURED_APP_STACK_BYTES;
            let ws = working_set_bytes(rmw);
            assert!(
                ws * 10 >= measured * tenths,
                "{rmw:?}: {ws} over {measured}"
            );
            assert!(
                ws * 10 < measured * 20,
                "{rmw:?}: {ws} is over 2x {measured}"
            );
        }
    }

    #[test]
    fn the_derived_pool_holds_every_measured_image() {
        let app = MEASURED_APP_STACK_BYTES;
        assert!(default_byte_pool_bytes(app, 0, PoolRmw::Zenoh) > ZENOH_WORST_PEAK_BYTES);
        assert!(default_byte_pool_bytes(app, 0, PoolRmw::Cyclone) > CYCLONE_WORST_PEAK_BYTES);
        // The floor is the measured peak itself at the measured app stack.
        assert_eq!(
            least_byte_pool_bytes(app, 0, PoolRmw::Zenoh),
            ZENOH_WORST_PEAK_BYTES
        );
        assert_eq!(
            least_byte_pool_bytes(app, 0, PoolRmw::Cyclone),
            CYCLONE_WORST_PEAK_BYTES
        );
    }

    /// `c/threadx_hooks.c` sizes the pool on the cmake road, where no build
    /// script runs: its literals must be this module's numbers.
    #[test]
    fn the_c_file_states_the_derived_working_sets() {
        let c = include_str!("../c/threadx_hooks.c");
        let define = |name: &str| -> usize {
            c.lines()
                .find_map(|l| {
                    let rest = l.trim().strip_prefix("#define ")?.strip_prefix(name)?;
                    rest.trim().trim_end_matches('u').parse().ok()
                })
                .unwrap_or_else(|| panic!("threadx_hooks.c defines no `{name}` literal"))
        };
        assert_eq!(
            define("NROS_THREADX_ZENOH_WORKING_SET_BYTES"),
            ZENOH_WORKING_SET_BYTES
        );
        assert_eq!(
            define("NROS_THREADX_DDS_WORKING_SET_BYTES"),
            CYCLONE_WORKING_SET_BYTES
        );
        assert_eq!(
            define("NROS_THREADX_ZENOH_MEASURED_WORKING_SET_BYTES"),
            ZENOH_WORST_PEAK_BYTES - MEASURED_APP_STACK_BYTES
        );
        assert_eq!(
            define("NROS_THREADX_DDS_MEASURED_WORKING_SET_BYTES"),
            CYCLONE_WORST_PEAK_BYTES - MEASURED_APP_STACK_BYTES
        );
        // The pool is the derived figure, and nothing subtracts the backing.
        let pool = c
            .lines()
            .find(|l| l.trim_start().starts_with("#define BYTE_POOL_SIZE"))
            .expect("threadx_hooks.c defines BYTE_POOL_SIZE");
        assert!(pool.contains("NROS_THREADX_BYTE_POOL_BYTES"), "{pool}");
        assert!(!pool.contains("NROS_EXECUTOR_BACKING_BYTES"), "{pool}");
    }

    /// Issue 2732 -- a spawned task that states no stack costs one app stack;
    /// one that states a stack costs it. Both the derived pool and the floor
    /// grow by the total, and an image that spawns nothing is unchanged.
    #[test]
    fn spawned_sched_task_stacks_grow_the_pool_and_its_floor() {
        let app = 64 * 1024;
        assert_eq!(sched_task_stack_bytes(0, 0, app), 0);
        assert_eq!(sched_task_stack_bytes(0, 1, app), app);
        assert_eq!(sched_task_stack_bytes(12_288, 2, app), 12_288 + 2 * app);
        let tasks = sched_task_stack_bytes(0, 1, app);
        for rmw in [PoolRmw::Zenoh, PoolRmw::Cyclone] {
            assert_eq!(
                default_byte_pool_bytes(app, tasks, rmw),
                default_byte_pool_bytes(app, 0, rmw) + app
            );
            assert_eq!(
                least_byte_pool_bytes(app, tasks, rmw),
                least_byte_pool_bytes(app, 0, rmw) + app
            );
        }
    }

    /// Issue 2732 -- the C file's derivation is this module's: its default and
    /// its floor both add `NROS_THREADX_SCHED_TASK_STACK_BYTES`, which reads 0
    /// when no build states it.
    #[test]
    fn the_c_file_counts_the_sched_task_stacks() {
        let c = include_str!("../c/threadx_hooks.c");
        let block = |start: &str, end: &str| -> &'static str {
            let i = c.find(start).unwrap_or_else(|| panic!("no `{start}`"));
            let j = i + c[i..].find(end).unwrap_or_else(|| panic!("no `{end}`"));
            &c[i..j]
        };
        let default = block("#ifndef NROS_THREADX_BYTE_POOL_BYTES", "#endif");
        assert!(
            default.contains("NROS_THREADX_SCHED_TASK_STACK_BYTES"),
            "{default}"
        );
        // The floor's comparison, whatever line clang-format puts it on.
        let floor = block("NROS_THREADX_BYTE_POOL_BYTES >=", ";");
        assert!(
            floor.contains("NROS_THREADX_SCHED_TASK_STACK_BYTES"),
            "{floor}"
        );
        let fallback = block("#ifndef NROS_THREADX_SCHED_TASK_STACK_BYTES", "#endif");
        assert!(
            fallback.contains("#define NROS_THREADX_SCHED_TASK_STACK_BYTES 0u"),
            "{fallback}"
        );
    }

    #[test]
    fn the_derived_pool_is_below_the_old_four_mib_literal() {
        let app = MEASURED_APP_STACK_BYTES;
        assert_eq!(default_byte_pool_bytes(app, 0, PoolRmw::Zenoh), 851_968);
        assert_eq!(default_byte_pool_bytes(app, 0, PoolRmw::Cyclone), 1_703_936);
        assert!(default_byte_pool_bytes(app, 0, PoolRmw::Cyclone) < 4 * 1024 * 1024);
    }

    /// `cmake/NanoRosRtosServiceHeap.cmake` prices the cmake road with these
    /// remainders; issue 2812.
    #[test]
    #[cfg(feature = "build-helpers")]
    fn the_cmake_road_states_these_remainders() {
        let cmake = include_str!("../../../../cmake/NanoRosRtosServiceHeap.cmake");
        let cmake_set = |name: &str| -> usize {
            crate::service_heap::cmake_cache_value(cmake, name)
                .unwrap_or_else(|| panic!("NanoRosRtosServiceHeap.cmake sets no `{name}`"))
        };
        assert_eq!(
            cmake_set("NROS_THREADX_PARAM_NODE_REMAINDER"),
            PARAM_NODE_REMAINDER_BYTES
        );
        assert_eq!(
            cmake_set("NROS_THREADX_LIFECYCLE_MACHINE_REMAINDER"),
            LIFECYCLE_MACHINE_REMAINDER_BYTES
        );
    }
}
