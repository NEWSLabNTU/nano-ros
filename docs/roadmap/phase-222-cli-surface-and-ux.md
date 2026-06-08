# Phase 222 — CLI surface shrink + bootstrap UX + book reconcile

**Goal.** Reduce the `nros` CLI surface to the verbs that do *real
work* (provision + codegen + topology resolve + introspection), drop
the verbs that thinly wrap platform toolchains, and fix the
chicken-egg + stale prereq blocks across the book that the Phase 218
monorepo merge surfaced.

**Status.** IMPLEMENTED 2026-06-06, except Path C prebuilt verification is
blocked until a `nros-v*` release tag/artifact exists.

**Priority.** P2 — no capability is blocked, but every new user hits
the broken prereq blocks on their first 10 minutes; CLI verbs that
don't pull their weight cost trust without adding power.

**Depends on.** Phase 218 (monorepo merge — landed). Touches every
book chapter under `book/src/getting-started/` and the `nros-cli`
subcommand surface.

---

## 1. Three concerns

### 1.1 Bootstrap chicken-egg

Phase 218 docs lead with:

```sh
source ./activate.sh
just setup-cli
```

…but `just` is not on a fresh machine. The user must install `just`
*before* they can run `just setup-cli`. The honest user flow on a bare
machine is one of:

| path | command | needs |
|---|---|---|
| A (bare metal) | `./scripts/bootstrap.sh base` | bash + curl |
| B (already has cargo) | `cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros` | rustup + cargo |
| C (tagged release, no Rust) | `./scripts/install-nros-prebuilt.sh` | bash + curl + a `nros-v*` tag reachable |

`scripts/bootstrap.sh base` already handles Path A — it installs
rustup + just + the CLI in one shot. The fix is a doc sweep that
leads users to it instead of pointing at `just setup-cli`.

### 1.2 CLI surface shrink

The Phase 212 design (`docs/design/0024-multi-node-workspace-layout.md`
§2) constrained `nros` to provisioner + codegen + metadata + deploy,
explicitly rejecting `nros build` / `nros test` / `nros flash`. Today
the CLI still ships `nros build`, `nros run`, `nros launch`,
`nros deploy`, `nros monitor` — each a thin wrapper around a
platform-specific tool that the user already invokes directly. Result:
two ways to do the same thing, neither obviously canonical.

Per-platform realities the wrappers fight:

| platform | build | run/flash |
|---|---|---|
| native | `cargo run` / `cmake --build` | direct execution |
| Zephyr | `west build` | `west <runner> run` (`fvp`, `qemu`, `flash`) |
| FreeRTOS-QEMU | `just qemu build` | `qemu-system-arm …` |
| NuttX-QEMU | `make` | `qemu-system-arm` / `nsh flash` |
| ThreadX | `cmake --build` | host exec / qemu |
| bare-metal stm32f4 | `cargo build --target` | `probe-rs run` |
| ESP-IDF | `idf.py build` | `idf.py flash monitor` |
| PX4 | `make px4_sitl …` | `make px4_sitl gz_*` |

Every cell is already a one-liner. A `nros build` / `nros run` /
`nros deploy` wrapper adds dispatch indirection without leverage.

### 1.3 Book stale prereq + `nros launch` ambiguity

Five book chapters lead with the (broken) `source ./activate.sh && just
setup-cli` pattern: `installation.md`, `first-node-{rust,c,cpp}.md`,
several `workspace-*.md` pages. Plus the `workspace-from-app-node.md`
"ROS 2 ↔ nano-ros command map" advertises `nros launch <bringup>` as
the rough analogue of `ros2 launch`, but the in-tree CLI's
one-process-per-`[[component]]` model fights the canonical Entry pkg
shape (which fuses Node pkg libs into a single binary). Either the
launch verb's semantics change to match the fused-binary case, or it
goes away.

---

## 2. Proposal — verbs by classification

### 2.1 Keep (real work)

| verb | role |
|---|---|
| `nros new` | scaffold project / pkg from templates (Node, Bringup, Entry) |
| `nros setup` | index-driven SDK / toolchain / source provisioning (the entire reason `nros` exists at the user-facing level) |
| `nros generate-rust` / `nros generate` | rosidl-style msg codegen |
| `nros codegen-system` | system-level codegen (`system_main.c`, the Zephyr H.1 shim, the multi-node boot stub) |
| `nros plan` | resolve a topology against a Bringup pkg + workspace pkg-index → plan JSON |
| `nros check` | static lint of a Bringup pkg / system.toml / package.xml |
| `nros explain` | inspect a resolved plan |
| `nros doctor` | readiness diagnosis (board-scoped via `--board`) |
| `nros board` | board / SDK introspection |
| `nros ws sync` | rewrite `[patch.crates-io]` for a workspace |

### 2.2 Drop (thin wrappers over platform tools)

