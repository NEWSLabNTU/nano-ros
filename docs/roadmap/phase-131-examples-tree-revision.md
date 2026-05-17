# Phase 131 — examples/ Tree Revision

**Goal.** Re-home every binary under `examples/` so the tree contains
**only copy-out user templates**. Tests, benches, soak loops, and
driver/board bringup smokes move into `packages/testing/`. Ad-hoc
top-level dirs collapse into the canonical
`<platform>/<language>/<rmw>/<example>` shape. Variant naming
normalizes to suffix form.

**Status.** Not started.

**Priority.** P2 — quality-of-life cleanup. Unblocks Phase 118
(example matrix coverage) by giving it a clean baseline tree, and
removes the perennial confusion between `core/` / `standalone/`
slots and real RMW slots.

**Depends on.** None. Touches `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`
which Phase 124 actively edits; coordinate sequencing.

**Related.** Phase 118 (example matrix coverage), CLAUDE.md
"Examples = Standalone Projects" section.

---

## Overview

`examples/` grew organically per phase. Today it holds three
distinct kinds of binary that should not share a roof:

1. **User-facing copy-out templates** — `<plat>/<lang>/<rmw>/<example>/`
   tree. The intended audience: someone porting nano-ros to a new
   board / language / RMW. These stay.
2. **Test / bench / soak binaries** — `cdr-test`, `wcet-bench`,
   `fairness-bench`, `stress-test`, `large-msg-test`. Built &
   invoked by `nros-tests` fixtures. Belong under
   `packages/testing/`.
3. **Driver / board bringup smokes** — `lan9118` (driver
   validation), `stm32f4 smoltcp` (TCP echo), `esp32 hello-world`
   (board bringup). No nros API. Belong with their driver / board
   crate or under a sibling `nros-smoke` test crate.

Plus five ad-hoc dirs that break the `<plat>/<lang>/<rmw>/<example>`
convention: `zephyr-aemv8r-cyclonedds/`, `multi-package-workspace/`,
`native/rust/bridge/`, `native/c/zenoh/baremetal-demo/`,
`px4/cpp/uorb/src/modules/<m>/`.

After this phase:

- `examples/<plat>/<lang>/<rmw>/<example>/` is the **only** shape
  inside platform dirs. `core/` and `standalone/` slots are gone.
- Cross-RMW templates live under `examples/bridges/` and
  `examples/templates/`.
- `examples/README.md` reflects reality (current README lists only
  zenoh and 6 of 12 platforms).
- Variant suffixes sort with peers (`talker-rtic` not `rtic-talker`).

---

## Architecture

### Target tree after R1+R2+R3

```
examples/
├── README.md                                      ← rewritten in R2
├── <plat>/<lang>/<rmw>/<example>/                 ← canonical
├── native/c/zenoh/custom-platform/                ← was baremetal-demo
├── native/c/zenoh/custom-transport-loopback/      ← keep
├── native/rust/zenoh/lifecycle-node/              ← keep (REP-2002)
├── native/rust/zenoh/custom-transport-{talker,listener}/  ← keep
├── native/rust/xrce/serial-{talker,listener}/     ← keep
├── qemu-arm-baremetal/rust/zenoh/serial-{talker,listener}/ ← keep
├── stm32f4/rust/zenoh/talker-embassy/             ← was core/embassy
├── bridges/                                       ← NEW top-level
│   └── native-rust-zenoh-to-dds/                  ← was native/rust/bridge/zenoh-to-dds
├── templates/                                     ← NEW top-level
│   └── multi-package-workspace/                   ← was top-level
├── zephyr/cpp/cyclonedds/talker-aemv8r/           ← was zephyr-aemv8r-cyclonedds/
└── px4/{cpp,rust}/uorb/<example>/                 ← was cpp/uorb/src/modules/<m>/
```

### Destination homes for relocated binaries

