# Phase 247 — Weak-symbol determinism: image-level checker, CI gating, reduction

**Implements.** [issue 0050](../issues/0050-weak-symbol-audit-and-checkers.md).
**Related.** RFC-0042 / [phase-241](phase-241-platform-build-determinism.md) D3
(deterministic linking — rejected weak for the `nros-rmw-cffi` dedup in favour
of a define-once export macro); [issue 0042](../issues/0042-platform-header-architecture-fragility-libc-std-clashes.md).

**Status.** Planned (2026-06-13).

## Why

Weak symbols (`__attribute__((weak))`, `.weak`) resolve by archive order,
`--gc-sections`, and `--whole-archive` — a weak default can be silently kept
instead of the intended strong override with **no link error**, only a runtime
mis-behaviour (the #48-class "registered into the wrong instance" hazard; the
155.A const-weak-inlining bug in `threadx_hooks.c`). Issue 0050 landed the
foundation; this phase closes it.

## Already landed (issue 0050, pre-phase)

- **Audit** of the ~26 owned weak symbols across 10 files, classified
  override-default vs optional-hook with the strong-def source.
- **Source-level gate**
  `nros-tests/tests/weak_symbol_audit.rs::owned_weak_symbols_are_audited` — fails
  when a non-allowlisted owned file introduces a weak symbol, or an allowlisted
  file's weak-decl count drifts. Fast, buildless, platform-independent. The
  allowlist IS the audit.

This phase adds the heavier checks, wires gating into `just check`, and does the
reduction fix-ups.

---

## W1 — Image-level weak-symbol checker (script-based)

The source gate proves "no new unaudited weak *site*"; it does NOT prove the
weak default is actually **strong-overridden in the final link** on each
platform — the real failure mode (a board forgets the override / `--gc-sections`
drops the strong def → the weak no-op silently wins). Add a per-artifact gate,
**driven by a shell script over `nm`** (preferred over a Rust test — buildless,
trivially CI-wireable, mirrors the `scripts/check-*.sh` gate family).

### Validated method (probed 2026-06-13 on real artifacts)

- **Tooling: one cross-arch tool — `llvm-nm`.** Confirmed it reads the
  `thumbv7m-none-eabi` FreeRTOS ELF identically to `arm-none-eabi-nm`
  (`nros_board_register_netif → T`, `nros_board_poll_netif → T`). The script
  takes `NM=${NM:-llvm-nm}` so a platform can override (e.g. a sysroot `nm`).
- **Check FINAL LINKED IMAGES, not `.a` archives.** An input staticlib
  *legitimately* holds the weak default as `W` (confirmed:
  `libzpico_platform_aliases.a` → `_z_open_serial_* = W`, `smoltcp_init = W`) —
  the override happens at the final link. So the gate runs on executables / `.elf`
  firmware, never `.a`.
