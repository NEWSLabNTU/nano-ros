# Phase 220 — CLI surface shrink + bootstrap UX + book reconcile

**Goal.** Reduce the `nros` CLI surface to the verbs that do *real
work* (provision + codegen + topology resolve + introspection), drop
the verbs that thinly wrap platform toolchains, and fix the
chicken-egg + stale prereq blocks across the book that the Phase 218
monorepo merge surfaced.

**Status.** PROPOSED 2026-06-04.

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

The Phase 212 design (`docs/design/multi-node-workspace-layout.md`
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

### 220.A — Book prereq sweep

- [ ] **220.A.1** Replace the `source ./activate.sh && just
      setup-cli` prereq block across the five affected chapters with a
      three-path block (A = bootstrap, B = cargo one-liner, C =
      prebuilt fetch on tagged checkouts). Lead with Path A.
- [ ] **220.A.2** Add a top-of-`installation.md` callout that
      `activate.sh` is the *after-install* step (every subsequent
      shell), not a prereq.
- [ ] **220.A.3** `workspace-from-app-node.md` command-map row for
      `ros2 launch` — strike the `nros launch` cell or mark
      `(deferred)` per 220.D; lead with `cargo run -p <entry_pkg>`.

**Files.** `book/src/getting-started/{installation,first-node-rust,
first-node-c,first-node-cpp,workspace-from-app-node,workspace-bringup,
workspace-entry-pkg,workspace-node-pkgs}.md`.

### 220.B — Deprecate `nros build` / `run` / `deploy` / `monitor`

- [ ] **220.B.1** Mark each verb's `--help` line with `(deprecated —
      see <platform-tool>; will be removed in nros 0.5.0)`.
- [ ] **220.B.2** Bump the verb impls to emit a one-line stderr
      warning on every invocation, then delegate to the underlying
      tool as today.
- [ ] **220.B.3** `nros doctor` flags use of any deprecated verb in
      `Cargo.toml` `[package.metadata.nros.deploy.*]` build / run /
      flash override fields, suggesting the platform tool.

**Files.** `packages/cli/nros-cli/src/cmd/{build,run,deploy,monitor}.rs`
(or wherever the verbs are dispatched), `packages/cli/nros-cli-core/`.

### 220.C — Delete deprecated verbs in 0.5.0

- [ ] **220.C.1** Remove the four verb subcommands (`build`, `run`,
      `deploy`, `monitor`) from the CLI's `clap` derive tree.
- [ ] **220.C.2** Drop the corresponding test fixtures.
- [ ] **220.C.3** Doc sweep — every reference in book / phase docs /
      examples bumps to the platform tool. Match the Phase 218 doc
      sweep style: retain historical mentions in archived phase docs.
- [ ] **220.C.4** Bundle bump to `0.5.0` via `just release-bump`.
      Coincides with the deletion to make the SemVer break visible.

**Files.** CLI clap tree, `packages/cli/nros-cli/tests/`, book, root
`Cargo.toml` + `packages/cli/Cargo.toml`.

### 220.D — `nros launch` decision

- [ ] **220.D.1** Land the launch decision (delete vs. redefine —
      see §2.3). Drives whether 220.C deletes a fifth verb.

### 220.E — Bootstrap script polish

- [ ] **220.E.1** `scripts/bootstrap.sh base` — confirm the cold-cache
      path (`bash` only, no `cargo`, no `just`) installs rustup, just,
      builds the CLI, exports the right PATH onto the user's shell rc
      (with prompt + dry-run flag).
- [ ] **220.E.2** `scripts/bootstrap.sh nros` — verify Path C tag-fetch
      path against an actual `nros-v*` release once Phase 218.G ships
      its first artifact.
- [ ] **220.E.3** `scripts/bootstrap.sh doctor` — pre-Phase-220
      verb-deprecation, the lane that surfaces stale verb invocations
      in user `.bashrc` / `.zshrc` rc files (`alias nros-build=...`
      etc.).

**Files.** `scripts/bootstrap.sh`.

### 220.F — `activate.sh` UX polish

- [ ] **220.F.1** `activate.sh` first-run greeting — when sourced
      against a checkout that has NO `packages/cli/target/release/nros`
      yet, print one-line `[hint] CLI not built yet — run
      ./scripts/bootstrap.sh base or cargo build --release …` instead
      of silently leaving `nros` off PATH.
- [ ] **220.F.2** Same for `activate.fish`.

**Files.** `activate.sh`, `activate.fish`.

---

## 4. Acceptance

- [ ] A fresh-machine new user (bash + curl only — no rustup, no just,
      no cargo, no Rust at all) reaches a working `nros new` in **one
      command** (`./scripts/bootstrap.sh base`), no chicken-egg, no
      "install just first" detour.
- [ ] `nros --help` lists only verbs from §2.1 by Phase 220 close;
      the §2.2 verbs are gone.
- [ ] `grep -rE '\b(nros build|nros run|nros deploy|nros monitor)\b'
      book/ examples/ docs/development/` returns zero matches (modulo
      historical phase doc references).
- [ ] Bundle version bumped to `0.5.0` (per `just release-bump`) at
      the 220.C close commit — the SemVer break is visible.

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
