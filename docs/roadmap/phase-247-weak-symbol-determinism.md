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

## W1 — Image-level weak-symbol checker

The source gate proves "no new unaudited weak *site*"; it does NOT prove the
weak default is actually **strong-overridden in the final link** on each
platform — the real failure mode (a board forgets the override / `--gc-sections`
drops the strong def → the weak no-op silently wins). Add a per-artifact gate.

- **W1.1** — A checker that, for each prebuilt fixture artifact (the
  `build/fixtures-cargo/<platform>` ELFs + the staticlibs the dup-symbol gate
  already covers), runs `llvm-nm` and, per the allowlist's classification:
  - **override-default** symbols MUST resolve to a **strong** (`T`/`D`, not
    `W`/`V`) definition in the final image (the weak no-op was overridden);
  - **optional-hook** symbols MAY remain weak (the no-op is the intended
    fallback) — but are *reported* so an unexpected strong/weak flip is visible;
  - any weak symbol in the image **not** in the allowlist fails (mirrors the
    source gate at the binary layer).
  Robust to `--gc-sections` / `--whole-archive`: assert against the *linked
  image*, not the input archives.
- **W1.2** — Reuse the `llvm-nm` + allowlist shape from the existing
  duplicate-symbol gate (the issue references `staticlib_duplicate_symbols.rs`;
  locate the current equivalent — symbol-gate tests live in
  `nros-tests/tests/{workspace_shadowing,zpico_build_matrix,cyclonedds_descriptors}.rs`).
  Share one allowlist source-of-truth between the source gate (W-existing) and
  the image gate (avoid divergence).
- **W1.3** — Per-platform: at minimum native + the cross targets whose board C
  stubs carry the weak surface (freertos/mps2-an385, threadx-linux,
  threadx-qemu-riscv64, px4/uorb). Gate on the prebuilt fixture; skip (not fail)
  when an artifact is absent, matching the build-stage-fixture pattern (no
  compilation inside tests).

**Acceptance.** For each covered artifact, every override-default weak symbol is
confirmed strong-overridden; an injected regression (delete a board's strong
override) makes the gate fail.

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