| verb | replacement |
|---|---|
| `nros build` | `cargo build` / `cmake --build` / `west build` / `idf.py build` (already documented in `workspace-from-app-node.md`'s command map) |
| `nros run` | `cargo run -p <entry_pkg>` / `west <runner> run` / `probe-rs run` / `idf.py monitor` |
| `nros deploy` | platform tool + Bringup pkg's `[deploy.<target>]` — flash/run combo lives in the platform's native verb |
| `nros monitor` | `probe-rs attach` / `picocom` / `idf.py monitor` — there is no nano-ros-specific telemetry to multiplex |

### 2.3 Decide (deferred per discussion 2026-06-04)

| verb | options |
|---|---|
| `nros launch` | (A) **delete** — composed-binary Entry pkg IS the launch product; multi-process is `tmux + N terminals` or a shell script. (B) **redefine** — keep the verb but make it explicitly the multi-Entry orchestrator (one process per `[deploy.<target>]` block, NOT one per `[[component]]`). Today's behaviour matches neither cleanly. |

Recommendation: defer the launch decision until a real multi-Entry
fleet (e.g. some Node pkgs on host, others on RTOS) drives the
requirement. Until then, drop the in-tree implementation and document
`cargo run -p <entry_pkg>` as the canonical single-deploy launch.

---

## 3. Work items

### 222.A — Book prereq sweep

- [x] **222.A.1** Replace the `source ./activate.sh && just
      setup-cli` prereq block across the five affected chapters with a
      three-path block (A = bootstrap, B = cargo one-liner, C =
      prebuilt fetch on tagged checkouts). Lead with Path A. _(2026-06-04)_
- [x] **222.A.2** Add a top-of-`installation.md` callout that
      `activate.sh` is the *after-install* step (every subsequent
      shell), not a prereq. _(2026-06-04)_
- [x] **222.A.3** `workspace-from-app-node.md` command-map row for
      `ros2 launch` — strike the `nros launch` cell or mark
      `(deferred)` per 222.D; lead with `cargo run -p <entry_pkg>`. _(2026-06-04)_

**Files.** `book/src/getting-started/{installation,first-node-rust,
first-node-c,first-node-cpp,workspace-from-app-node,workspace-bringup,
workspace-entry-pkg,workspace-node-pkgs}.md`.

### 222.B — Deprecate `nros build` / `run` / `deploy` / `monitor`

- [x] **222.B.1** Mark each verb's `--help` line with `(deprecated —
      see <platform-tool>; will be removed in nros 0.5.0)`. _(2026-06-04)_
- [x] **222.B.2** Bump the verb impls to emit a one-line stderr
      warning on every invocation, then delegate to the underlying
      tool as today. `NROS_SUPPRESS_DEPRECATION=1` opt-out for CI lanes
      that still need to drive the wrapper. _(2026-06-04)_
- [x] **222.B.3** `nros doctor` flags use of any deprecated verb in
      workspace-root `nros.toml` `[deploy.<name>].build` / `.package`
      shell-step arrays (WARN, gated migration; will fail in 0.5.0).
      _(2026-06-04)_
- [x] **222.B.4** Integration tests cover the `--help` deprecation
      suffix, the stderr warning on invocation, and the
      `NROS_SUPPRESS_DEPRECATION=1` opt-out across all four verbs
      (`packages/cli/nros-cli/tests/deprecated_verbs.rs`).
      _(2026-06-04)_

**Files.** `packages/cli/nros-cli/src/cmd/{build,run,deploy,monitor}.rs`
(or wherever the verbs are dispatched), `packages/cli/nros-cli-core/`.

### 222.C — Delete deprecated verbs in 0.5.0

- [x] **222.C.1** Remove the five verb subcommands (`build`, `run`,
      `deploy`, `monitor`, `launch`) from the CLI's `clap` derive
      tree. Phase 222.D added `launch` to the deprecation set.
      _(2026-06-06)_
- [x] **222.C.2** Drop the corresponding test fixtures. _(2026-06-06)_
- [x] **222.C.3** Doc sweep — every reference in book / phase docs /
      examples bumps to the platform tool. Match the Phase 218 doc
      sweep style: retain historical mentions in archived phase docs.
      _(2026-06-06)_
- [x] **222.C.4** Bundle bump to `0.5.0` via `just release-bump`.
      Coincides with the deletion to make the SemVer break visible.
      _(2026-06-06)_

**Files.** CLI clap tree, `packages/cli/nros-cli/tests/`, book, root
`Cargo.toml` + `packages/cli/Cargo.toml`.

### 222.D — `nros launch` decision — landed 2026-06-04

**Decision: Option A — delete.** Phase 212.N locked the Entry pkg
shape as a *fused* binary (Node pkg libs linked into one Entry; the
ROS 2 composable-node parallel). The single Entry binary IS the
launch product. `nros launch`'s one-process-per-`[[component]]`
model fights that shape. ROS 2 migration users get pointed at
`cargo run -p <entry_pkg>` instead — same composability, same
process model. Multi-Entry / mixed-host orchestration (Option D —
codegen a per-Bringup `launch.sh`) waits for real demand in a
follow-on phase; the launch.xml parser stays as a compile-time
input to `nros::main!()` regardless.

- [x] **222.D.1** Decision recorded (Option A, defer Option D).
      _(2026-06-04)_
- [x] **222.D.2** `nros launch` joins the 222.B deprecation set:
      `--help` carries the `(deprecated)` suffix; the verb body emits
      the `NROS_SUPPRESS_DEPRECATION` warning on stderr; the doctor
      `match_deprecated_verb` map recognises it; the integration test
      matrix covers it. _(2026-06-04)_
- [x] **222.D.3** `book/src/getting-started/workspace-from-app-node.md`
      command-map row swept: `ros2 launch ↔ cargo run -p <entry_pkg>`,
      with a note that the old `nros launch` verb is deprecated and
      removed in nros 0.5.0. _(2026-06-04)_
- [x] **222.D.4** 222.C scope updated — deletion in nros 0.5.0 covers
      five verbs (build / run / deploy / monitor / launch), not four.
      _(2026-06-04)_

### 222.E — Bootstrap script polish

- [x] **222.E.1** `scripts/bootstrap.sh base` — confirm the cold-cache
      path (`bash` only, no `cargo`, no `just`) installs rustup, just,
      builds the CLI, exports the right PATH onto the user's shell rc
      (with prompt + dry-run flag). _(2026-06-04)_
- [ ] **222.E.2** `scripts/bootstrap.sh nros` — verify Path C tag-fetch
      path against an actual `nros-v*` release once Phase 218.G ships
      its first artifact.
      **Blocked 2026-06-06:** `git tag --list 'nros-v*'` and
      `git ls-remote --tags origin 'refs/tags/nros-v*'` both returned
      no tags, so there is no actual release artifact to verify yet.
- [x] **222.E.3** `scripts/bootstrap.sh shell-doctor` — pre-Phase-222.C
      verb-deprecation lane that surfaces stale verb invocations in
      user `.bashrc` / `.zshrc` / `config.fish` rc files
      (`alias nros-build=...`, `alias x='nros run …'` etc.) +
      checks PATH, version lockstep, and the activate-source line.
      Distinct surface from `just doctor` (build-env). _(2026-06-04)_

      Note: the rc-edit helper for §222.E.1 lives inside
      `install_base()` (`offer_shell_rc_update`); it is idempotent
      (skips if the activate path already appears in rc), prints the
      snippet to stderr regardless, and honours `--no-prompt` (CI
      mode) / `--dry-run`. This was 222.E.2-as-originally-scoped
      (shell-rc append); the 222.E.2 line above is the unrelated
      tagged-release tag-fetch verification, still blocked on the
      first 218.G artifact.

**Files.** `scripts/bootstrap.sh`.

### 222.F — `activate.sh` UX polish

- [x] **222.F.1** `activate.sh` first-run greeting — when sourced
      against a checkout that has NO `packages/cli/target/release/nros`
      yet AND no `nros` reachable on any other PATH entry, print a
      four-line hint pointing at `./scripts/bootstrap.sh base`,
      the cargo one-liner, or `./scripts/install-nros-prebuilt.sh`
      (instead of silently leaving `nros` off PATH). Suppress with
      `NROS_QUIET_ACTIVATE=1` for CI lanes that build the CLI as a
      separate step. _(2026-06-04)_
- [x] **222.F.2** Same for `activate.fish` (mirror; same env opt-out).
      _(2026-06-04)_

**Files.** `activate.sh`, `activate.fish`.

---

## 4. Acceptance

- [ ] A fresh-machine new user (bash + curl only — no rustup, no just,
      no cargo, no Rust at all) reaches a working `nros new` in **one
      command** (`./scripts/bootstrap.sh base`), no chicken-egg, no
      "install just first" detour.
- [x] `nros --help` lists only verbs from §2.1 by Phase 222 close;
      the §2.2 verbs are gone.
- [x] `grep -rE '\b(nros build|nros run|nros deploy|nros monitor)\b'
      book/ examples/ docs/development/` returns zero matches (modulo
      historical phase doc references).
- [x] Bundle version bumped to `0.5.0` (per `just release-bump`) at
      the 222.C close commit — the SemVer break is visible.

---

## 5. Notes

- The verb-shrink is a SemVer-breaking change inside `nros`; coincides
  with the `0.5.0` bundle bump per Phase 218.J's documented rule that
  ABI-breaking changes bump minor.
- The book sweep depends on `activate.sh` + `scripts/bootstrap.sh`
  already shipping (they did, in Phase 218). No new infrastructure
  required.
- This phase intentionally does NOT add new CLI verbs. It is a
  shrink, not a redesign. New verbs (e.g. a redefined `nros launch`)
  land in their own phase after the design discussion.
- ROS-2-from-the-corner-of-your-eye expectations: users coming from
  `ros2 launch` will look for `nros launch`. The doc sweep needs to
  point them at `cargo run -p <entry_pkg>` AND explain the composed-
  binary shape — "one Entry pkg = one binary = one process; you do
  not orchestrate processes, you orchestrate Node pkgs at link time."
