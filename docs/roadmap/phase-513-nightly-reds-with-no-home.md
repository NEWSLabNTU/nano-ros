# Phase 513 — Nightly and merge-queue reds with no home

**Status (2026-10-24). In progress.** R1 and R2 are fixed on main (#2579);
their acceptance is a tier-2 nightly, because this bench is aarch64 and Zephyr
3.7's `native_sim` builds only on an x86 host. R5's instrumentation, R6 and R7
are in review. R3, R4 and the rest of R5 wait on CI evidence.

**Prior:** none. This phase gathers open issues whose phases were archived
while the issue stayed open, plus one new red found while reading them.

---

## Why a phase

On 2026-10-24 eleven open issues had no phase. Four were rated medium, which is
higher than any open issue in an unclaimed phase. Three of the four are reds in
the tier-2 nightly or the merge queue. A red lane cannot report a new
regression: the next failure looks like the last one. That is how issue 2032
arrived unnoticed, behind issue 2010 in the same Zephyr family. Clearing these
reds is what gives the nightly a verdict again.

## Work items

### R1 — the Zephyr mixed entry's archive gets its panic ending

[Issue 2010](../issues/2010-zephyr-mixed-entry-nros-c-has-no-panic-handler.md).

The mixed workspace's single linked archive is the `nros_ws_runtime` umbrella.
`zephyr/CMakeLists.txt` built its feature list from `_nros_cpp_features`
alone. The panic feature travels beside that string, in `_nros_panic_suffix`,
and the umbrella's `nros-cpp` dependency is `default-features = false`. So once
phase-359 W10 stage C removed `std`, and with it libstd's handler, the archive
had no `#[panic_handler]`. The cmake lane's umbrella already appends the
feature (`NanoRosRuntimeCrate.cmake`). The Zephyr lane now does the same.

**Done when** a tier-2 nightly builds `build-ws-mixed-entry-zenoh` and the
Zephyr mixed cell of `entry_e2e` runs.

### R2 — Zephyr's kernel arm defines the whole lock ABI

[Issue 2032](../issues/2032-zephyr-kernel-arm-lacks-lock-storage-probes.md).

The Zephyr port's non-`CONFIG_POSIX_API` arm left eight `<nros/platform.h>`
functions undefined. The native level runner (phase-509 P10) calls one of
them. On `native_sim` the linker resolved that call from the POSIX port that
`nros-rmw-xrce-cffi` bundles, and 56 duplicate symbols followed in every
Zephyr C++ XRCE leaf. The kernel arm now defines all eight.

The scheduled `run-matrix` hit the same collision in its C XRCE leaves, which
link `libnros_cpp.a` as well. That run was filed as issue 2025, which is
retired as a duplicate of 2032.

**Done when** a tier-2 nightly links the six `build-cpp-*-xrce` Zephyr leaves,
and a scheduled `run-matrix` links its `build-c-*-xrce` leaves.

### R3 — the six Zephyr Rust leaves pass `check-lock` on newslab-server-241

[Issue 1941](../issues/1941-zephyr-rust-leaf-locks-drift-after-seed.md).

The fix landed in `b28d9cd8e4`: `seed-lock` re-seeds a lock its realign cannot
move. What remains is the evidence the issue asks for.

**Done when** a tier-2 nightly that runs on `nano-ros-runner-newslab-server-241`
passes `check-lock` with the six leaves checked. The job log should show each
leaf being seeded.

### R4 — read the tier-2 nightly's failures, and file each cause

[Issue 1991](../issues/1991-tier2-nightly-keeps-no-junit-eleven-reds-unread.md).

Item 1 is done: the job uploads `tier2-nightly-junit` and prints the panic
text. Item 2 needs a night that gets past the fixture build, and R1 and R2 are
what stop it now.

**Done when** each of the issue's rows 4–14 is filed or attributed from that
night's text.

### R5 — tell a late reconnect from a hung one

[Issue 1976](../issues/1976-zenoh-reconnect-resume-exceeds-75s-in-queue.md).

`declares_keep_running_across_a_reconnect` should log how long the resume
actually took, even when it passes. Then queue runs carry the distribution, and
the issue can choose between the "load" fix and the "hang" fix.

The test now records when the first declare succeeded after the restart and
when the 50-round resume completed. A pass prints both. A timeout says whether
any declare succeeded ("late") or none did ("never resumed"). Showing a pass's
output needed one more change: the shim prints every failed declare with
`printf`, which is the test's stdout, about 63,000 lines per outage. So the
test sends fd 1 to a file during the churn and prints only the line count and
the last six lines. The 5 ms retry stays, because the declares must overlap
`_z_reopen`'s replay for the issue-1749 check to mean anything. The nextest
override shows the output on pass and keeps it in junit. Measured solo on this
bench: first declare and full resume both 39.85 s after the restart.

**Done when** the test prints the measured resume time on success (landed),
and the issue records the distribution from at least one merge-queue batch.

### R6 — a gate for a port arm that is short of the ABI

R2's class is a port with two arms where one arm omits functions that the
other defines. The kernel arm had been eight functions short since issue 0566.
Nothing noticed, because no caller reached the gap until P10.
`check-platform-abi-mirror` compares hand-written declarations and has no view
of which arm defines what.

**Done when** a buildless gate compares the function set of each arm of
`nros-platform-zephyr/src/platform.c`, has a negative control, and fails on a
function present in only one arm unless an entry with a reason allows it.

Landed as `just check platform-port-arms`
(`scripts/check/check-platform-port-arms.py`). It covers every tracked
`packages/platform/*/src/*.c`, not only Zephyr's, which is 15 files. Every
multi-arm conditional group must define the same `nros_platform_*` functions in
each arm. Nested groups are checked on their own and count toward their parent
arm. The self-test runs on every invocation. Run against the Zephyr port as it
stood before #2579, the gate reports exactly issue 2032's eight functions. The
`ALLOWED` table is empty.

### R7 — a gate for a Zephyr cargo build that drops the panic ending

R1's class is a feature list that reaches cargo without `_nros_panic_suffix`.
`check-image-panic-policy` reads only the Rust `nros::main!` side.

**Done when** a buildless gate fails on any `_nros_zephyr_api_cargo_build` or
`nros_write_runtime_umbrella_crate` call in `zephyr/CMakeLists.txt` whose
features omit the panic suffix, and has a negative control.

Landed as `just check zephyr-panic-suffix`
(`scripts/check/check-zephyr-panic-suffix.py`). It reads the file with comments
stripped. An umbrella's `CPP_FEATURES` may name the suffix through a variable,
and then the variable's last assignment before the call must carry it. A
direct `nros_cargo_build` with `FEATURES` must carry it too. One with no
`FEATURES` must build a generated `MANIFEST_PATH`. Run against the file as it
stood before #2579, the gate reports the umbrella line issue 2010 was about.

## Issues homed here

| Issue | Item | Severity |
| --- | --- | --- |
| [2010](../issues/2010-zephyr-mixed-entry-nros-c-has-no-panic-handler.md) | R1 | medium |
| [2032](../issues/2032-zephyr-kernel-arm-lacks-lock-storage-probes.md) | R2 | medium |
| [1941](../issues/1941-zephyr-rust-leaf-locks-drift-after-seed.md) | R3 | medium |
| [1991](../issues/1991-tier2-nightly-keeps-no-junit-eleven-reds-unread.md) | R4 | medium |
| [1976](../issues/1976-zenoh-reconnect-resume-exceeds-75s-in-queue.md) | R5 | medium |

## Not in this phase

The seven low or unrated homeless issues (1808, 1155, 1942, 1823, 1997, 1205,
1091) are not reds in a gating or nightly lane. On 2026-10-25 the four of them
still open and unhomed (1808, 1823, 1205, 1091) moved to
[phase-516](archived/phase-516-low-severity-issues-with-no-home.md), together with 0506.
