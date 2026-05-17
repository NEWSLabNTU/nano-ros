# Phase 143 — Unified `qemu-system-arm` Across All Test Call Sites

**Goal.** Make every `qemu-system-arm` invocation in the project go
through the patched binary at `build/qemu/bin/qemu-system-arm`
(produced by `just qemu setup-qemu` from the in-tree submodule +
patch set). System `qemu-system-arm` becomes the fallback path used
only when the patched build is absent. Closes the "system qemu 6.2
too old" WARN class across NuttX DDS multi-instance, LAN9118
receive-flush, and any future patched-only behaviour.

**Status.** Not started.

**Priority.** P2 — deterministic test results, not a correctness
blocker. The patched binary already exists; ~5 call sites just
ignore it.

**Depends on.** None. The patched build is already wired via
`just qemu setup-qemu` (pulled by `just setup`).

**Related.** Phase 127.D (introduced the patched build + `QEMU_SYSTEM_ARM`
env-var opt-in), Phase 142 (`just setup` orchestrator tiering).

---

## Overview

`third-party/qemu/qemu` (submodule, pinned) + `third-party/qemu/patches/`
(currently one LAN9118 receive-flush patch) build to
`build/qemu/bin/qemu-system-arm`. `just qemu setup-qemu` handles
the build, and it's pulled by `just setup` orchestrator's `qemu`
module.

Today only **one** Rust test call site honours
`QEMU_SYSTEM_ARM`: `packages/testing/nros-tests/src/qemu.rs:18-27`'s
`qemu_system_arm_command()` helper. Every other invocation
hard-codes `Command::new("qemu-system-arm")` and reaches the system
binary. Result: when system qemu is older than the patched build
(common on Ubuntu jammy — system 6.2 vs patched 8.2+):

- NuttX DDS multi-instance tests skip with WARN (`-netdev dgram
  unix` needs qemu 7.2+).
- LAN9118 RX FIFO drain bug silently drops frames under load
  (Phase 127.D was the diagnosis; the patch is the fix).
- Future qemu-side patches that the project will accumulate get
  silently ignored at any non-helper call site.

Phase 143 routes everything through the patched binary by default.

### Scope = qemu-system-arm only

Out of scope:

- **`qemu-system-riscv32`** — Espressif fork installed by
  `just esp32 setup`. Different upstream, different patches; their
  esp32c3 machine model isn't in mainline. Stays separate.
- **`qemu-system-riscv64`** — system binary works for ThreadX-RISC-V
  tests today. No patches accumulated.
- **`qemu-system-aarch64`** — Zephyr aemv8r uses ARM FVP (license-gated);
  system qemu-aarch64 is enough for other paths. No patches accumulated.

Each of those can graduate to "build from submodule" in a follow-up
phase if and when patches accumulate. Option-A (build EVERY arch
from the submodule) is overkill today.

---

## Architecture

### A. Single-helper contract

```rust
// packages/testing/nros-tests/src/qemu.rs

/// Path to the qemu-system-arm binary to use. Patched build under
/// build/qemu/bin/ wins when present (Phase 127.D + future patches).
/// Falls back to system PATH so a fresh checkout that hasn't run
/// `just qemu setup-qemu` still produces the documented [SKIPPED]
/// rather than an exec error.
pub fn qemu_system_arm() -> Command {
    let bin = std::env::var_os("QEMU_SYSTEM_ARM")
        .or_else(|| {
            let patched = project_root().join("build/qemu/bin/qemu-system-arm");
            patched.exists().then(|| patched.into_os_string())
        })
        .unwrap_or_else(|| std::ffi::OsString::from("qemu-system-arm"));
    Command::new(bin)
}
```

Every existing `Command::new("qemu-system-arm")` becomes
`qemu_system_arm()`. Single-helper means future patches automatically
propagate.

### B. `just` recipes

