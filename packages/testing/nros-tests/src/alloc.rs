//! RFC-0051 / phase-295 W1+W4 — the ONE isolation allocator.
//!
//! Every baked-isolation cell of [`crate::matrix::CELLS`] gets a
//! deterministic, matrix-unique (router port, XRCE agent port, ROS domain)
//! assignment from the formulas here — shared by the FIXTURE BAKER (the
//! locator/domain a cell's image compiles in) and the TEST RUNNER (the
//! router/agent the test starts), so the two can never disagree by hand
//! (the pre-295 failure mode: 27 test files hand-mirroring `175xx`
//! fixtures.toml literals).
//!
//! Since the phase-295 W4 re-bake, the formula's numbers ARE the baked
//! numbers everywhere: `examples/fixtures.toml` locator/domain columns,
//! the per-example Cargo `[package.metadata.nros.deploy]` locators, the
//! zephyr west lane (`scripts/build/zephyr-fixture-leaves.sh`), the
//! threadx cyclone domain bakes (`just/threadx-*.just`), and
//! [`crate::platform::PlatformConfig`]'s bases are all derived from (or
//! verified against) these functions. Changing a formula here re-bakes
//! the world — rebuild every affected fixture family after editing.
//!
//! Band layout: `7000 + platform.index() * 400` gives each platform a
//! 400-wide TCP window; within it `workload.port_offset()` (0..=92) +
//! `lang.port_index() * 100` (0/100/200/300) stay < 400 by construction.
//! XRCE agents mirror the same offsets in their own `2000+` band. ROS
//! (DDS) domains get 21 ids per platform (7 workload slots × 3 langs)
//! inside the DDS-valid 1..=232 range. Injectivity is PROVEN by the
//! exhaustive test below over the whole matrix, so extending an axis can
//! never silently collide.
//!
//! Native cells DON'T use this: host processes take runtime-ephemeral
//! ports (`zenohd_router::start_unique`, `xrce_agent::start_unique`) and
//! `unique_ros_domain_id()` — already parallel-safe, strictly better than
//! any static assignment.

use crate::matrix::{Cell, Lang, PlatformId, Workload};

/// Base of a platform's 400-wide zenoh/TCP router-port window.
pub const fn platform_port_base(platform: PlatformId) -> u16 {
    7000 + platform.index() * 400
}

/// Base of a platform's XRCE agent UDP-port window (mirrors the TCP
/// window layout in its own 2000+ band).
pub const fn platform_xrce_base(platform: PlatformId) -> u16 {
    2000 + platform.index() * 400
}

/// Zenoh router (or generic TCP locator) port for a baked
/// (platform, lang, workload) coordinate. `kind`/`rmw` don't participate:
/// an Example and a Workspace lane never share a workload coordinate on
/// the same platform (asserted by the injectivity test), and same-cell
/// zenoh/cyclone siblings may share the slot (cyclone never dials it).
pub const fn port_of(platform: PlatformId, lang: Lang, workload: Workload) -> u16 {
    platform_port_base(platform) + workload.port_offset() + lang.port_index() * 100
}

/// [`port_of`] over a matrix cell.
pub const fn port(cell: &Cell) -> u16 {
    port_of(cell.platform, cell.lang, cell.workload)
}

/// XRCE agent UDP port for a baked (platform, lang, workload) coordinate.
pub const fn xrce_agent_port_of(platform: PlatformId, lang: Lang, workload: Workload) -> u16 {
    platform_xrce_base(platform) + workload.port_offset() + lang.port_index() * 100
}

/// [`xrce_agent_port_of`] over a matrix cell.
pub const fn xrce_agent_port(cell: &Cell) -> u16 {
    xrce_agent_port_of(cell.platform, cell.lang, cell.workload)
}