```
packages/testing/
├── nros-tests/                       ← existing
│   ├── tests/                        ← existing
│   └── bins/                         ← NEW — fixture binaries
│       ├── cdr-roundtrip-qemu/       ← was qemu-arm-baremetal/rust/core/cdr-test
│       └── lan9118-qemu/             ← was qemu-arm-baremetal/rust/standalone/lan9118
├── nros-bench/                       ← NEW Cargo workspace member
│   ├── wcet-cycles-qemu/             ← was qemu-arm-baremetal/rust/core/wcet-bench
│   ├── executor-fairness/            ← was native/rust/zenoh/fairness-bench
│   ├── stress-zenoh/                 ← was native/rust/zenoh/stress-test
│   ├── stress-xrce/                  ← was native/rust/xrce/stress-test
│   ├── large-msg-zenoh/              ← was native/rust/zenoh/large-msg-test
│   ├── large-msg-xrce/               ← was native/rust/xrce/large-msg-test
│   └── large-msg-baremetal/          ← was qemu-arm-baremetal/rust/zenoh/large-msg-test
└── nros-smoke/                       ← NEW — board/driver bringup
    ├── stm32f4-smoltcp-echo/         ← was stm32f4/rust/standalone/smoltcp
    └── esp32-hello-world/            ← was esp32/rust/standalone/hello-world
```

Each relocated binary keeps its standalone `Cargo.toml` +
`.cargo/config.toml` (CLAUDE.md "Examples = Standalone Projects"
contract still applies inside `packages/testing/`).

### Coupling surface

`packages/testing/nros-tests/src/fixtures/binaries/mod.rs`
hard-codes `examples/<path>` for every fixture binary. Movement
requires path edits there. The file currently references:

- `examples/qemu-arm-baremetal/rust/core/cdr-test`
- `examples/qemu-arm-baremetal/rust/standalone/lan9118` (via
  `QEMU_LAN9118_BINARY` plus `fixtures/binaries/mod.rs` resolver)
- `examples/native/rust/zenoh/{talker,listener,stress-test,custom-transport-{talker,listener},lifecycle-node}`
- `examples/native/rust/zenoh/{action,service}-{server,client}`
- `examples/qemu-esp32-baremetal/rust/{zenoh,dds}/<name>`

Just recipes touching example paths: `freertos.just`,
`native.just`, `qemu-baremetal.just`, `px4.just`,
`threadx-linux.just`, root `justfile`. Audit for path strings.

---

## Work Items

Work groups run in parallel where dependency arrows allow:

```
Group A (scaffold)  ─┬─→ Group B (relocate)  ─┬─→ Group D (variant rename)  ─→ Group E (docs)
                     │                         │
Group C (outliers)  ─┴───────────────────────  ┘
```

A and C are independent. B waits on A (needs destination crates).
D waits on B (touches the same fixture file; serialize edits). E
waits on B+C+D (README cannot describe final state earlier).

---

### Group A — Scaffold destination crates

Create the receiving Cargo packages first so Group B moves drop
into place. No file moves yet.

- [ ] A.1 — Create `packages/testing/nros-bench/` Cargo workspace
      member. Empty `Cargo.toml` declaring it as a virtual member;
      sub-bench crates land in B. Add to root workspace `members`.
      **Files:** `packages/testing/nros-bench/Cargo.toml`,
      `Cargo.toml` (root members list),
      `packages/testing/nros-bench/.gitignore` (`/target/`).
- [ ] A.2 — Create `packages/testing/nros-smoke/` Cargo workspace
      member same shape as A.1.
      **Files:** `packages/testing/nros-smoke/Cargo.toml`,
      `Cargo.toml` (root members),
      `packages/testing/nros-smoke/.gitignore`.
- [ ] A.3 — Create `packages/testing/nros-tests/bins/` directory
      with a top-level `README.md` explaining: "fixture binaries
      that `nros-tests` integration tests build & invoke. Not
      user-facing examples — see `examples/` for those."
      **Files:** `packages/testing/nros-tests/bins/README.md`.

### Group C — Tidy ad-hoc outliers in `examples/` (independent)

Five moves + one rename inside `examples/`. No touching of
`packages/testing/` so safe to run alongside A and B.

- [ ] C.1 — `examples/zephyr-aemv8r-cyclonedds/` →
      `examples/zephyr/cpp/cyclonedds/talker-aemv8r/`. Update
      `just zephyr build-fvp-aemv8r-cyclonedds` recipe to new path.
      **Files:** moved tree, `just/zephyr.just`, the doc references
      in the example's own README.
- [ ] C.2 — `examples/px4/cpp/uorb/src/modules/nros_register_check/` →
      `examples/px4/cpp/uorb/nros-register-check/`. Hoist the
      module out of the `src/modules/` sub-path so it matches
      `<plat>/<lang>/<rmw>/<example>` shape. Adjust the example's
      `CMakeLists.txt` for the new layout. Stub
      `examples/px4/rust/uorb/.gitkeep` + README explaining `just
      px4 build-sitl-rs` is the entry point once a Rust example
      lands.
      **Files:** moved dir, `examples/px4/cpp/uorb/CMakeLists.txt`,
      `just/px4.just` (path reference).
