# Phase 165 — Unified Setup UX + Shell Activation File

**Goal.** Resolve four user-facing setup / UX issues that surfaced
from the multi-persona book audits and from operating the project:

1. **Setup entry is split across `just setup` and `tools/setup.sh`** —
   pick one canonical command surface.
2. **Per-platform setup (`just <plat> setup`) coverage is unverified** —
   confirm each per-platform recipe correctly fetches submodules,
   builds the requisite SDK, AND leaves the examples / tests / build
   in a runnable state.
3. **SDK tiers (`tier=minimal|default|extended`)** add an axis users
   may not need; evaluate whether tiers earn their keep or should be
   collapsed.
4. **Shipped binaries (`nros`, `zenohd`, `nros-codegen`, `MicroXRCEAgent`,
   etc.) are scattered across `build/` and `cargo install` paths** with
   no convenient PATH activation. Ship a `setup.bash` / `setup.fish`
   that users can `source` to get every nano-ros binary on PATH and
   any required env vars set.

**Status.** Not Started.

**Priority.** P2 — onboarding quality + day-to-day ergonomics.

**Depends on.** Nothing — orthogonal to runtime / examples.

---

## Findings

### 1. Two setup entries — which is canonical?

| Surface | Audience | Scope |
|---|---|---|
| `just setup [tier=<tier>]` (no arg) | Contributors hacking on nano-ros | Walks every module's `setup` recipe |
| `just setup <target>` | Users picking one `(platform, rmw)` pair | Shims to `tools/setup.sh --target=<plat>-<rmw>` |
| `tools/setup.sh --target=<plat>-<rmw>` | Direct script invocation | Reads `config/submodule-deps.toml` and fetches the union of paths |
| `just <plat> setup` | Users wanting one RTOS's full SDK | Module-scoped: fetches the RTOS submodule(s) + builds platform deps |

The book currently exposes all four shapes, which is confusing.
**Recommendation:** make `just setup` the single user-facing entry,
with two argument forms:

