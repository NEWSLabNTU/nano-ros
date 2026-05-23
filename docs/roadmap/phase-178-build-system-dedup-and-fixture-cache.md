# Phase 178 - Build system de-dup and fixture cache discipline

**Goal.** Make `just build-all` a true staged superset without rebuilding
the same platform examples/fixtures twice, and restore safe incremental
reuse for CMake fixture build dirs.

**Status.** In progress. Created from the 2026-05-24 build-all review.

**Priority.** P2 (developer and CI wall-clock).

**Depends on.** Phase 174/176 build-performance work. Keep the current
Zephyr model: every `examples/zephyr/<lang>/<role>` remains a standalone
user-copyable app and each E2E fixture builds that app as its own image.

## Findings

### Duplicate build-all stages

`just build-all` currently does too much by composition:

- The static path runs root `just build-examples`, then
  `just build-test-fixtures`.
- The jobserver path (`build-all.mk`) runs root `build-examples` and all
  per-platform fixture targets concurrently.
- Root `build-examples` calls per-platform `build-examples` for native,
  FreeRTOS, ThreadX Linux, and ThreadX RV64.
- Those platforms' `build-fixtures` recipes rebuild many of the same
  role examples again, often into the same target dirs.

Concrete overlaps:

- FreeRTOS `build-examples` builds Rust roles into `target-zenoh`;
  FreeRTOS `build-fixtures` builds those same Rust roles into
  `target-zenoh` again before adding C/C++ fixtures.
- ThreadX Linux and ThreadX RV64 have the same Rust `target-zenoh`
  overlap.
- Native `build-examples` auto-discovers many standalone Cargo examples;
  native `build-fixtures` rebuilds fixture variants, with some default
  target-dir overlap and several feature-specific target dirs.
- In the jobserver path, these can run at the same time, creating both
  wasted work and possible same-dir Cargo contention.

### Root build-examples naming

Root `build-examples` is useful as aggregate UX, but its current scope is
not "all platforms". It builds root workspace prerequisites plus native,
FreeRTOS, ThreadX Linux, and ThreadX RV64 examples. It does not include
Zephyr, NuttX, ESP32, PX4, and other platform example tiers. The name is
fine for public UX, but `build-all` should not use it blindly as an
internal dependency if fixture tiers already cover those examples.

### CMake build-zenoh dirs are per-example, not shared

`build-zenoh/` does not collide across examples because it lives under
each example directory, for example:

```text
examples/qemu-arm-freertos/c/talker/build-zenoh
examples/qemu-arm-freertos/c/listener/build-zenoh
```

The problem is not cross-example overlap. The problem is that fixture
recipes delete these dirs on every run:

```sh
rm -rf "$dir/build-zenoh"
cmake -S "$dir" -B "$dir/build-zenoh" ...
cmake --build "$dir/build-zenoh"
```

That protects against stale CMake cache/toolchain/platform/codegen
state, but it throws away incremental build reuse. Zephyr already has a
better pattern: record a small signature beside the build dir and only
reconfigure when board/source/RMW/toolchain/codegen inputs change.

### build-zenoh-posix-fixture placement

`build-zenoh-posix-fixture` exists for deterministic test artifacts:

- `target-zenoh-fixture-posix/release/libnros_rmw_zenoh_staticlib.a`
- generated `zenoh_generic_config.h`

The tests `zenoh_archive_symbols.rs` and `zenoh_header_parity.rs` inspect
those artifacts and must not build them inside test bodies. It is valid
as a fixture prerequisite, but the current jobserver graph runs prereqs
and then root `build-examples`, whose `build` dependency can repeat some
of the same broad setup work. This should be made explicit and
non-duplicating.

## Plan

- [x] **178.A — split aggregate UX from internal build-all graph.**
  Keep public `just build-examples`, but make `build-all` use internal
  leaf targets so the same platform examples are not rebuilt by both
  `build-examples` and `build-fixtures`.

- [x] **178.B — define fixture-only platform targets.** For platforms
  where `build-fixtures` currently rebuilds the normal role examples,
  split into:
  - `build-examples`: user-facing example compile smoke.
  - `build-fixtures`: full test fixture staging.
  - internal `build-fixture-extras`: only feature variants, C/C++ cells,
    smoke binaries, and test-only artifacts not already covered by
    `build-examples`.
  Done for FreeRTOS, ThreadX Linux, and ThreadX RV64: public
  `build-fixtures` now depends on `build-examples` plus
  `build-fixture-extras`, and the extras target omits the duplicate
  normal Rust role loop where `build-examples` already produces the same
  `target-zenoh` artifacts. Native is split into a narrower
  `build-fixture-role-examples` plus `build-fixture-extras`, because
  public `native build-examples` is a broad repository-level Rust
  example aggregate rather than a native-only fixture role target.

- [x] **178.C — fix jobserver DAG.** In `build-all.mk`, avoid running
  root `build-examples` concurrently with platform fixture targets that
  write the same target dirs. Use ordered or disjoint targets:
  prerequisites first, example tiers once, fixture extras once.
  Landed 2026-05-24: root `build-all` no longer launches public
  `build-examples`; it runs `build-example-extras` plus platform fixture
  leaves. Platforms with shared target dirs sequence their own
  `build-examples`/fixture-extra split inside `build-fixtures`.

