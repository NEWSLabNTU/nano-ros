//! phase-510 W5 — loan publish on an embedded target, measured (issue 0814).
//!
//! The configured cell `(FreertosMps2, Rust, Zenoh, Pubsub, Example) +
//! [Lending]` (RFC-0051 §7a). Two MPS2-AN385 FreeRTOS images from
//! `packages/testing/nros-bench/loan-copy-count`, a host zenoh router between
//! them:
//!
//! * the PUBLISHER publishes `PER_PATH` samples through `publish_raw`, then
//!   `PER_PATH` through `try_loan` + `commit`, and counts the payload-sized
//!   copies each call makes — `--wrap` on `memcpy` and the ARM EABI
//!   `__aeabi_memcpy*` helpers, since a bare-metal image has no `LD_PRELOAD`;
//! * the SUBSCRIBER, a second image (a same-session pub->sub loops back
//!   in-process and proves nothing about the wire), counts each path's
//!   deliveries and checks every sample arrived whole.
//!
//! Delivery alone is not the assertion. A lending lane that only proves
//! delivery passes whether or not the loan avoided the copy, which is the lane
//! 0814 rejected. So this asserts both: the loan samples arrive, and the loan
//! path makes fewer payload copies than `publish_raw` on the same image.

use std::time::Duration;

use nros_tests::{
    fixtures::{RequireFixture, ZenohRouter, require_zenohd},
    qemu::QemuProcess,
};

const ROW: &str = "packages/testing/nros-bench/loan-copy-count";

/// The router port both bench images bake (`loan-copy-count/src/lib.rs`
/// `LOCATOR`): in the FreeRTOS window and assigned to no baked cell, so this
/// test cannot share a router with a concurrent `rtos_e2e` lane.
const ROUTER_PORT: u16 = 7850;

fn image(name: &str) -> std::path::PathBuf {
    let row = nros_tests::fixtures::groups::select_sole_row(ROW)
        .expect("loan-copy-count bench has a [[fixture]] row");
    nros_tests::fixtures::groups::row_resolved_dir(row).join(format!(
        "thumbv7m-none-eabi/{}/{name}",
        nros_cargo_profile::target_dir(nros_cargo_profile::FREERTOS_QEMU_PROFILE)
    ))
}

/// `key=value` from the one line that starts with `prefix`.
fn field(output: &str, prefix: &str, key: &str) -> Option<u32> {
    let line = output.lines().find(|l| l.contains(prefix))?;
    line.split_whitespace()
        .find_map(|kv| kv.strip_prefix(&format!("{key}=")))
        .and_then(|v| v.parse().ok())
}

#[test]
fn freertos_loan_publish_delivers_and_saves_the_payload_copy() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[nros_tests::matrix::PlatformId::FreertosMps2],
        "the FreeRTOS lending cell",
    );
    require_zenohd();
    let sub_image = image("loan-copy-sub");
    let pub_image = image("loan-copy-pub");
    for (label, b) in [("subscriber", &sub_image), ("publisher", &pub_image)] {
        nros_tests::fixtures::require_prebuilt_artifact(b, "just freertos build-fixtures")
            .require(&format!("loan-copy {label} image"));
    }

    let router = nros_tests::fixtures::or_unmet(ZenohRouter::start_slirp(ROUTER_PORT));
    // Subscriber first: zenoh delivers nothing published before a
    // subscription is declared.
    let mut sub_qemu = QemuProcess::start_mps2_an385_networked(&sub_image)
        .expect("start loan-copy SUBSCRIBER QEMU");
    std::thread::sleep(Duration::from_secs(5));
    let mut pub_qemu = QemuProcess::start_mps2_an385_networked(&pub_image)
        .expect("start loan-copy PUBLISHER QEMU");

    let pub_out = pub_qemu.collect_until("LOAN-COPY raw_publishes", Duration::from_secs(90));
    let sub_out = sub_qemu.collect_until("LOAN-COPY-RX", Duration::from_secs(60));
    drop(pub_qemu);
    drop(sub_qemu);
    drop(router);

    let get = |o: &str, p: &str, k: &str| {
        field(o, p, k).unwrap_or_else(|| {
            panic!(
                "no `{k}=` on the `{p}` line.\n--- publisher ---\n{pub_out}\n\
                 --- subscriber ---\n{sub_out}"
            )
        })
    };
    let raw_n = get(&pub_out, "LOAN-COPY raw_publishes", "raw_publishes");
    let raw_copies = get(&pub_out, "LOAN-COPY raw_publishes", "raw_copies");
    let loan_n = get(&pub_out, "LOAN-COPY raw_publishes", "loan_publishes");
    let loan_copies = get(&pub_out, "LOAN-COPY raw_publishes", "loan_copies");
    let rx_raw = get(&sub_out, "LOAN-COPY-RX", "raw");
    let rx_loan = get(&sub_out, "LOAN-COPY-RX", "loan");
    let rx_short = get(&sub_out, "LOAN-COPY-RX", "short");
    eprintln!(
        "lending on FreeRTOS: raw {raw_n} publishes / {raw_copies} payload copies, \
         loan {loan_n} publishes / {loan_copies} payload copies; received raw {rx_raw}, \
         loan {rx_loan}, short {rx_short}"
    );

    assert_eq!(
        loan_n, raw_n,
        "every loan must be granted on zenoh, the backend that lends (issue 0814)\n{pub_out}"
    );
    assert_eq!(rx_short, 0, "a sample arrived truncated\n{sub_out}");
    assert!(
        rx_loan * 2 >= loan_n,
        "the loan path delivered {rx_loan} of {loan_n} samples to a second image \
         (raw path: {rx_raw} of {raw_n})\n--- subscriber ---\n{sub_out}"
    );
    assert!(
        raw_copies >= raw_n,
        "`publish_raw` made {raw_copies} payload copies for {raw_n} publishes — fewer \
         than one each means the counter is not seeing the copies at all, so the loan \
         comparison below would be vacuous\n{pub_out}"
    );
    assert!(
        loan_copies + loan_n <= raw_copies,
        "the loan path made {loan_copies} payload copies for {loan_n} publishes, against \
         {raw_copies} for {raw_n} through `publish_raw`: it must save at least one copy \
         per publish, which is the whole point of lending (issue 0814)\n{pub_out}"
    );
}

/// RFC-0051 §7a/§7c — the one configured cell this file answers. Fixture-free.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Config, Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_config_cases,
    };
    assert_consumer_config_cases(
        Consumer::LendingFreertos,
        &[(
            (P::FreertosMps2, L::Rust, R::Zenoh, W::Pubsub, K::Example),
            &[Config::Lending],
        )],
    );
}
