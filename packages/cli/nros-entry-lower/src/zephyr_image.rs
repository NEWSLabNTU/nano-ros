//! Which Kconfig symbols carry a Zephyr image's TRANSPORT priority, and the
//! one reader of them out of that image's `.config` (issues 1508, 1537).
//!
//! A derived tier is allocated out of the image's own `pool.app`, and on
//! Zephyr that pool is DERIVED per image (RFC-0079 section 4.1): both ends of
//! the transport's priority chain are Kconfig. The KERNEL half
//! (`CONFIG_NUM_PREEMPT_PRIORITIES` & co.) is Zephyr's own vocabulary and
//! `nros-orchestration-ir` reads it. The TRANSPORT half is a backend
//! statement, and RFC-0071 D2 keeps backend names out of core crates - so the
//! core crate is handed the bands as NUMBERS, and the symbols are named here.
//!
//! Here, rather than in `nros-cli-core` where issue 1508 first put them,
//! because TWO derivations need them and one of them is the `nros::main!`
//! proc-macro, which cannot depend on the CLI (issue 0083). This crate is the
//! one both can afford, and it gains no dependency for it. The list is held
//! equal to the Zephyr board descriptor's `[board.priority_plan] inputs` by
//! `nros-cli-core`'s `the_band_symbols_are_the_descriptors_inputs`.

/// The Kconfig symbols carrying the normalised band (0..255) each transport
/// task is created at. An image that sets none of them runs the Kconfig
/// default (`nros_orchestration_ir::priority_plan::ZEPHYR_TRANSPORT_BAND_DEFAULT`,
/// checked against `zephyr/Kconfig` by `check-tier-priority-plan-image.py
/// --selftest`).
pub const ZEPHYR_TRANSPORT_BAND_SYMBOLS: [&str; 2] = [
    "CONFIG_NROS_ZENOH_READ_PRIORITY",
    "CONFIG_NROS_ZENOH_LEASE_PRIORITY",
];

/// The kernel half of the plan's inputs: Zephyr's own symbols, read by
/// `PriorityPlan::from_zephyr_dotconfig` itself.
pub const ZEPHYR_KERNEL_PLAN_SYMBOLS: [&str; 4] = [
    "CONFIG_NUM_PREEMPT_PRIORITIES",
    "CONFIG_NUM_COOP_PRIORITIES",
    "CONFIG_POSIX_PRIORITY_SCHEDULING",
    "CONFIG_PREEMPT_ENABLED",
];

/// The transport bands an image's `.config` states, in
/// [`ZEPHYR_TRANSPORT_BAND_SYMBOLS`] order. An absent symbol is `default` -
/// the Kconfig default, which is what the image runs with. (An argument
/// because the number is the core crate's, and this crate does not depend on
/// it.)
#[must_use]
pub fn transport_bands(dotconfig: &str, default: i64) -> [i64; 2] {
    ZEPHYR_TRANSPORT_BAND_SYMBOLS.map(|sym| {
        dotconfig
            .lines()
            .filter_map(|l| l.trim().strip_prefix(sym)?.strip_prefix('='))
            .filter_map(|v| v.trim().parse::<i64>().ok())
            .next_back()
            .unwrap_or(default)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_band_is_the_default_and_the_last_assignment_wins() {
        assert_eq!(transport_bands("", 200), [200, 200]);
        let text = "CONFIG_NROS_ZENOH_READ_PRIORITY=90\n\
                    CONFIG_NROS_ZENOH_READ_PRIORITY=100\n\
                    # CONFIG_NROS_ZENOH_LEASE_PRIORITY is not set\n";
        assert_eq!(transport_bands(text, 200), [100, 200]);
        // A symbol whose name merely STARTS with a band symbol is not it.
        assert_eq!(
            transport_bands("CONFIG_NROS_ZENOH_READ_PRIORITY_X=5\n", 200),
            [200, 200]
        );
    }
}