```just
# Single source of truth, already exists at just/qemu-baremetal.just:8-9.
QEMU_PREFIX := absolute_path("build/qemu")
QEMU_BIN := QEMU_PREFIX / "bin/qemu-system-arm"
```

Other module just files (notably `just/nuttx.just`) import or
hardcode `qemu-system-arm`. Phase 143 replaces those with `{{QEMU_BIN}}`
through a shared variable in the root justfile (or per-module
import).

### C. Fallback path

When `build/qemu/bin/qemu-system-arm` doesn't exist (e.g. a
contributor that ran `just setup --tier=minimal` and skipped the
qemu module), tests fall back to system `qemu-system-arm`. The
existing version-detection in `nros-tests::qemu::has_netdev_dgram`
already produces `[SKIPPED]` for the dgram-gated tests when the
fallback is too old. Same gate continues to work; just the patched
binary won't trigger the WARN anymore once it's the default.

### D. Doctor downgrade

`just qemu doctor` currently:
- WARN if system qemu < 7.2
- MISSING if `build/qemu/bin/qemu-system-arm` absent

After 143, the WARN class disappears for users who ran `just setup`
(patched binary present, version-detection sees ≥ 8.2). The MISSING
gate becomes the primary signal: "run `just qemu setup-qemu`".

---

## Work Items

- [ ] **143.1 — Helper landed at one call site (verify).**
      Confirm `qemu_system_arm_command()` (or equivalent) exists in
      `packages/testing/nros-tests/src/qemu.rs`. Today it's at
      `qemu.rs:18-27` per Phase 127.D. Rename if needed for
      consistency.
      **Files.** `packages/testing/nros-tests/src/qemu.rs`.

- [ ] **143.2 — Sweep all `Command::new("qemu-system-arm")` call
      sites in nros-tests.** Replace with the helper. Audit list (per
      pre-phase-143 grep):
      - `qemu.rs:532` — NuttX QEMU launch
      - `qemu.rs:643` — (verify)
      - `qemu.rs:690` — (verify)
      - `qemu.rs:733` — (verify)
      - `qemu.rs:865` — (verify)
      - `qemu.rs:879` — `has_netdev_dgram` version probe
      Re-grep at execution time; new call sites may have landed.
      **Files.** `packages/testing/nros-tests/src/qemu.rs` (and any
      other test that calls qemu-system-arm directly).

- [ ] **143.3 — Sweep `qemu-system-arm` hardcoded in just files.**
      `just/nuttx.just:193,243` references system binary directly.
      Replace with `{{QEMU_BIN}}` from `just/qemu-baremetal.just`
      (import via `mod qemu` already present). Same for any other
      module that shells out.
      **Files.** `just/nuttx.just`, possibly others (`just/freertos.just`,
      `just/qemu-baremetal.just`).

- [ ] **143.4 — Patched-binary smoke test.**
      Add `packages/testing/nros-tests/tests/qemu_patched_binary.rs`
      that asserts: (a) `qemu_system_arm()` returns a Command whose
      program path resolves to `build/qemu/bin/qemu-system-arm` when
      that file exists, (b) the patched binary reports version ≥ 8.2,
      (c) `-netdev help` lists `dgram`. Skips cleanly when the patched
      binary isn't built (no `[SKIPPED]` confusion — explicit
      precondition `nros_tests::skip!`).
      **Files.** `packages/testing/nros-tests/tests/qemu_patched_binary.rs`,
      `packages/testing/nros-tests/Cargo.toml`.

- [ ] **143.5 — Submodule pin bump.**
      Verify `third-party/qemu/qemu` is pinned to a release that
      supports `-netdev dgram,local.type=unix,...` (qemu 7.2+ per
      `just qemu doctor` WARN). Recommend pinning to qemu 8.2 LTS or
      9.x stable. Patch set may need rebasing.
      **Files.** `third-party/qemu/qemu` (submodule),
      `third-party/qemu/patches/` (if rebase needed).

