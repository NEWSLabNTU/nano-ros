//! issue 1795 — the RTOS TYPED CARRIER road boots and publishes.
//!
//! `nros_components_register_node(... TYPED DEPLOY <rtos>)` builds its image
//! through `nano_ros_node_register`'s RTOS carrier arm, which synthesises a C++
//! entry holding the component's node as a STATIC object. Every other RTOS
//! C/C++ cell links through `nano_ros_add_executable()` / `nano_ros_entry()`, so
//! this road had no lane, and it failed three times without one:
//!
//! * issue 1742 — the carrier applied no panic policy;
//! * issue 1753 — the static object needs the never-exits C++ static-object ABI
//!   and the board's `.fini_array` bounds, which ThreadX RISC-V lacked;
//! * issue 1795 itself — the arms compared the raw `NROS_PLATFORM` with the
//!   family (`"threadx"`), and every lane passes the variant
//!   (`threadx_riscv64`, `nuttx_armv7a`), so no lane-built carrier ever fired.
//!
//! No peer: the image's own publish lines are the evidence that the road
//! linked, booted and drove the component on the board's executor. Each image
//! dials `alloc::port_of(<platform>, C, TypedCarrier)`, baked by its fixture row.

use nros_tests::{
    alloc::port_of,
    fixtures::{
        QemuProcess, RequireFixture, ZenohRouter, is_qemu_available, is_qemu_riscv64_available,
        nuttx::build_nuttx_c_typed_carrier, threadx_riscv64::build_rv64_c_typed_carrier,
    },
    matrix::{Cell, Kind, Lang, PlatformId, Rmw, Tier, Workload},
    output::TYPED_CARRIER_SENT_PREFIX,
    process::require_zenohd,
};
use std::{path::Path, time::Duration};

/// Platforms with a case below. The tripwire holds the matrix to exactly this.
const COVERED: &[PlatformId] = &[PlatformId::ThreadxRiscv64, PlatformId::NuttxArm];

fn is_typed_carrier_cell(c: &Cell) -> bool {
    matches!(c.workload, Workload::TypedCarrier)
        && matches!(c.kind, Kind::Example)
        && matches!(c.tier, Tier::Runtime)
}

/// Tripwire — a TypedCarrier cell with no case here would run nowhere, which
/// is the build-only-reads-as-coverage shape this file exists to close; a case
/// with no cell would run a coordinate no lane builds.
#[test]
fn typed_carrier_cases_cover_every_matrix_cell() {
    let mut cells: Vec<&Cell> = nros_tests::matrix::CELLS
        .iter()
        .filter(|c| is_typed_carrier_cell(c))
        .collect();
    for c in &cells {
        assert!(
            matches!(c.lang, Lang::C) && matches!(c.rmw, Rmw::Zenoh),
            "a TypedCarrier cell moved off c/zenoh: {c:?}"
        );
    }
    cells.sort_by_key(|c| format!("{:?}", c.platform));
    let mut got: Vec<String> = cells.iter().map(|c| format!("{:?}", c.platform)).collect();
    let mut want: Vec<String> = COVERED.iter().map(|p| format!("{p:?}")).collect();
    got.sort();
    want.sort();
    assert_eq!(
        got, want,
        "the TypedCarrier cells and this file's cases disagree — add a case (and \
         COVERED) for a new cell, or a cell for a new case"
    );
}

/// This lane's TypedCarrier cell on `platform`. The cell list is narrowed by
/// `lane_scope::admits` (issue 0571's rule for a file that iterates CELLS by
/// platform); the caller has already lane-skipped an out-of-lane platform, so
/// a miss here means the matrix lost the cell.
fn lane_cell(platform: PlatformId) -> &'static Cell {
    nros_tests::matrix::CELLS
        .iter()
        .filter(|c| nros_tests::lane_scope::admits(c.platform))
        .find(|c| is_typed_carrier_cell(c) && c.platform == platform)
        .unwrap_or_else(|| panic!("matrix regression: no {platform:?}/TypedCarrier cell"))
}

fn boot_and_publish(
    platform: PlatformId,
    image: &Path,
    start: fn(&Path) -> nros_tests::TestResult<QemuProcess>,
) {
    let cell = lane_cell(platform);
    let port = port_of(cell.platform, cell.lang, cell.workload);
    require_zenohd();
    // 0.0.0.0, so the slirp guest (gateway 10.0.2.2) reaches it.
    let _router = ZenohRouter::start_slirp(port)
        .unwrap_or_else(|e| nros_tests::unmet!("zenohd failed to start on {port}: {e}"));

    let mut qemu = start(image).unwrap_or_else(|e| panic!("boot {platform:?} QEMU: {e}"));
    let third = format!("{TYPED_CARRIER_SENT_PREFIX} 2");
    let out = qemu.collect_until(&third, Duration::from_secs(90));
    qemu.kill();
    assert!(
        out.contains(&format!("{TYPED_CARRIER_SENT_PREFIX} 0")) && out.contains(&third),
        "{platform:?}: the typed-carrier image did not publish 3 messages through the \
         router on port {port} (issue 1795). A carrier that fails to LINK never gets \
         here — this is a boot or session failure.\nConsole:\n{out}"
    );
}

#[test]
fn threadx_riscv64_typed_carrier_boots_and_publishes() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ThreadxRiscv64],
        "the rv-virt-threadx typed-carrier image",
    );
    if !is_qemu_riscv64_available() {
        nros_tests::unmet!("qemu-system-riscv64 not found");
    }
    let image = build_rv64_c_typed_carrier()
        .map(|p| p.to_path_buf())
        .require("examples/rv-virt-threadx/c/typed-carrier");
    boot_and_publish(PlatformId::ThreadxRiscv64, &image, |p| {
        QemuProcess::start_riscv64_virt(p, 0)
    });
}

#[test]
fn nuttx_arm_typed_carrier_boots_and_publishes() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::NuttxArm],
        "the qemu-armv7a-nuttx typed-carrier image",
    );
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not found");
    }
    let image = build_nuttx_c_typed_carrier()
        .map(|p| p.to_path_buf())
        .require("examples/qemu-armv7a-nuttx/c/typed-carrier");
    boot_and_publish(PlatformId::NuttxArm, &image, |p| {
        QemuProcess::start_nuttx_virt(p, true)
    });
}

/// RFC-0051 §7c (phase-510 W3) — the cases this file runs are exactly the
/// Runtime cells `matrix::consumer_of` assigns to it. Fixture-free: a cell
/// added, retiered or reassigned without this list moving is a red here.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_cases,
    };
    assert_consumer_cases(
        Consumer::TypedCarrier,
        &[
            (P::NuttxArm, L::C, R::Zenoh, W::TypedCarrier, K::Example),
            (
                P::ThreadxRiscv64,
                L::C,
                R::Zenoh,
                W::TypedCarrier,
                K::Example,
            ),
        ],
    );
}
