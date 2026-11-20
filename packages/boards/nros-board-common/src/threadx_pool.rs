// Issue 2624.
//! The ThreadX byte pool's derived size.
//!
//! On this port the byte pool is the whole allocator: the app thread's stack,
//! every sched task's stack, NetX Duo's packet pool and IP/ARP/BSD stacks,
//! and the RMW's working set all come out of it. It was a 4 MiB literal
//! (~~`BYTE_POOL_BASE_SIZE`~~ in `c/threadx_hooks.c`) that nothing derived.
//!
//! ```text
//! app_stack_bytes + WORKING_SET[rmw]
//! ```
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

/// The derived byte pool for an image whose app thread takes
/// `app_stack_bytes`.
#[must_use]
pub const fn default_byte_pool_bytes(app_stack_bytes: usize, rmw: PoolRmw) -> usize {
    app_stack_bytes + working_set_bytes(rmw)
}

/// The least a stated pool may be: the app stack plus the MEASURED working
/// set, with no margin. A statement below it is refused at build time,
/// naming the knob.
#[must_use]
pub const fn least_byte_pool_bytes(app_stack_bytes: usize, rmw: PoolRmw) -> usize {
    app_stack_bytes
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
        assert!(default_byte_pool_bytes(app, PoolRmw::Zenoh) > ZENOH_WORST_PEAK_BYTES);
        assert!(default_byte_pool_bytes(app, PoolRmw::Cyclone) > CYCLONE_WORST_PEAK_BYTES);
        // The floor is the measured peak itself at the measured app stack.
        assert_eq!(
            least_byte_pool_bytes(app, PoolRmw::Zenoh),
            ZENOH_WORST_PEAK_BYTES
        );
        assert_eq!(
            least_byte_pool_bytes(app, PoolRmw::Cyclone),
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

    #[test]
    fn the_derived_pool_is_below_the_old_four_mib_literal() {
        let app = MEASURED_APP_STACK_BYTES;
        assert_eq!(default_byte_pool_bytes(app, PoolRmw::Zenoh), 851_968);
        assert_eq!(default_byte_pool_bytes(app, PoolRmw::Cyclone), 1_703_936);
        assert!(default_byte_pool_bytes(app, PoolRmw::Cyclone) < 4 * 1024 * 1024);
    }
}