- `just setup` → contributor flow (every module).
- `just setup <plat>` (e.g. `just setup freertos`) → user flow
  (one RTOS's full SDK), exactly equivalent to `just freertos setup`.
- `just setup <plat>-<rmw>` (legacy) → keep as compatibility shim.

`tools/setup.sh` stays as the underlying script (callable for power
users, exposed in advanced docs only).

### 2. Per-platform setup coverage

Run `just <plat> setup` for each supported platform on a fresh clone,
confirm the resulting working tree:

- has the right submodules at the right SHAs,
- can build that platform's example tree (`just <plat> build-fixtures`),
- can run that platform's tests (`just <plat> test-all`),
- documents any required env vars (e.g. `IDF_PATH`, `XILINX_VITIS`).

Per-platform setup verification matrix:

| `just <plat> setup` | Submodules fetched? | Examples build? | Tests run? | Env vars documented? |
|---|---|---|---|---|
| `qemu` | TBD | TBD | TBD | TBD |
| `freertos` | TBD | TBD | TBD | TBD |
| `nuttx` | TBD | TBD | TBD | TBD |
| `zephyr` | TBD | TBD | TBD | TBD |
| `threadx_linux` | TBD | TBD | TBD | TBD |
| `threadx_riscv64` | TBD | TBD | TBD | TBD |
| `esp32` | TBD | TBD | TBD | TBD |
| `esp_idf` | TBD | TBD | TBD | TBD |
| `xrce` | TBD | TBD | TBD | TBD |
| `rmw_zenoh` | TBD | TBD | TBD | TBD |
| `cyclonedds` | TBD | TBD | TBD | TBD |
| `verification` | TBD | TBD | TBD | TBD |

Each row needs a green tick before we can claim "`just <plat> setup`
is sufficient on its own."

### 3. Are SDK tiers necessary?

The current tier system (`minimal` ⊂ `default` ⊂ `extended`) was
introduced to keep `just setup` from forcing a multi-GB install on
Rust-only contributors. But:

- **Most users only need one platform** — they should `just freertos
  setup`, not `just setup tier=default`.
- **Contributors need every module** — `just setup` (no arg) already
  does that.
- **The `tier=` mid-ground is rarely the right choice** — it
  conflates "what I'm working on" with "what's safe to install
  unattended."

**Recommendation:** collapse tiers down to `minimal` (workspace +
verification + zenohd — pure Rust contributors) and `everything`
(no-arg `just setup`). Drop the `default` / `extended` distinction.
Or drop tiers entirely and document `just <plat> setup` as the
correct per-need entry.

Track the decision: keep, simplify, or drop.

### 4. Shell activation file (PATH + env)

nano-ros currently ships binaries at:

- `build/zenohd/zenohd` (router, after `just zenohd setup`)
- `build/qemu/bin/qemu-system-arm` (patched QEMU, after
  `just qemu setup-qemu`)
- `packages/codegen/packages/target/release/nros-codegen` (codegen
  tool, built on demand by examples)
- `packages/codegen/packages/target/release/nros` (CLI, after
  `cargo install --path packages/codegen/packages/nros-cli`)
- `third-party/xrce/agent/build/MicroXRCEAgent` (XRCE Agent, after
  `just xrce setup`)

None of these end up on PATH automatically. Users either invoke
with absolute paths or remember `cargo install` arguments.

**Proposal:** ship a `setup.bash` (and `setup.fish`, `setup.zsh`)
at the repo root that:

- Adds the shipped binaries' directories to `PATH`
- Sets canonical env vars (`NROS_ZENOHD`, `NROS_CODEGEN`,
  `NROS_QEMU_SYSTEM_ARM`, `NROS_XRCE_AGENT`)
- Optionally exports `RMW_IMPLEMENTATION=rmw_zenoh_cpp` for the
  current shell when running stock ROS 2 alongside.

Users `source ./setup.bash` once per shell session and get every
nano-ros binary on PATH — same ergonomics as ROS 2's
`source /opt/ros/humble/setup.bash`.

---

## Work items

### 165.A — Setup entry unification

- [ ] **165.A.1** Decide canonical surface: `just setup` only.
      `tools/setup.sh` stays as the underlying script (power-user
      / debugging surface, not in user docs).
- [ ] **165.A.2** Update book (`installation.md`,
      `setup-compared-to-ros2.md`, every starter page) to use
      `just setup [<plat>]` only. Demote `tools/setup.sh` to a
      one-line aside.

### 165.B — Per-platform setup verification

- [ ] **165.B.1** Run `just <plat> setup` on a fresh clone for each
      module in the matrix above. Record what worked, what failed,
      and what env vars / system packages it needed.
- [ ] **165.B.2** Land a `just <plat> doctor` recipe per module
      that diagnostics-only validates the setup output. Tie into
      the existing `just doctor` orchestrator.
- [ ] **165.B.3** Fix gaps (missing submodules, missing build
      steps, undocumented env vars) one platform at a time.

### 165.B-test — Audit-reader verification (use book-walker agents)

Each per-platform setup verification (165.B.1) MUST also pass an
audit-reader pass: an agent acts as a first-time user, opens the
matching book page, executes every command literally, and reports
friction. Each agent reports against the 5-section template
(Project layout / Configure / Build / Run / GitHub source) plus
the readiness signal.

The 2026-05-19 first-pass already surfaced concrete bugs (zenohd
recipe missing, FreeRTOS binary-name mismatch, per-language port
divergence, wrong error codes in C troubleshooting). Treat every
new per-platform setup as needing the same audit pass before
declaring it "done."

- [ ] **165.B-test.1** Audit-reader agent: Linux Rust starter
      (`book/src/getting-started/first-node-rust.md`). Acceptance:
      blind reader executes every Build / Run command and reaches
      `Published: 1` without bouncing.
- [ ] **165.B-test.2** Audit-reader agent: Linux C starter
      (`book/src/getting-started/first-node-c.md`).
- [ ] **165.B-test.3** Audit-reader agent: Linux C++ starter
      (`book/src/getting-started/first-node-cpp.md`).
- [ ] **165.B-test.4** Audit-reader agent: FreeRTOS QEMU starter
      (`book/src/getting-started/freertos.md`) — covers Rust + C +
      C++ variants, runs `just freertos build-fixtures`. Blocked
      on [Phase 166](./phase-166-freertos-board-crate-dup-symbols.md)
      until the duplicate-symbol regression is closed.
- [ ] **165.B-test.5** Audit-reader agent: Zephyr west-module
      starter (`book/src/getting-started/integration-zephyr.md`).
- [ ] **165.B-test.6** Audit-reader agent: NuttX apps/external
      starter (`book/src/getting-started/integration-nuttx.md`).
- [ ] **165.B-test.7** Audit-reader agent: ThreadX-Linux + RV64
      starter (`book/src/getting-started/threadx.md`).
- [ ] **165.B-test.8** Audit-reader agent: ESP32 esp-hal starter
      (`book/src/getting-started/esp32.md`) and ESP-IDF starter
      (`book/src/getting-started/integration-esp-idf.md`). Hardware-
      board steps stay deferred (need physical board); the build-
      path steps must pass.
- [ ] **165.B-test.9** Audit-reader agent: Bare-metal Cortex-M3
      starter (`book/src/getting-started/bare-metal.md`).
- [ ] **165.B-test.10** Audit-reader agent: PX4 external-module
      starter (`book/src/getting-started/px4.md`). Build-path only;
      no hardware Pixhawk required.

For each audit-reader pass, the agent's report should list:
- Per-command exit code + last 10 lines of output.
- Any documented prereq the agent missed because the book didn't
  flag it (e.g. zenohd-must-be-running, terminal-blocks).
- Any discrepancy between the book's "Project layout" tree and
  the actual on-disk directory.
- Any binary-name / port / error-code mismatch between docs and
  reality.

### 165.C — Tier system audit

- [ ] **165.C.1** Decide: keep three tiers, collapse to two
      (`minimal` + `everything`), or drop entirely.
- [ ] **165.C.2** If dropped: update the justfile orchestrator to
      take a platform list directly. If kept: document why each
      tier earns its slot.
- [ ] **165.C.3** Update book per the decision.

### 165.D — Shell activation file

- [ ] **165.D.1** Ship `setup.bash` at the repo root that:
      - Computes `NROS_ROOT` (the script's dirname).
      - Adds `${NROS_ROOT}/build/zenohd`, `${NROS_ROOT}/build/qemu/bin`,
        `${NROS_ROOT}/packages/codegen/packages/target/release`,
        and `${NROS_ROOT}/third-party/xrce/agent/build` to PATH
        (only the dirs that exist — silently skip the rest).
      - Exports `NROS_ZENOHD`, `NROS_CODEGEN`, `NROS_QEMU_SYSTEM_ARM`,
        `NROS_XRCE_AGENT` to the resolved absolute paths.
      - Prints a one-line confirmation banner with `NROS_ROOT` + the
        binaries it found.
- [ ] **165.D.2** Ship `setup.fish` (mirror).
- [ ] **165.D.3** Ship `setup.zsh` (or confirm `setup.bash`
      works under zsh).
- [ ] **165.D.4** `just setup` (the orchestrator) prints
      "✅ Source ./setup.bash to get every nano-ros binary on PATH"
      at the end so users discover the activation step.
- [ ] **165.D.5** Update book's installation + first-node pages to
      lead with `source ./setup.bash` after `just setup`.

---

## Files

### New

- `setup.bash` (repo root)
- `setup.fish` (repo root)
- `setup.zsh` (repo root) — or alias to bash
- `docs/reference/setup-activation.md` — short reference for the
  env vars and what each binary does.

### Modified

- `justfile` — `just setup` print activation hint; tier
  orchestrator may be simplified per 165.C decision.
- `book/src/getting-started/installation.md` — lead with
  `source ./setup.bash` after `just setup`.
- `book/src/getting-started/first-node-*.md` — drop hardcoded
  `./build/zenohd/zenohd` paths in favour of `zenohd` (PATH).
- Every starter "Run" section — same simplification.
- `tools/setup.sh` — demoted from user-facing to advanced /
  contributor-tooling.

---

## Acceptance criteria

- [ ] A user clones the repo, runs `just setup freertos`, sources
      `./setup.bash`, and has `zenohd`, `qemu-system-arm`, `nros`,
      `nros-codegen` all reachable as bare commands.
- [ ] Book's Linux starter does NOT mention `./build/zenohd/zenohd`
      explicit path; uses `zenohd` after `source ./setup.bash`.
- [ ] `just setup` prints a one-line activation hint at the end.
- [ ] Each `just <plat> setup` row in the verification matrix is
      green (submodules + examples + tests + env-var docs OK).
- [ ] Tier decision landed (keep / simplify / drop) and book
      reflects it.

---

## Notes

- The activation pattern mirrors ROS 2's `source /opt/ros/<distro>/setup.bash`.
  Users already know this idiom; lifting it into nano-ros costs nothing
  and removes the "where did `zenohd` go?" question.
- For consumers who install nano-ros into a downstream workspace
  (Pattern A from `installation.md`), the activation file lives in
  `<workspace>/src/nano-ros/setup.bash` — same shape.
- The activation file MUST NOT depend on any nano-ros build state.
  If `build/zenohd/zenohd` doesn't exist yet, skip its dir silently.
  Re-sourcing after the build picks it up.