- **`--gc-sections` semantics — the rule per override-default symbol:**
  - **absent** from the image → fine (unused here; gc'd).
  - present as **strong** (`nm` type `T`/`t`/`D`/`d`/`R`/`B`/`b`) → correct (the
    override won).
  - present as **weak** (`W`/`V`/`w`/`v`) → **FAIL** — the strong override was
    dropped, the no-op silently won.
  Proven: in `freertos_rs_talker_entry` (final ELF) both board-netif hooks are
  `T`; in the staticlib they are `W`. A deleted board override would surface the
  symbol as `W` in the ELF → caught.

### Work

- **W1.1** — `scripts/check-weak-symbols-image.sh <artifact>`: `nm` the artifact,
  parse `<addr> <type> <name>`, and for each **override-default** symbol in the
  shared allowlist apply the rule above; **optional-hook** symbols may stay weak
  but are *reported*; any **owned-prefix** weak symbol (`nros_`, `nros_board_`,
  `nros_orb_`, `_z_`, `smoltcp_`, `_tx_`) present-as-weak that is NOT an
  allowlisted optional-hook fails (mirrors the source gate at the binary layer).
  Toolchain weaks (`__cxa_*`, `__gnu_Unwind_*`, FreeRTOS `vPort*`) are excluded by
  the owned-prefix filter.
- **W1.2** — Artifact coverage map (symbol → the image(s) that should link it
  strongly), since `--gc-sections` means each symbol only appears where used:
  - board netif/stack hooks → freertos / threadx firmware ELFs
    (`examples/qemu-arm-freertos/.../target/.../*entry`, threadx fixtures);
  - `nros_app_register_backends` → the **cmake C/C++ app** images (the
    `cpp_robot_entry` / `pure_c_workspace` cmake fixtures — NOT the Rust
    bare-metal ELFs, which never link the C register dance);
  - `_z_*_serial_*` / `smoltcp_*` → a **serial** example final ELF
    (serial-talker/listener once Phase 244.D1 Wave D lands them);
  - `nros_orb_*` → a px4/uorb link.
  Gate on the **prebuilt fixture**; skip (not fail) when the artifact is absent
  (build-stage-fixture pattern — no compilation in the check).
- **W1.3** — Share ONE allowlist source-of-truth with the source gate
  (`weak_symbol_audit.rs`). Options: emit the allowlist to a generated data file
  the shell script reads, or have the script parse the `ALLOWLIST` const. Avoid
  two drifting copies.

**Acceptance.** For each covered prebuilt artifact, every override-default weak
symbol is confirmed strong (or absent); an injected regression (delete a board's
strong override) makes the script exit non-zero.

## W2 — Gate in `just check`

The source gate currently only runs under `cargo nextest`. Wire a fast weak-symbol
check into the `just check` aggregate (justfile:281), matching the existing
`check-*` sub-recipe pattern (`[private] check-…: @bash scripts/check-….sh`).

- **W2.1** — `scripts/check-weak-symbols.sh` — the source-level scan (port the
  `weak_symbol_audit.rs` logic, or have the script invoke that single nextest
  test). Buildless + fast so it fits the `just check` budget (the other
  `check-*` gates are sub-second shell scripts). Single allowlist source of
  truth shared with the Rust gate.
- **W2.2** — add `check-weak-symbols` to the `check:` dependency list.
- **W2.3** — the image-level gate (W1) stays under `test` (it needs prebuilt
  fixtures), wired into `just test-all` / the per-platform `ci` lanes, not the
  fast `just check`.

**Acceptance.** `just check` fails on an unaudited / drifted weak site; a new
weak symbol added without allowlisting is caught pre-merge without a full build.

## W3 — Reduction (fix-up work)

Replace weak defaults that exist only to dodge a link-order problem (not a
genuine optional hook) with a define-once / explicit-registration structure
(RFC-0042 D3 pattern). Prioritise the highest-fragility sites:

- **W3.1** — `nros_app_register_backends` weak/strong dance
  (`nros-c`/`nros-cpp` `c-stubs/weak_register_backends.c` ↔ the cmake-generated
  strong stub). This is the #48-class hazard. Evaluate the RFC-0042 D3
  define-once export-macro shape; if adopted, drop the weak default.
- **W3.2** — the 155.A-class const-weak constants in `threadx_hooks.c`
  (`nros_board_app_stack_size`/`_priority`) — a weak *data* symbol that the
  compiler can inline at the use site before the strong override is seen. Prefer
  a getter hook or an explicit board-supplied config struct over a weak `const`.
- **W3.3** — re-audit each remaining override-default: if the strong def is
  *guaranteed* (always linked), the weak default is dead weight + a footgun —
  drop it and let the missing-symbol link error speak. Keep weak only for
  genuinely-optional hooks (no-op IS a valid runtime).
- Each W3 change: update both gates' allowlist; the image gate (W1) proves the
  replacement still resolves correctly on every platform.

**Acceptance.** The register-backends dance + the const-weak constants no longer
rely on weak resolution; the allowlist shrinks to genuine optional-hooks +
unavoidable override-defaults; both gates green.

## Phase close

Issue 0050 → resolved: audit complete, source + image gates green and wired
(`just check` + per-platform CI), fragile weak defaults reduced. Archive 0050.
