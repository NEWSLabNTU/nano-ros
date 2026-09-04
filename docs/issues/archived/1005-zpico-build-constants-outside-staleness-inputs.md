---
id: 1005
title: "A zenoh constant that lives in `nros-zpico-build` is invisible to the
  fixture staleness probe, so a fixture baked before a fix reports FRESH"
status: resolved
type: bug
area: testing, build
severity: high
found: 2026-09-03
related: [issue-0877, issue-0906, issue-0196, issue-0911, issue-0627, issue-0442, phase-340, phase-363, phase-414]
---

## What happens

The zenoh fixture staleness arm resolves its inputs from the
`cargo:rerun-if-changed` lines that `zpico-sys`'s build script RECORDED
(`packages/testing/nros-tests/src/fixtures/binaries/mod.rs`). Dumping the
recorded set for the FreeRTOS C talker gives 28 in-repo entries, and **none of
them is under `packages/rmw/zenoh/nros-zpico-build/`**.

That crate is a build-script DEPENDENCY. Cargo tracks it correctly through its
own unit graph — change it and cargo rebuilds — but it never appears as a
`rerun-if-changed` PATH, and the path list is what the probe reads. So a
constant that lives there is outside everything the probe examines.

## Measured

`Z_TRANSPORT_LEASE_MS = 60_000` at
`packages/rmw/zenoh/nros-zpico-build/src/lib.rs:289` (issue 0906's fix,
2026-08-30). Every built FreeRTOS fixture in the tree still baked the old value:

    examples/qemu-arm-freertos/{c,cpp}/*/build-zenoh/cargo/.../zpico-sys-*/out/
        zenoh-config/zenoh_generic_config.h
        #define Z_TRANSPORT_LEASE 10000      <- all 20 of them

    examples/qemu-arm-freertos/c/talker/build-zenoh/c_talker   mtime 2026-08-21

Source said 60000. Binaries said 10000. The probe reported FRESH.

## Why it matters more than a stale binary

Issue 0906 measured what the old value costs on exactly these images: **19 heard
of 77** before, **77 of 77** after, because a 10 s lease against a router
keep-alive on a 30 s cadence expires deterministically. So the probe was
reporting FRESH on binaries that carry a known, measured, delivery-breaking
defect.

Found while rediagnosing issue 0877 (phase-414 W1), whose "0 messages received"
is very likely this: the report is dated one day before 0906's fix, and the
fixtures on disk had not moved since.

**This is the shape CLAUDE.md already names**: "Build-side stale probes must
watch the same inputs as test-side gates — a probe that misses `generated/**`
lets a museum binary pass every sweep" (issue 0196). Same rule, a different
input class: not a generated tree, but a build-script dependency crate.

## Why the usual reasoning does not save it