- [ ] **143.6 — Doctor wording update.**
      `just qemu doctor` currently WARNs on system qemu < 7.2 with
      sudo PPA suggestion. After 143, the primary remedy is "run
      `just qemu setup-qemu`" (which gives the patched binary).
      Downgrade the sudo-PPA path to a fallback hint. The patched
      build is portable across distros; PPA isn't.
      **Files.** `just/qemu-baremetal.just` (doctor recipe),
      `just/nuttx.just` (doctor recipe).

- [ ] **143.7 — Doc page.**
      Add `book/src/internals/qemu-patched-binary.md` — explains the
      submodule + patch set + which arches go through the patched
      build vs. system + how to add a new patch. Cross-link from
      `book/src/SUMMARY.md` and from `CLAUDE.md`'s "## Practices"
      section.
      **Files.** `book/src/internals/qemu-patched-binary.md` (new),
      `book/src/SUMMARY.md`, `CLAUDE.md`.

- [ ] **143.8 — CI cache key.**
      Patched qemu binary is ~150 MB. CI should cache `build/qemu/`
      across runs so each runner builds once. Cache key off
      `third-party/qemu/qemu` submodule SHA + `third-party/qemu/patches/`
      content hash.
      **Files.** `.github/workflows/*.yml` (or local CI config —
      project may use a different CI driver; verify).

---

## Acceptance

- [ ] `grep -rn 'Command::new("qemu-system-arm")' packages/testing/`
      returns nothing (helper used everywhere).
- [ ] `grep -nE '\bqemu-system-arm\b' just/*.just justfile` returns
      only the variable definition + the doctor probe — no shell
      invocations.
- [ ] `cargo test -p nros-tests --test qemu_patched_binary` passes
      when patched binary present; `[SKIPPED]` cleanly when absent.
- [ ] `just qemu doctor` no longer WARNs after `just setup` (patched
      binary is ≥ 7.2 by construction).
- [ ] NuttX DDS multi-instance tests no longer hit the dgram-too-old
      gate on Ubuntu jammy.
- [ ] `book/src/internals/qemu-patched-binary.md` published; SUMMARY
      lists it.
- [ ] `just ci` green; previously-skipped patched-qemu-gated tests
      now run.

---

## Notes

- **Why not unify every arch.** Espressif's `qemu-system-riscv32`
  fork is sufficiently different from mainline that maintaining
  it in `third-party/qemu/qemu` would mean tracking two upstream
  trees. Their esp32c3 machine model isn't in mainline; we'd have
  to forward-port it ourselves. Cost > benefit until/unless we
  accumulate ESP-specific qemu patches. `qemu-system-riscv64` +
  `qemu-system-aarch64` work fine from system today; no patches
  pending. Phase 143 unifies the one arch that has patches; future
  phases can extend if and when other arches accumulate patches.
- **Why the helper, not env-only.** `QEMU_SYSTEM_ARM` env var keeps
  working as an override (developer pointing at a custom build).
  But making the patched binary the DEFAULT (when present) means
  users don't need to set env to get the right behaviour — it Just
  Works after `just setup`.
- **Cost.** ~10 min initial qemu build per machine, ~150 MB disk.
  Both already accepted today via `just qemu setup-qemu`; Phase 143
  doesn't add new cost, just propagates the existing artifact.
- **What happens if the submodule is dirty.** `just qemu setup-qemu`
  hard-resets the submodule + reapplies patches before each build.
  Phase 143 doesn't change that contract; the patched binary at
  `build/qemu/bin/` is always reproducible from the submodule SHA +
  patches dir.
- **Phase 142 tier interaction.** A contributor running `just setup
  --tier=minimal` would skip the qemu module. The helper fallback to
  system `qemu-system-arm` keeps their tests working (with the
  documented WARN class for too-old system qemu). Tier doesn't break
  Phase 143; the helper handles the absence cleanly.
