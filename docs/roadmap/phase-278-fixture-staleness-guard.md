# Phase 278 — Resolver-level fixture staleness guard

Status: **In progress — 2026-07-06** · Implements issue #147 · Informs issues
#146, #140, #129, #132 (the recurring stale-fixture failure class).

> **Goal.** A test that consumes a prebuilt fixture fails LOUD ("… is stale, run
> `just build-test-fixtures`") the moment the fixture's source is newer than the
> built binary — under ANY launcher, including a bare `cargo nextest run`, not
> just the `just test-all` preflight. No test silently runs a stale binary again.

## Why

Fixture staleness is detected today only in the `just test-all` preflight
(`scripts/check-fixtures-stale.sh`), and only as WARN + self-heal. A bare
`cargo nextest run` — the normal dev/debug loop — skips it entirely:
`require_prebuilt_binary` (`nros-tests/src/fixtures/binaries/mod.rs:391`) is a
pure existence check. This has repeatedly masked real bugs behind stale
binaries:

- **#146** — a stale `native/rust/listener` (pre-W4 `Int32_` vs current
  `String_`) gave 0-received "ros2→nano broken" under bare nextest before the
  real QoS defect was reachable; the talker fixture was stale too.
- **#129 / #140** — "stale June prebuilts masked months of lane rot" in both
  root-cause writeups.

The audit (issue #147) mapped ~318 fixtures across three tiers: workspace
fixtures (~70) already guard via a resolver-enforced `.inputsig` content hash;
plain single-node (243) have only the preflight; zephyr-workspace (9) + all
non-cargo (west/qemu/idf) have nothing.

## Approach — detect-only dep-info probe

Move the guard to the RESOLVER using a DETECT-ONLY probe: read the toolchain's
own recorded dependency graph and mtime-compare against the built binary.

- **Reads, never invokes.** The maintainer's constraint is "no building final
  binaries at test time" (slow + triggers unnecessary rebuild). Invoking
  `cargo build`/`cmake --build` as a probe is a no-op when fresh but rebuilds
  the final binary when STALE — the forbidden case, and there is no stable
  `cargo build --dry-run`. Reading cargo's `<binary>.d` (make-style dep list,
  ~186 files for the listener, incl. shared crates `nros-core`/`nros-macros`/
  generated msg crates) + `stat()` cannot rebuild and covers deps a
  source-dir content hash would miss.
- **C/C++** — same on the per-object `.d` files gcc/clang emit under `-MD`
  (ninja already generates them); read + stat, do NOT invoke ninja (sidesteps
  the Corrosion always-run step that broke `ninja -n` in the preflight).

Accepted caveat: mtime flags stale after a `git checkout` that resets source
mtimes even when content is unchanged — errs toward a correct rebuild, never
toward silently-stale (identical to cargo's own fingerprint behavior).

## Waves

### W1 — Rust dep-info probe + native single-node rollout
- [x] W1.a `require_prebuilt_binary_fresh(&Path)` in
  `nros-tests/src/fixtures/binaries/mod.rs`: existence check, then parse the
  sibling `<binary>.d`, `stat()` each listed source, fail
  `BuildFailed("… is stale: run just build-test-fixtures")` if any is newer
  than the binary. Missing `.d` → treat as existence-only (no regression) with
  a one-line eprintln note, so non-cargo callers that reuse
  `require_prebuilt_binary` are unaffected until migrated.
- [x] W1.b Route the native rust funnels `build_example` (mod.rs:438) and
  `build_example_rmw` (mod.rs:534) through the fresh check. This covers the
  #146 family (talker/listener/service/action/interop) + RTIC + feature
  variants (safety/tls/zero-copy/header) in one edit each.
- [x] W1.c Guard the `.d` parse: dep paths are absolute in these builds; handle
  the make-escaping (`\` line continuations, escaped spaces) and a binary with
  no `.d` (older cargo / non-cargo) gracefully.
- [x] W1.d Acceptance: with a fresh tree the migrated tests pass; `touch`-ing an
  example source then re-running WITHOUT rebuild fails "… is stale"; rebuilding
  clears it. Re-run `rmw_interop` (the #146 suite) green on fresh fixtures.

### W2 — C/C++ + bins/ probe
- [ ] W2.a Object-`.d` reader for cmake cells: enumerate the `.d` files under
  the cell's `build-<rmw>/…CMakeFiles/…` tree, union their dep lists, mtime-
  compare against the resolved binary. Fall back to existence-only when no `.d`
  is present (unconfigured cell).
- [ ] W2.b Route `build_example_cmake_rmw` (mod.rs:563) + the `bins/`
  resolvers (`build_test_fixture`, mod.rs:1762) through it.
- [ ] W2.c Acceptance: touch a C example `.c`/`.h` → stale; touch a linked
  `nros-c` source → stale (dep coverage); rebuild clears.

### W3 — Zephyr-workspace entries
- [ ] W3.a The 9 `build_zephyr_workspace_*` (mod.rs:2238–2331) use bare
  `require_prebuilt_binary` where the native/cmake workspace family is guarded
  — the clearest oversight. Give them the fresh check: the west build emits
  `zephyr/zephyr.exe` + `.d` files, so the same dep-info probe applies (union
  the rust staticlib `.d` + the C object `.d`s), OR switch to
  `require_prebuilt_workspace_binary` and have `zephyr-fixture-leaves.sh` write
  the workspace `.inputsig`. Pick whichever the west artifact layout makes
  clean.
- [ ] W3.b Acceptance: touch a `ws-*/src/zephyr_entry` source → the matching
  `*_zephyr_entry_e2e` fails stale without a rebuild; rebuild clears. (This
  would have caught the repeated stale-image reruns in phase-276.)

### W4 — Docs + preflight reconciliation
- [ ] W4.a Update the stale justfile:1029 comment (the `.nros-fixture.inputsig`
  mechanism it names no longer exists) and note the resolver now guards plain
  fixtures independently of `_check-fixtures-stale`.
- [ ] W4.b `docs/development/` or AGENTS.md one-liner: bare `cargo nextest` now
  fails loud on stale fixtures; the fix is `just build-test-fixtures`, not
  `NROS_SKIP_FIXTURE_CHECK`.
- [ ] W4.c Resolve #147; note non-cargo/embedded (qemu/west/idf) left on
  existence + `.compile-ok` as accepted (they rebuild wholesale per lane).

## Non-goals

- Guarding qemu/west/idf/compile-check non-cargo fixtures beyond what W3 covers
  (they rebuild wholesale per lane; existence + `.compile-ok` is tolerable).
- Replacing the `just test-all` preflight — it stays (it self-heals, which is
  convenient in CI); this phase adds the resolver guard for the direct-nextest
  path the preflight can't reach.
- Signing shared-crate deps for the content-signature path — superseded; the
  `.d` probe covers deps natively.

## Acceptance (phase)

- `just format` + `just ci` green.
- A `touch` on any migrated example's source, with no rebuild, makes its e2e
  fail "… is stale" under a bare `cargo nextest run` (not just `just test-all`).
- The #146 `rmw_interop` suite stays green on fresh fixtures.
- #147 resolved.

## Sequencing

W1 (rust, the #146 family) is independent and lands first — highest value,
smallest surface. W2 (C/C++) and W3 (zephyr-workspace) are independent of each
other; W3 is a small self-contained win directly relevant to the phase-276
zephyr lanes. W4 closes docs + the issue.
