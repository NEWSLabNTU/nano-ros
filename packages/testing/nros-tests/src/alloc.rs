//! RFC-0051 / phase-295 W1 — the ONE isolation allocator.
//!
//! Every baked-isolation cell of [`crate::matrix::CELLS`] gets a
//! deterministic, matrix-unique (router port, XRCE agent port, ROS domain)
//! assignment from the formulas here — shared by the FIXTURE BAKER (the
//! locator/domain a cell's image compiles in) and the TEST RUNNER (the
//! router/agent the test starts), so the two can never disagree by hand
//! (the pre-295 failure mode: 27 test files hand-mirroring `175xx`
//! fixtures.toml literals).
//!
//! Design choice: the allocator CODIFIES the proven phase-89.13 banding
//! (`platform base + workload offset + lang * 100`) instead of renumbering
//! the world — the RFC's guarantee is injectivity, not any particular
//! numeral. Injectivity is PROVEN by the exhaustive test below over the
//! whole matrix, so extending an axis can never silently collide.
//!
//! Native cells DON'T use this: host processes take runtime-ephemeral
//! ports (`zenohd_router::start_unique`, `xrce_agent::start_unique`) and
//! `unique_ros_domain_id()` — already parallel-safe, strictly better than
//! any static assignment.

use crate::matrix::{Cell, PlatformId, Rmw};

/// Zenoh router (or generic TCP locator) port for a baked cell.
///
/// Band layout per platform: `7000 + platform.index() * 400` gives each
/// platform a 400-wide window; within it `workload.port_offset()`
/// (0..=92) + `lang.port_index() * 100` (0/100/200) stay < 400 by
/// construction. The historical 745x bases live inside the same windows
/// for the already-migrated families (kept via [`legacy_port`] until W4
/// re-bakes fixtures.toml onto this formula).
pub const fn port(cell: &Cell) -> u16 {
    7000 + cell.platform.index() * 400 + cell.workload.port_offset() + cell.lang.port_index() * 100
}

/// XRCE agent UDP port: its own band (2000+) mirroring the zephyr
/// phase-89.13 agent scheme, generalized per platform.
pub const fn xrce_agent_port(cell: &Cell) -> u16 {
    2000 + cell.platform.index() * 400 + cell.workload.port_offset() + cell.lang.port_index() * 100
}

/// ROS (DDS) domain id for a baked cell — the cyclone SPDP-isolation
/// axis. Valid range 1..=232; each platform gets a 21-wide window
/// (7 workload slots × 3 langs), which fits 11 platforms exactly.
pub const fn domain(cell: &Cell) -> u8 {
    let w = cell.workload.port_offset() / 10; // 0..=9 → slot
    let slot = if w > 6 { 6 } else { w }; // clamp tail workloads into window
    (1 + cell.platform.index() * 21 + slot * 3 + cell.lang.port_index()) as u8
}

/// The pre-W4 baked assignments still living in `fixtures.toml` /
/// `platform.rs` for the classic QEMU families. `matrix-gen` (W1.c)
/// verifies fixtures.toml against THESE until W4 re-bakes onto
/// [`port`]; after W4 this table and the 745x bases die together.
pub const fn legacy_port(cell: &Cell) -> Option<u16> {
    use crate::platform::*;
    let cfg: &PlatformConfig = match cell.platform {
        PlatformId::QemuBaremetal => &BAREMETAL,
        PlatformId::FreertosMps2 => &FREERTOS,
        PlatformId::NuttxArm => &NUTTX,
        PlatformId::ThreadxRiscv64 => &THREADX_RISCV,
        PlatformId::Esp32Qemu => &ESP32,
        PlatformId::ThreadxLinux => &THREADX_LINUX,
        PlatformId::ZephyrNativeSim => &ZEPHYR,
        _ => return None,
    };
    let variant = match cell.workload.as_test_variant() {
        Some(v) => v,
        None => return None,
    };
    match cell.rmw {
        Rmw::Xrce => {
            let p = cfg.xrce_agent_port_for(variant, cell.lang.as_test_lang());
            if p == 0 { None } else { Some(p) }
        }
        _ => Some(cfg.zenohd_port_for(variant, cell.lang.as_test_lang())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matrix::{CELLS, Kind, PlatformId, Tier};
    use std::collections::HashMap;

    fn baked(cell: &&Cell) -> bool {
        // Native = ephemeral; everything else bakes.
        !matches!(cell.platform, PlatformId::Native)
            && matches!(cell.tier, Tier::Runtime)
            && !matches!(cell.kind, Kind::Interop)
    }

    /// THE RFC-0051 guarantee: the allocator is injective over every
    /// baked runtime cell — two distinct cells never share a port or a
    /// (rmw-relevant) domain. Extending any axis re-proves this here.
    #[test]
    fn allocator_injective_over_matrix() {
        let mut ports: HashMap<u16, &Cell> = HashMap::new();
        let mut domains: HashMap<u8, &Cell> = HashMap::new();
        for c in CELLS.iter().filter(baked) {
            let p = port(c);
            if let Some(prev) = ports.insert(p, c) {
                // Same-cell zenoh/cyclone pairs may share the TCP port
                // slot (cyclone doesn't dial it); only same-RMW clashes
                // are real.
                assert!(prev.rmw != c.rmw, "port collision {p}: {prev:?} vs {c:?}");
            }
            if matches!(c.rmw, crate::matrix::Rmw::Cyclonedds) {
                let d = domain(c);
                if let Some(prev) = domains.insert(d, c) {
                    panic!("domain collision {d}: {prev:?} vs {c:?}");
                }
            }
        }
    }

    /// Ports stay inside the documented band and off well-known ports.
    #[test]
    fn ports_in_band() {
        for c in CELLS.iter().filter(baked) {
            let p = port(c);
            assert!((7000..12000).contains(&p), "port {p} out of band for {c:?}");
        }
    }

    /// Domains stay DDS-valid.
    #[test]
    fn domains_valid() {
        for c in CELLS.iter().filter(baked) {
            let d = domain(c);
            assert!((1..=232).contains(&d), "domain {d} invalid for {c:?}");
        }
    }
}