- [ ] C.3 — `examples/native/c/zenoh/baremetal-demo/` →
      `examples/native/c/zenoh/custom-platform/`. Rename
      `platform_impl.c` doc comments accordingly. Update example
      README opening line.
      **Files:** moved dir,
      `examples/native/c/zenoh/custom-platform/README.md`,
      `examples/native/c/zenoh/custom-platform/src/platform_impl.c`.
- [ ] C.4 — `examples/native/rust/bridge/zenoh-to-dds/` →
      `examples/bridges/native-rust-zenoh-to-dds/`. Create
      `examples/bridges/README.md` describing the category
      (cross-RMW gateways).
      **Files:** moved dir, `examples/bridges/README.md`.
- [ ] C.5 — `examples/multi-package-workspace/` →
      `examples/templates/multi-package-workspace/`. Create
      `examples/templates/README.md` describing the category
      (multi-platform copy-out recipes).
      **Files:** moved dir, `examples/templates/README.md`,
      `examples/templates/multi-package-workspace/README.md` (path
      hint adjustments).
- [ ] C.6 — `examples/stm32f4/rust/core/embassy/` →
      `examples/stm32f4/rust/zenoh/talker-embassy/`. The example
      uses zenoh; `core/` slot was a misclassification. Remove now-
      empty `examples/stm32f4/rust/core/` dir.
      **Files:** moved dir, `examples/stm32f4/rust/zenoh/talker-embassy/README.md`.

### Group B — Relocate test / bench / smoke binaries

10 movements. Within Group B each sub-item is independent on disk,
but **all touch
`packages/testing/nros-tests/src/fixtures/binaries/mod.rs`** —
serialize that file's edits or fold all path updates into one
commit at the end of B.

Sub-group B.1 (fixture-binaries, into `nros-tests/bins/`):

- [ ] B.1.1 — `examples/qemu-arm-baremetal/rust/core/cdr-test/` →
      `packages/testing/nros-tests/bins/cdr-roundtrip-qemu/`. Update
      `QEMU_TEST_BINARY` resolver path in `fixtures/binaries/mod.rs:172`.
- [ ] B.1.2 — `examples/qemu-arm-baremetal/rust/standalone/lan9118/` →
      `packages/testing/nros-tests/bins/lan9118-qemu/`. Update
      `QEMU_LAN9118_BINARY` resolver path.

Sub-group B.2 (benches, into `nros-bench/`):

- [ ] B.2.1 — `examples/qemu-arm-baremetal/rust/core/wcet-bench/` →
      `packages/testing/nros-bench/wcet-cycles-qemu/`. Update
      `QEMU_WCET_BENCH_BINARY` resolver path.
- [ ] B.2.2 — `examples/native/rust/zenoh/fairness-bench/` →
      `packages/testing/nros-bench/executor-fairness/`. Audit
      callers (none in fixture mod, but `nros-tests` tests may
      `cargo run` it directly).
- [ ] B.2.3 — `examples/native/rust/zenoh/stress-test/` →
      `packages/testing/nros-bench/stress-zenoh/`. Update the
      `examples/native/rust/zenoh/stress-test` ref in `fixtures/binaries/mod.rs:1154`.
- [ ] B.2.4 — `examples/native/rust/xrce/stress-test/` →
      `packages/testing/nros-bench/stress-xrce/`.
- [ ] B.2.5 — `examples/native/rust/zenoh/large-msg-test/` →
      `packages/testing/nros-bench/large-msg-zenoh/`. Update
      `nros-tests/tests/large_msg.rs` path refs.
- [ ] B.2.6 — `examples/native/rust/xrce/large-msg-test/` →
      `packages/testing/nros-bench/large-msg-xrce/`.
- [ ] B.2.7 — `examples/qemu-arm-baremetal/rust/zenoh/large-msg-test/` →
      `packages/testing/nros-bench/large-msg-baremetal/`.

Sub-group B.3 (smoke, into `nros-smoke/`):

- [ ] B.3.1 — `examples/stm32f4/rust/standalone/smoltcp/` →
      `packages/testing/nros-smoke/stm32f4-smoltcp-echo/`. Remove
      now-empty `examples/stm32f4/rust/standalone/` dir.
- [ ] B.3.2 — `examples/esp32/rust/standalone/hello-world/` →
      `packages/testing/nros-smoke/esp32-hello-world/`. Remove
      now-empty `examples/esp32/rust/standalone/` dir.

