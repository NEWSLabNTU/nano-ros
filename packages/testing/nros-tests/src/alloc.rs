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
//! numbers everywhere. The zephyr west lane READS them: it asks the
//! `cell-isolation` bin ([`isolation_table`]) instead of recomputing them,
//! and a west row that states its own values is checked against it at emit
//! time (issue 2251). Elsewhere they are still spelled by hand — the
//! `examples/fixtures.toml` locator/domain columns, the threadx cyclone
//! domain bakes (`just/threadx-*.just`) — and
//! [`crate::fixtures::baked_locator`] verifies the locator rows it names.
//! Changing a formula here re-bakes the world — rebuild every affected
//! fixture family after editing.
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

use crate::matrix::{Cell, Lang, PlatformId, Rmw, Workload};

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
///
/// "Exactly" is load-bearing, and the enum now holds TWELVE platforms: index 11
/// computes domain 233 and `domains_valid` rejects it. That is deliberate rather
/// than a latent bug — `PlatformId::index` documents that low indices are
/// allocated to platforms that BAKE, and the two that do not (`Fvp`, `Px4`) hold
/// the tail. The gate firing is the signal that the scheme is full and wants
/// narrowing (the workload window is 7 slots wide but no platform uses more than
/// a handful), not that another renumber is due.
pub const fn domain_of(platform: PlatformId, lang: Lang, workload: Workload) -> u8 {
    let w = workload.port_offset() / 10; // 0..=9 → slot
    let slot = if w > 6 { 6 } else { w }; // clamp tail workloads into window
    (1 + platform.index() * 21 + slot * 3 + lang.port_index()) as u8
}

/// [`domain_of`] over a matrix cell.
pub const fn domain(cell: &Cell) -> u8 {
    domain_of(cell.platform, cell.lang, cell.workload)
}

/// How far an XRCE cell's DDS domain sits above its [`domain_of`] slot.
///
/// Issue 2221 — an XRCE-DDS image does not speak DDS itself, its Agent does,
/// and the CLIENT picks the domain the Agent's participant joins
/// (`uxr_buffer_create_participant_bin(..., domain_id, ...)`). Two XRCE images
/// behind DIFFERENT Agent ports therefore still meet on one RTPS bus when they
/// bake the same domain: measured, a Rust listener behind Agent 2400 heard
/// 23 of 23 samples from a C talker behind Agent 2500. The Agent port isolates
/// the client↔Agent hop only.
///
/// Before this shift every Zephyr XRCE image baked no domain at all (domain 0
/// for all nine native_sim cells), and the formula domain it would have taken
/// is its Cyclone sibling's — the same `(platform, lang, workload)`. Nine is
/// three workload slots: native_sim's XRCE cells land on 31–39, beside its
/// Cyclone cells on 22–30. The value has no meaning beyond being free;
/// `allocator_injective_over_matrix` below is what proves it is.
pub const XRCE_DOMAIN_SHIFT: u8 = 9;

/// The DDS domain a baked cell's image joins, for every RMW that HAS one on
/// the wire: Cyclone's own participant, or an XRCE Agent's participant on the
/// client's behalf (issue 2221). Zenoh has no domain on the wire; it gets
/// [`domain_of`], which nothing reads.
pub const fn dds_domain_of(platform: PlatformId, lang: Lang, workload: Workload, rmw: Rmw) -> u8 {
    let base = domain_of(platform, lang, workload);
    match rmw {
        Rmw::Xrce => base + XRCE_DOMAIN_SHIFT,
        _ => base,
    }
}

/// [`dds_domain_of`] over a matrix cell.
pub const fn dds_domain(cell: &Cell) -> u8 {
    dds_domain_of(cell.platform, cell.lang, cell.workload, cell.rmw)
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
/// `qemu-bsp-listener`, `examples/mps2-an385-baremetal/rust/{talker,listener}`).
pub const BAREMETAL_BSP_PORT: u16 = aux_port(PlatformId::QemuBaremetal, 0);

/// mps2-an385 RTIC mixed-priority pubsub pair
/// (`examples/mps2-an385-baremetal/rust/{talker,listener}-rtic-mixed`).
pub const BAREMETAL_MIXED_PRIORITY_PORT: u16 = aux_port(PlatformId::QemuBaremetal, 1);

/// mps2-an385 large-message bench firmware
/// (`packages/testing/nros-bench/large-msg-baremetal`).
pub const BAREMETAL_LARGE_MSG_PORT: u16 = aux_port(PlatformId::QemuBaremetal, 2);

/// mps2-an385 RTIC generated-entry run-plan image
/// (`packages/testing/nros-tests/bins/rtic-run-plan-e2e`, run by `just qemu
/// test-rtic-main-e2e`). Issue 1865 — the image STATES this port in its
/// `system.toml` and the recipe reads it from there; this constant only keeps
/// the slot out of every other image's way (`allocator_injective_over_matrix`)
/// and `baked_locator`'s test holds the two equal.
pub const BAREMETAL_RTIC_RUN_PLAN_PORT: u16 = aux_port(PlatformId::QemuBaremetal, 3);

/// Does this cell's image BAKE its isolation values at build time?
///
/// Native cells are ephemeral (runtime ports, `unique_ros_domain_id`); every
/// other runtime cell bakes. The one predicate both the injectivity gate and
/// [`isolation_table`] select by, so the table the build reads is exactly the
/// set the gate proved collision-free.
pub fn is_baked(cell: &Cell) -> bool {
    use crate::matrix::{Kind, Tier};
    !matches!(cell.platform, PlatformId::Linux)
        && matches!(cell.tier, Tier::Runtime)
        && !matches!(cell.kind, Kind::Interop)
        // phase-510 — a CONFIGURED cell shares its default-build sibling's
        // (platform, lang, workload) and so its formula port; its image is
        // a different fixture, which carries its own locator.
        && cell.config.is_empty()
}

/// The `rmw = "..."` token `examples/fixtures.toml` spells an RMW with.
///
/// Local to the table on purpose: `matrix::Rmw` has no blessed spelling
/// (`Cyclonedds` is `"cyclone"` to the native example consumers), and this is
/// the MANIFEST's vocabulary, the one a fixture row and the leaf builder use.
const fn manifest_rmw_token(rmw: Rmw) -> &'static str {
    match rmw {
        Rmw::Zenoh => "zenoh",
        Rmw::Cyclonedds => "cyclonedds",
        Rmw::Xrce => "xrce",
        Rmw::Uorb => "uorb",
    }
}

