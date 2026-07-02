---
id: 127
title: "NuttX Rust `*_entry` demos can't be build-asserted as fixtures — standalone `[[bin]]` link fails on unresolved libc/syscall symbols"
status: resolved
type: tech-debt
area: testing
related: [phase-275, rfc-0026, rfc-0032, issue-0130]
resolved_in: "phase-127-nuttx-entry-link (2026-07-03)"
---

## Resolution summary

Resolved by the **board-centric NuttX entry link** (RFC-0032 "third leg"),
landed on `phase-127-nuttx-entry-link` after a 3-risk spike (all verified:
`-Tdramboot.ld` by-name via propagated `-L`; vectortab via
`+whole-archive`; `-lgcc` via the driver multilib — linked AND booted under
QEMU, byte-parity with the `workspace-rust-qemu-nuttx` control):

- **`nros_board_common::nuttx_image_link`** — the board crate's build.rs
  stages the dynamic link pieces (cpp-preprocessed `dramboot.ld` →
  `OUT_DIR`, `arm_vectortab.o` + empty-builtins stub archived into
  `libnros_nuttx_boot.a`, `-L` for OUT_DIR/staging/board) and emits the
  PROPAGATING `cargo:rustc-link-search`/`rustc-link-lib` directives
  (`rustc-link-arg` does NOT propagate — that asymmetry is the design).
  `NUTTX_DIR`-gated; parameterized via the `NUTTX_*` env family.
- **Static args** (`-Tdramboot.ld`, `--entry=__start`, `-nostartfiles`,
  `-nodefaultlibs`, the kernel-lib `--start-group` list, `-lgcc`) in the
  Entry `.cargo/config.toml` rustflags, SSoT'd in the board descriptor's
  `cargo_config` (nros-board.toml) — with the build-std `libc` patch line
  under the SINGLE `[patch.crates-io]` table (blocker 1: the old
  `nuttx-libc-patch.sh` appended a second header → invalid TOML; now
  awk-inserts under an existing table).
- The two old per-app build.rs copies (`qemu_nuttx_entry`,
  `logging-smoke-nuttx-qemu-arm`) were **deleted** (leaving them collides:
  `multiple definition of g_builtin_count`) — both migrated onto the board
  mechanism.
- 6 `[[fixture]]` rows (NROS_LOCATOR/NROS_DOMAIN_ID baked at build time) +
  `tests/nuttx_entry_build.rs`; the 6 nuttx entries left the
  `examples_fixture_coverage.rs` ALLOWLIST (now empty).

The orthogonal Entry-path runtime gap found during the spike — no-op
`BoardInit::init_hardware` never configures eth0 → ConnectionFailed for any
*networked* nuttx-entry e2e — is issue **#130**.