Sub-group B.4 (cleanup):

- [ ] B.4 — After B.1–B.3, `examples/<plat>/<lang>/core/` and
      `examples/<plat>/<lang>/standalone/` slots should be empty;
      remove the directories. Audit just recipes referencing
      `examples/qemu-arm-baremetal/rust/{core,standalone,zenoh}/{cdr-test,wcet-bench,lan9118,large-msg-test}` —
      `freertos.just`, `native.just`, `qemu-baremetal.just`, root
      `justfile`.

### Group D — Variant suffix normalization (after B)

Mass rename. Touches the same fixture-binaries file as B; do not
overlap.

- [ ] D.1 — `<plat>/rust/zenoh/rtic-{talker,listener,service-server,service-client,action-server,action-client,mixed-talker,mixed-listener}/`
      → `<plat>/rust/zenoh/{talker,listener,service-{server,client},action-{server,client},mixed-talker,mixed-listener}-rtic/`.
      Affects `stm32f4`, `qemu-arm-baremetal`, `native`.
- [ ] D.2 — `<plat>/rust/<rmw>/async-{service-client,action-client}/` →
      `<plat>/rust/<rmw>/{service-client,action-client}-async/`.
      Affects `native`, `zephyr`.
- [ ] D.3 — Update `fixtures/binaries/mod.rs` for every renamed
      path. Sweep `just/*.just` and root `justfile` for old names.
- [ ] D.4 — `cargo nextest run -E 'package(nros-tests)'` to verify
      fixture builds resolve.

### Group E — Documentation refresh (after B+C+D)

- [ ] E.1 — Rewrite `examples/README.md` from scratch: current
      matrix of platforms × languages × RMWs × cases, new top-level
      categories (`bridges/`, `templates/`), pointer to
      `packages/testing/nros-{bench,smoke}/` for tests/benches.
- [ ] E.2 — Per-dir `.gitignore` audit. Every C/C++ example dir
      needs `/build/`; every Rust dir needs `/target/` plus any
      `--target-dir=target-<suffix>/` variants. Spot-check confirmed
      ✓ for `native/cpp/*` and `qemu-arm-freertos/cpp/*` — extend to
      every C/C++ tree.
- [ ] E.3 — Update CLAUDE.md `## Practices` section if the
      `examples/` rules change wording. The "Examples = Standalone
      Projects" section already applies to `packages/testing/nros-{bench,smoke}/`
      destinations; no rewrite expected.

---

## Acceptance

- [ ] `find examples -type d -name core -o -type d -name standalone -o -type d -name bridge`
      returns nothing.
- [ ] `find examples -mindepth 4 -maxdepth 4 -type d` returns only
      paths matching `<plat>/<lang>/<rmw>/<example>` plus the
      explicit exceptions (`examples/bridges/<name>/`,
      `examples/templates/<name>/`, `examples/README.md`).
- [ ] `packages/testing/nros-{bench,smoke}/` directory exists; each
      relocated binary builds in isolation with `cargo build -p <name>`.
- [ ] `just ci` passes (existing fixtures resolve the new paths;
      moved benches build green).
- [ ] `examples/README.md` enumerates every `<plat>/<lang>/<rmw>`
      cell with current presence/absence — drives Phase 118.
- [ ] No just recipe references a stale path under `examples/`.

---

## Notes

- **Out of scope.** Filling RMW coverage gaps (DDS C/C++ on RTOS
  QEMU, XRCE embedded, CycloneDDS POSIX) — that is Phase 118.
  Phase 131 only carves the tree so Phase 118 inherits a clean
  baseline.
- **CLAUDE.md "Examples = Standalone Projects" still applies**
  inside `packages/testing/nros-{bench,smoke}/`. Each relocated
  binary keeps its own `Cargo.toml` + `.cargo/config.toml` and
  builds independently of the workspace.
- **Git history** is preserved via `git mv`. Aggregate per
  sub-group into single commits so `git log --follow` reads
  cleanly.
- **Coordination with Phase 124.** Phase 124 actively edits
  `fixtures/binaries/mod.rs`. Land Group B in a single commit
  scoped to fixture path updates; rebase Phase 124 atop it (or vice
  versa) to avoid three-way conflicts on the cached `OnceCell`
  block.
- **Coordination with Phase 130.** Phase 130 swaps the executor
  wake primitive — touches `Executor::spin_once` but no example
  paths. Independent.