/// Issue 2251 — every baked cell's isolation values, for a BUILD to read.
///
/// One line per (fixture platform token, lang, rmw, workload), tab-separated:
///
/// ```text
/// <platform-token>  <lang>  <rmw>  <Workload>  <router port>  <xrce agent port>  <dds domain>
/// ```
///
/// The router port is `-` unless the RMW is zenoh, the Agent port `-` unless it
/// is XRCE. The domain is always stated: Cyclone and XRCE put it on the RTPS
/// bus, and a zenoh image still bakes it (rmw_zenoh scopes key expressions by
/// it, which is why a host peer must join the same one).
///
/// This is how the formulas above reach a fixture build without a second
/// spelling: `zephyr-fixture-leaves.sh` used to recompute them in bash
/// (`2400 + lang_offset + variant_offset`, `22 + variant_idx * 3 + lang_idx`),
/// and nothing compared the two, so a formula edit here would have left every
/// image baking the old numbers while the injectivity gate kept proving the
/// new ones. The build now asks `cell-isolation`, which prints this.
///
/// Sorted and de-duplicated, so the output is stable and a key appears once.
pub fn isolation_table() -> String {
    use std::{collections::BTreeSet, fmt::Write as _};
    let mut lines = BTreeSet::new();
    for c in crate::matrix::CELLS.iter().filter(|c| is_baked(c)) {
        for token in c.platform.fixture_tokens() {
            // A column the cell's RMW does not dial is `-`, so no build can
            // bake a router port into a DDS image or an Agent port into a
            // zenoh one and have it look meaningful.
            let router = match c.rmw {
                Rmw::Zenoh => port(c).to_string(),
                _ => "-".to_string(),
            };
            let agent = match c.rmw {
                Rmw::Xrce => xrce_agent_port(c).to_string(),
                _ => "-".to_string(),
            };
            lines.insert(format!(
                "{token}\t{}\t{}\t{:?}\t{router}\t{agent}\t{}",
                c.lang.as_str(),
                manifest_rmw_token(c.rmw),
                c.workload,
                dds_domain(c),
            ));
        }
    }
    let mut out = String::new();
    for l in lines {
        let _ = writeln!(out, "{l}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matrix::CELLS;
    use std::collections::HashMap;

    fn baked(cell: &&Cell) -> bool {
        is_baked(cell)
    }

    /// Issue 2251 — a key the build looks up appears ONCE. Two lines for one
    /// (token, lang, rmw, workload) would make the build's answer depend on
    /// which line it read last.
    #[test]
    fn isolation_table_keys_are_unique() {
        let table = isolation_table();
        let mut seen = std::collections::HashSet::new();
        for line in table.lines() {
            let key: Vec<&str> = line.split('\t').take(4).collect();
            assert!(seen.insert(key.clone()), "duplicate isolation key {key:?}");
        }
        assert!(
            table
                .lines()
                .any(|l| l.starts_with("zephyr\trust\txrce\tPubsub\t")),
            "the native_sim role leaves' cells are missing from the table:\n{table}"
        );
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
            // Issue 2221 — every RMW whose image puts a participant on the
            // RTPS bus: Cyclone directly, XRCE through its Agent. Checking
            // Cyclone alone let all nine Zephyr XRCE cells share domain 0.
            if matches!(c.rmw, Rmw::Cyclonedds | Rmw::Xrce) {
                let d = dds_domain(c);
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
            BAREMETAL_RTIC_RUN_PLAN_PORT,
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
            let d = dds_domain(c);
            assert!((1..=232).contains(&d), "domain {d} invalid for {c:?}");
        }
    }

    /// W4 emitter — prints the full bake table (one row per baked runtime
    /// cell) so the fixture-side bakes (fixtures.toml, Cargo deploy
    /// metadata, zephyr-fixture-leaves.sh, just/threadx-*.just) can be
    /// regenerated/diffed by eye:
    /// `cargo test -p nros-tests --lib alloc::tests::print_bake_table -- --nocapture`
    ///
    /// It asserts the one property a reader of the table relies on — that there
    /// IS a table. A test whose effects are exhausted by printing passes on
    /// every tree, including one where the `baked` filter selects nothing
    /// (issue 1544; `check-no-vacuous-tests`).
    #[test]
    fn print_bake_table() {
        let rows = CELLS.iter().filter(baked).count();
        assert!(rows > 0, "no baked runtime cell — the bake table is empty");
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
                dds_domain(c)
            );
        }
        println!(
            "aux: baremetal bsp={BAREMETAL_BSP_PORT} mixed={BAREMETAL_MIXED_PRIORITY_PORT} \
             large-msg={BAREMETAL_LARGE_MSG_PORT} rtic-run-plan={BAREMETAL_RTIC_RUN_PLAN_PORT}"
        );
    }
}