- [x] **178.D — add CMake fixture signatures.** Replace unconditional
  `rm -rf build-zenoh` in FreeRTOS, NuttX, ThreadX Linux, and ThreadX
  RV64 C/C++ fixture recipes with a signature file covering:
  platform, RMW, toolchain file, SDK/config dirs, codegen binary,
  source dir, and CMake cache inputs. Reconfigure on signature change;
  otherwise run `cmake --build` directly.
  Landed 2026-05-24: shared `nros_cmake_fixture_build` records
  `.nros-cmake-fixture.sig` in each CMake fixture build dir. The four
  embedded C/C++ fixture recipes now reuse unchanged `build-zenoh/` or
  `build-<rmw>/` dirs and only wipe/reconfigure on signature changes.

- [ ] **178.E — keep Zephyr standalone app coverage.** Do not collapse
  Zephyr roles into one runtime-dispatch image for this phase. The E2E
  value is that `examples/zephyr/<lang>/<role>` builds as a user-copyable
  standalone project.

- [x] **178.F — document timing output.** Preserve or improve the
  `build-test-fixtures` timing output and add equivalent stage timing
  for jobserver `build-all.mk`, so future regressions show which
  platform or fixture tier got slower.
  Done with per-run log directories to avoid concurrent runs overwriting
  shared files:
  - static `build-test-fixtures`: `tmp/build-test-fixtures-*/`
    contains `build-test-fixtures.joblog`, `parallel.joblog`, and
    `zephyr.log`; `tmp/build-test-fixtures-latest` points at the newest
    run.
  - jobserver `build-all.mk`: `tmp/build-all-*/build-all.joblog`
    records prereqs, root examples, and each platform fixture stage;
    `tmp/build-all-latest` points at the newest run.

- [x] **178.G — stop building examples in test-only prerequisite paths.**
  Audit platform `test` / `test-all` dependencies that still pull in
  `build-examples` even though the tests consume binaries staged by
  `build-fixtures`. Keep public `just <platform> test` convenient, but
  avoid compiling examples twice in the `build-all`/`test-all` flow.

- [ ] **178.H — make native CMake examples incremental.** Native C/C++
  helper recipes still use `rm -rf build && cmake ...` in some paths.
  Apply the same signature-based reuse planned for embedded
  `build-zenoh/`: reconfigure only when source/RMW/toolchain/codegen
  inputs change, otherwise run `cmake --build`.

- [x] **178.I — reduce root build repetition inside aggregate paths.**
  Root `build-examples` depends on root `build`, while the jobserver
  graph already has explicit prereqs (`build-workspace`,
  `generate-bindings`, `build-zenoh-posix-fixture`). Split internal
  aggregate targets so public UX stays simple but `build-all` does not
  rerun broad setup work.
  Landed 2026-05-24: public `build-test-fixtures` keeps its
  self-contained `generate-bindings` and `build-zenoh-posix-fixture`
  prereqs, while root `build-all` calls internal
  `build-test-fixtures-leaves` after `build` has already run those
  prerequisites.

- [x] **178.J — Zephyr Rust generated-dir preflight.** Before launching
  expensive Zephyr fixture builds, verify or regenerate the Rust
  `generated/<pkg>/` dirs for Zephyr Rust examples. This catches missing
  generated message crates before the Zephyr graph schedules kernel,
  picolibc, and link work.
  Done in `just zephyr build-fixtures`: before the west fixture matrix
  is scheduled, the recipe builds the canonical `nros` CLI if needed
  and runs `nros generate-rust` for any Zephyr Rust example with a
  missing or older `generated/Cargo.toml`. Set
  `NROS_ZEPHYR_GENERATE_RUST_FORCE=1` to force refresh all Zephyr Rust
  example bindings.

- [x] **178.K — survey NuttX make fixture tier.** `nuttx
  build-fixtures-make` validates the native NuttX external-app
  integration path that CMake fixtures bypass, but it may not need to be
  part of every `build-all` if that target is meant to pre-stage
  `test-all` artifacts. Decide whether it belongs in `build-all`, a
  slower `build-all-full`, or CI-only coverage.
  Decision: keep it out of standard `nuttx build-all` and root
  `build-all`; those pre-stage artifacts consumed by the normal
  `test-all` flow, while `nuttx_make_e2e` already skips unless the
  make-built `$NUTTX_DIR/nuttx` was explicitly staged. Added
  `just nuttx build-all-full` as the opt-in tier for the slower
  Kconfig/Application.mk external-app coverage.

- [ ] **178.L — make generate-bindings incremental.** `generate-bindings`
  currently builds `nros-cli` and runs `generate-rust --force` across
  discovered example `package.xml` dirs. Add a stamp/hash over
  `package.xml`, generator version, requested interfaces, and source
  interface files so unchanged examples skip regeneration while ROS
  interface upgrades still force correct output refresh.

## Acceptance

- `just build-all` does not launch root `build-examples` and the same
  platform's duplicate role fixture builds in the same run.
- Jobserver and static paths produce the same artifacts needed by
  `just test-all`.
- C/C++ platform fixtures reuse `build-zenoh/` when their signature is
  unchanged.
- `just build-test-fixtures` still leaves
  `zenoh_archive_symbols.rs`/`zenoh_header_parity.rs` artifacts available.
- Zephyr fixture count and one-image-per-example behavior stay unchanged.
- Missing Zephyr Rust generated dirs fail or regenerate before any
  expensive Zephyr image build starts.
- Warm `generate-bindings` skips unchanged examples.