* It is not a `--target-dir` or profile confusion (issue 0488's class): the
  binaries are in the right place, they are simply old.
* It is not the exemption machinery (issue 0442/0445): nothing is exempted here.
  The input was never a candidate.
* A STALE verdict is absorbing and loud; a FRESH verdict is silent. This is the
  silent direction, which is the worse one.

## Confirmed independently 2026-09-03

A sanctioned `just build freertos` flipped every live FreeRTOS zenoh fixture
from `10000` to `60000`. The binaries it replaced were dated **2026-08-20**, ten
days before the fix that changed the constant, and the probe reported FRESH for
all of them. Measured, not inferred.

**And a second gap found while verifying it:** the cell that would notice the
old value cannot. `test_rtos_pubsub_e2e` FreeRTOS kills its talker after 15 s,
so it emits 12 publishes and the first lease lapse (~20 s of session life) never
arrives — a build baked at `10000` passes it 6 of 6. So the constant is
unprotected in BOTH directions: the probe cannot see it change, and the cell
cannot see it be wrong. Fixing the probe alone leaves the second half open.

---

# Fixed, probe half — 2026-09-04

## Which direction, and why

Direction (1) — **ask cargo for the build closure** — but resolved from what
cargo already wrote on disk rather than from `cargo metadata` or a generated
list.

The recorded-path arm answers "what did the build script READ". The missing
class is "what was the build script COMPILED FROM", and no amount of emitting
fixes that in general, because the closure is TRANSITIVE: `nros-zpico-build`
itself build-depends on `nros-cc-flags`, `nros-board-common`,
`nros-build-paths` and `nros-zephyr-build`, and a constant in any of those has
the identical defect. Direction (2) would need each consuming build script to
enumerate a closure a build script cannot compute — `cargo metadata` inside a
build script takes the package-cache lock cargo is already holding, the exact
constraint issue 0627 records. Direction (3) watches one generated header and
nothing else.

Cargo's dep-info for the corrosion staticlib carries the whole thing.
Measured on `examples/qemu-arm-freertos/c/talker/build-zenoh` (2026-09-04),
`<group>/<triple>/<profile>/libnros_c.d` lists **236 in-repo inputs**: the
target-side crates (`packages/core/**`, 65), the C shim the `cc` crate compiled
(`zpico-sys/c/**`, 14), and the build-script closure —
`nros-zpico-build/src/lib.rs`, `nros-board-common/src/*`,
`nros-cc-flags/src/lib.rs`, `nros-build-paths/src/lib.rs`.

So this is phase-363's "ask the tool that owns the graph" one level up, read
through the same `.d` reader the pure-cargo and Zephyr arms already use. It is
DERIVED, not authored: nothing enumerates a crate, so a build script that gains
a dependency tomorrow is covered without anyone remembering. That is why it
needs no generated list plus a drift gate the way issue 0627's CLI closure did
— there is no authored artifact here to keep honest.

`deps/*.d` is deliberately NOT read: those name paths relative to the cargo
invocation's cwd, which is the issue-0696 trap. The staticlib `.d` beside the
`.a` is absolute and is already their union.

## The self-declaration fix landed separately, and is kept

A narrower fix for the same issue reached `main` first (commit `50819ce`,
direction 2): `nros_zpico_build::runner::run()` emits its own sources as an
input,

    println!("cargo:rerun-if-changed={}/src", env!("CARGO_MANIFEST_DIR"));

measured to raise the recorded set from 28 entries to 29, with
`packages/rmw/zenoh/nros-zpico-build/src` among them. It is emitted from the
DEPENDENCY rather than from each consumer, so a future consumer inherits it.

That line is untouched here and stays. It is subsumed by the dep-info arm above
-- which covers the TRANSITIVE closure the self-declaration cannot reach -- but
it costs nothing and it keeps the recorded-path arm honest for the one crate it
names. The dep-info arm is what the acceptance below exercises.

## A second defect found on the way, and fixed with it

`find_build_script_outputs` walked with `DirEntry::file_type()`, which LSTATs.
Since the phase-340 shared cargo group dir landed, a cross-compiled leaf's
`build-<rmw>/cargo` is a **symlink** into
`build/corrosion-cargo/<platform>/<hash>/`, so the walk answered "not a
directory" and stopped there. Measured 2026-09-04: `zpico_recorded_inputs`
returned **0 entries** for every FreeRTOS / NuttX / ThreadX fixture (28 once the
walk STATs instead), so the probe had silently been running the hand-authored
bootstrap walk that its own doc comment calls unreachable — across the entire
cross-compiled half of the tree.

The bootstrap arm announces itself through `staleness::probe_accounting()`,
which is rendered only inside a STALE message. On the FRESH path — the
direction that matters — it said nothing at all. That asymmetry is still open
(see below).

## Acceptance, run 2026-09-04

The counterfactual from this issue, on
`examples/qemu-arm-freertos/c/talker/build-zenoh/c_talker`. The artifact's
mtime was first advanced so the probe had a clean FRESH baseline (the tree
carried an unrelated `zpico.c` mtime bump); everything was restored afterwards,
binary byte-identical.

    1. constant unchanged                      -> FRESH
    2. Z_TRANSPORT_LEASE_MS 60_000 -> 45_000,
       NO rebuild                              -> STALE packages/rmw/zenoh/nros-zpico-build/src/lib.rs
    3. same edit, cargo arm disabled
       (pre-fix behaviour)                     -> FRESH

Membership, measured on the same fixture: the pre-fix arms walk 1777 candidates
and `nros-zpico-build/src/lib.rs` is in none of them; the new arm walks 271 and
it is there.

Regression tests (`packages/testing/nros-tests/src/fixtures/binaries/mod.rs`),
all hermetic, all verified to FAIL with the fix reverted:

* `a_build_script_dependency_crate_is_a_staleness_input`
* `the_cmake_probe_consults_the_cargo_input_arm` (the wiring, per issue 0196 —
  the arm being right does not mean the probe calls it)
* `the_build_script_record_is_found_across_a_symlinked_cargo_dir`

## Sibling sweep — the class is "a build-script dependency crate"

Measured across all four freshness entry points:

| arm | input source | sees build-script deps? |
| --- | --- | --- |
| `require_prebuilt_binary_fresh` / `_row_` | `<binary>.d` | YES — `talker.d` lists `nros-zpico-build/src/{lib,runner}.rs` |
| `require_prebuilt_binary_fresh_zephyr` | `librustapp.d` | YES — 209 in-repo inputs incl. `nros-zpico-build`, `nros-cc-flags`, `nros-build-paths`, `nros-zephyr-build` |
| `require_prebuilt_binary_fresh_cmake` | `ninja -t deps` + recorded `rerun-if-changed` | **NO** — this issue |

Cargo folds the build-script closure into the dep-info of any unit it emits
one for, so the three arms that read a `.d` were never blind. The cmake arm
read neither, because corrosion is one opaque ninja edge. One affected family,
and it is the one measured.

The other in-repo build-dependency crates — `nros-board-common`,
`nros-cc-flags`, `nros-build-paths`, `nros-zephyr-build`,
`nros-build-helpers`, `nros-board-threadx-port-riscv64` — are consumed by ~20
crates and are now covered by the same arm, because the fix names none of them.

## Consequence worth stating

A C/C++ cmake fixture now watches the whole Rust closure it links (~140–410
in-repo files depending on the leaf), not just its C surface. That is correct —
those sources are IN the binary — and it was silently unwatched before. It also
means a real edit to a core crate will make every C/C++ cmake fixture report
STALE. Measured 2026-09-04 across all 101 cmake fixture binaries: at the mtime
level the pre-existing arms already flagged 101 of 101, so the new arm adds no
verdict today; the difference it makes is at the CONTENT level, where the
candidate set is what decides.

## Remaining — this issue stays open

The second half is untouched: `test_rtos_pubsub_e2e` FreeRTOS still cannot
observe a wrong lease, because it kills its talker before the first lapse. The
probe can now see the constant change; no cell can see it be wrong.

Also open, found here: `staleness::probe_accounting()` — which carries the
"INPUT SET UNMEASURED" announcement — is rendered only in a STALE message, so a
degraded probe is invisible on the FRESH path. That is what let the symlink
defect run for the whole cross-compiled tree unnoticed.