/// ROS (DDS) domain id for a baked (platform, lang, workload) coordinate —
/// the cyclone SPDP-isolation axis. Valid range 1..=232; each platform
/// gets a 21-wide window (7 workload slots × 3 langs), which fits 11
/// platforms exactly.
pub const fn domain_of(platform: PlatformId, lang: Lang, workload: Workload) -> u8 {
    let w = workload.port_offset() / 10; // 0..=9 → slot
    let slot = if w > 6 { 6 } else { w }; // clamp tail workloads into window
    (1 + platform.index() * 21 + slot * 3 + lang.port_index()) as u8
}

/// [`domain_of`] over a matrix cell.
pub const fn domain(cell: &Cell) -> u8 {
    domain_of(cell.platform, cell.lang, cell.workload)
}

/// Auxiliary image slots: a few demo sets carry MORE runnable image pairs
/// than the workload axis has values (the mps2-an385 bare-metal set runs a
/// BSP pair, an RTIC pair, an RTIC mixed-priority pair, and a large-msg
/// bench off ONE `Pubsub` cell). Each extra pair gets a named slot in the
/// platform window's 300..=390 region — disjoint from every
/// workload+lang offset (max 292) — so the images stop sharing a router
/// and their tests parallelize (the pre-W4 `qemu-baremetal-shared`
/// serialization group existed only for this sharing).
pub const fn aux_port(platform: PlatformId, slot: u16) -> u16 {
    assert!(slot < 10, "aux slot out of the 300..=390 window");
    platform_port_base(platform) + 300 + slot * 10
}

/// mps2-an385 BSP (non-RTIC) pubsub demo pair (`qemu-bsp-talker` /
/// `qemu-bsp-listener`, `examples/qemu-arm-baremetal/rust/{talker,listener}`).
pub const BAREMETAL_BSP_PORT: u16 = aux_port(PlatformId::QemuBaremetal, 0);

/// mps2-an385 RTIC mixed-priority pubsub pair
/// (`examples/qemu-arm-baremetal/rust/{talker,listener}-rtic-mixed`).
pub const BAREMETAL_MIXED_PRIORITY_PORT: u16 = aux_port(PlatformId::QemuBaremetal, 1);

/// mps2-an385 large-message bench firmware
/// (`packages/testing/nros-bench/large-msg-baremetal`).
pub const BAREMETAL_LARGE_MSG_PORT: u16 = aux_port(PlatformId::QemuBaremetal, 2);

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
        // Aux slots live outside every workload+lang offset by
        // construction; assert it anyway so a formula change can't
        // silently overlap them.
        for aux in [
            BAREMETAL_BSP_PORT,
            BAREMETAL_MIXED_PRIORITY_PORT,
            BAREMETAL_LARGE_MSG_PORT,
        ] {
            assert!(
                !ports.contains_key(&aux),
                "aux port {aux} collides with a matrix cell"
            );
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

    /// W4 emitter — prints the full bake table (one row per baked runtime
    /// cell) so the fixture-side bakes (fixtures.toml, Cargo deploy
    /// metadata, zephyr-fixture-leaves.sh, just/threadx-*.just) can be
    /// regenerated/diffed by eye:
    /// `cargo test -p nros-tests --lib alloc::tests::print_bake_table -- --nocapture`
    #[test]
    fn print_bake_table() {
        println!(
            "platform            lang   rmw         workload        kind       port   xrce  domain"
        );
        for c in CELLS.iter().filter(baked) {
            println!(
                "{:<19} {:<6} {:<11} {:<15} {:<10} {:<6} {:<5} {}",
                format!("{:?}", c.platform),
                format!("{:?}", c.lang),
                format!("{:?}", c.rmw),
                format!("{:?}", c.workload),
                format!("{:?}", c.kind),
                port(c),
                xrce_agent_port(c),
                domain(c)
            );
        }
        println!(
            "aux: baremetal bsp={BAREMETAL_BSP_PORT} mixed={BAREMETAL_MIXED_PRIORITY_PORT} \
             large-msg={BAREMETAL_LARGE_MSG_PORT}"
        );
    }
}
