# Phase 316 — every example path level names a real axis

**Status (2026-07-28): Draft.** Not started.
**Implements:** RFC-0026 (example directory layout).
**Closes:** issue 0315. **Informed by:** issues 0314, 0319, archived 0295.

## Goal

Make RFC-0026's rule true as written — *"there are no `<rmw>/` siblings"* —
by removing the last three `<rmw>/` levels, and end
`scripts/check-example-matrix.sh`'s carve-out list.

Two of the three are not RMW levels at all. px4's `uorb/` and `xrce/` encode
**where the code runs** (in-firmware vs companion), which is a real axis wearing
a backend's name. Renaming it to say so is the fix; flattening it would destroy
information.

```
px4/cpp/uorb/…      ->  px4/cpp/firmware/…     in-firmware PX4 modules, uORB bus, no serialization
px4/rust/xrce/…     ->  px4/rust/companion/…   nano-ros nodes beside PX4; RMW pinned by uxrce_dds_client
zephyr/*/cyclonedds/talker-aemv8r
                    ->  zephyr/*/talker-aemv8r  board variant, per the -aemv8r suffix convention
```

## Ordering principle

**The renames do not depend on the undecided bridge question.** W1–W3 are
mechanical and land first; W4 needs a decision recorded before any code.

## Work items

### W1 — px4: rename the level to its axis

- **W1.1** `git mv examples/px4/cpp/uorb examples/px4/cpp/firmware`,
  `git mv examples/px4/rust/xrce examples/px4/rust/companion`.
- **W1.2** Update the ~29 referencing files: root `Cargo.toml` (exclude
  entries), `just/px4.just`, `scripts/build/compile-check-fixtures.sh`,
  `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`,
  `packages/testing/nros-px4-sitl-test/tests/px4_xrce_e2e.rs`,
  `docs/reference/px4-xrce-companion.md`, `book/src/getting-started/{px4,integration-px4}.md`,
  `integrations/px4/README.md`, `examples/px4/README.md`, `examples/README.md`.
- **W1.3** Delete the px4 structural exemption from `is_allowed()` in
  `scripts/check-example-matrix.sh`, plus the explanatory comment block.

**Acceptance:** `just px4 build-examples` and `just px4 build-fixtures` green;
`check-example-matrix.sh` no longer needs the px4 branch.

### W2 — zephyr: flatten the board variant

- **W2.1** `git mv` both `zephyr/{rust,cpp}/cyclonedds/talker-aemv8r` up one
  level to `zephyr/{rust,cpp}/talker-aemv8r`.
- **W2.2** Update the ~28 referencing files: root `Cargo.toml`,
  `just/zephyr-setup.just`,
  `packages/testing/nros-tests/tests/examples_fixture_coverage.rs`,
  `book/src/getting-started/arm-fvp.md`, RFC-0026, and the phase-217 /
  phase-275-276 notes.
- **W2.3** Delete both `allowed_roots` lines.

**Acceptance:** `allowed_roots` is EMPTY and the script still passes; the
aemv8r fixture still builds.

### W3 — the non-example, and the drifted docs

- **W3.1** Move `nros-register-check` out of `examples/`. Its own header says
  *"the build itself is the validation"* — it is a link/registration assertion,
  and CLAUDE.md puts non-example binaries under `packages/testing/`. Decide
  between a testing fixture and an `examples/fixtures.toml` build-step
  assertion; either way it stops being an "example".
- **W3.2** Fix `examples/bridges/README.md`, which still describes the retired
  `<plat>/<lang>/<rmw>/<example>` form.
- **W3.3** RFC-0026: record that px4's level is a deployment axis, so a future
  reader does not "helpfully" flatten it back.

**Acceptance:** `examples/` contains only examples; no doc describes the
retired path form.

### W4 — uORB interop example + bridge (BLOCKED on a decision)

Two things that do not exist yet, and one open question that gates them.

- **W4.1** *Decide the bridge's purpose.* PX4 already ships
  `uxrce_dds_client`, which is exactly uORB → XRCE-DDS, so a nano-ros
  uORB→DDS bridge duplicates it. The non-duplicative framings:
  - **uORB → Zenoh** — PX4 has nothing there; the bridge is genuinely new
    capability.
  - **registry demonstration** — the point is nano-ros's multi-RMW registry
    running in-firmware, and the translation is incidental.

  Record the answer in this doc before writing code.
- **W4.2** Write the uORB **interop** example: a nano-ros node exchanging with
  a stock PX4 app over uORB, demonstrating the zero-serialization path. This is
  what `examples/px4/cpp/firmware/` should contain after W3.1 empties it.
- **W4.3** Write the bridge under `examples/bridges/`, per W4.1.

**Acceptance:** a reader can see uORB interop working against an unmodified PX4
app, and one bridge translating uORB to a networked backend.

## Risks

- **Reference sweep is the whole cost of W1–W2.** ~57 files across `Cargo.toml`
  excludes, just recipes, fixture builders, tests, the book and RFCs. A missed
  reference fails loudly at build time (a path that does not exist), which is
  the good case; the bad case is a stale doc nobody notices — so grep for the
  old paths in `docs/` and `book/` explicitly, not just in code.
- **Fixture rebuild.** Renaming an example directory changes its fixture path;
  per CLAUDE.md any prebuilt fixture keyed on the old path reads stale. Rebuild
  the px4 and zephyr fixture families after W1/W2 rather than debugging a
  "runtime" failure.
- **W4 is not a rename.** It is new example code plus, for the bridge, the
  first non-POSIX entry in `examples/bridges/` (uORB is an in-process bus, so
  the bridge is a C++ PX4 module). Do not scope it with W1–W3.
- **Concurrent sessions.** Other agents are active in this repo; land each W in
  small pushed steps.

## Receipts to collect

| Step | Receipt |
| --- | --- |
| W1 | `just px4 build-examples` + `build-fixtures` green; px4 branch gone from `is_allowed()` |
| W2 | `allowed_roots` empty; `check-example-matrix.sh` passes; aemv8r fixture builds |
| W3 | `examples/` free of non-examples; no doc mentions `<plat>/<lang>/<rmw>/` |
| W4 | interop demo runs against a stock PX4 app; bridge translates uORB → networked backend |
