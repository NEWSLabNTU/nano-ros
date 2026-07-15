# Phase 247 — Weak-symbol determinism: image-level checker, CI gating, reduction

**Implements.** [issue 0050](../../issues/archived/0050-weak-symbol-audit-and-checkers.md).
**Related.** RFC-0042 / [phase-241](phase-241-platform-build-determinism.md) D3
(deterministic linking — rejected weak for the `nros-rmw-cffi` dedup in favour
of a define-once export macro); [issue 0042](../../issues/archived/0042-platform-header-architecture-fragility-libc-std-clashes.md).

**Status.** **COMPLETE — archived** (2026-07-16; work landed 2026-06-13→15).
W1.1–W1.3 (image gate + coverage map + SSoT cross-check), W2 (`just check`
wiring), W3.1 (resolved by phase-249 P4a — the weak `nros_app_register_backends`
default deleted), W3.2/W3.3 all DONE; issue 0050 resolved + archived. See
"Phase close" below — the header had simply never been flipped from Planned.

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

- **W1.1 — DONE (2026-06-13).** `scripts/check-weak-symbols-image.sh` +
  `just check-weak-symbols-image`. Coverage map (artifact `find` base + name-glob
  → override-default symbols that must be strong there); `nm` each final image
  (`.a`/`.o`/`.rlib` skipped); strong→ok, weak→FAIL, absent→WARN; skips covered
  classes whose artifacts aren't prebuilt. Validated: 10 checks across 3 real
  final images green — FreeRTOS `freertos_rs_talker_entry`
  (`nros_board_{register,poll}_netif` = `T`) + the serial ELFs
  (`_z_*_serial_*` = `T`, the same symbols that are `W` in
  `libzpico_platform_aliases.a` — proving the override lands at final link).
  Negative path confirmed: pointing the classifier at the staticlib's `W`
  `_z_open_serial_from_dev` trips FAIL. (Coverage map completed in W1.2 below —
  the **threadx RISC-V64** rows are live now; the freertos / cmake-C++ / serial /
  smoltcp rows skip until their fixtures are prebuilt on CI; px4-uorb has no image
  yet. No further W1.1 work — the rows light up passively as fixtures build.)
- **W1.1 (design, for reference)** — `scripts/check-weak-symbols-image.sh <artifact>`: `nm` the artifact,
  parse `<addr> <type> <name>`, and for each **override-default** symbol in the
  shared allowlist apply the rule above; **optional-hook** symbols may stay weak
  but are *reported*; any **owned-prefix** weak symbol (`nros_`, `nros_board_`,
  `nros_orb_`, `_z_`, `smoltcp_`, `_tx_`) present-as-weak that is NOT an
  allowlisted optional-hook fails (mirrors the source gate at the binary layer).
  Toolchain weaks (`__cxa_*`, `__gnu_Unwind_*`, FreeRTOS `vPort*`) are excluded by
  the owned-prefix filter.
- **W1.2 — DONE (2026-06-14).** Completed the coverage map so every
  image-checkable override-default class has a row (skip-until-built; no
  compile in the check):
  - board netif hooks → freertos firmware ELFs (`freertos_rs_*entry`);
  - `nros_app_register_backends` → the cmake C/C++ app images
    (`cpp_robot_entry` / `c_mixed_workspace`);
  - `_z_*_serial_*` → the serial example ELFs (`qemu-serial-{talker,listener}`);
  - `smoltcp_{init,cleanup}` → the bare-metal net ELFs (`qemu-bsp-{talker,listener}`,
    strong def from the `nros-smoltcp` driver);
  - `nros_board_init_eth` + **W3.2** `nros_board_app_stack_size/_priority` →
    the **threadx RISC-V64** firmware ELFs (`qemu-riscv64-threadx-{talker,listener}`)
    — this row is the on-platform guard for the 155.A class.
  - `nros_orb_*` → **pending**: no Cargo.toml currently links `nros-rmw-uorb`
    into an example, so there is no final image to nm; the gate self-reports it
    as a declared-but-uncovered symbol (no silent gap). Add a row when a px4
    uorb example ships.
  **Finding (the gate earned its keep): `_tx_initialize_low_level` was
  mis-classified** as "board strong". The board `.S` actually defines it
  `.global .weak` as the *sole* def (build.rs excludes the port copy) — it
  legitimately stays weak in the image. The image gate flagged the contradiction
  (`WEAK in <elf> — strong override DROPPED`); re-audited to **optional-hook**
  (overridable sole def, not image-checked). Validated against the prebuilt
  threadx ELFs: `nros_board_init_eth`/`app_stack_size`/`app_priority` all confirm
  strong.
- **W1.3 — DONE (2026-06-14).** Single source of truth without a risky format
  change: each override-default allowlist line carries an `[img: <sym> …]` token
  (the exact image-checkable symbols); both source gates already take only the
  text before `#`, so the token is invisible to them. The image gate parses the
  `[img:]` set and **cross-checks its coverage map against it**: a COVERAGE
  symbol not declared `[img:]` fails (drift), and a declared symbol with no
  coverage row is reported. Runs even with no prebuilt images (static check).
  Negative path proven: an injected undeclared COVERAGE symbol exits non-zero.

**Acceptance MET.** For each covered prebuilt artifact, every override-default
weak symbol is confirmed strong (or absent); the coverage map cannot drift from
the allowlist SSoT (cross-check + negative-path proof); an injected regression
(a weak override-default in a final image) makes the script exit non-zero
(proven live — it caught the `_tx_initialize_low_level` mis-classification).

## W2 — Gate in `just check` — DONE (2026-06-13)

- **W2.1 — DONE.** `scripts/check-weak-symbols.sh` — buildless source-level scan
  (`find` + `grep -c`), sub-second. Reads the single source-of-truth allowlist
  `scripts/weak-symbols-allowlist.txt` (path → expected weak-decl count +
  classification). Fails on a new unaudited weak site, a drifted count, or a
  stale entry.
- **W2.1 — single source of truth.** Both gates now read
  `scripts/weak-symbols-allowlist.txt`: the shell gate (W2) and the Rust gate
  `weak_symbol_audit.rs` (which dropped its inline const for `load_allowlist`).
  No two copies to drift.
- **W2.2 — DONE.** `check-weak-symbols` added to the `check:` aggregate
  (justfile) + a `[private] check-weak-symbols` recipe.
- **W2.3 — DONE.** The image gate (W1) stays out of the fast `check` — it's the
  standalone `just check-weak-symbols-image` (needs prebuilt fixtures), for
  `test-all` / per-platform CI.

**Acceptance MET.** `just check` now fails on an unaudited / drifted weak site;
a new weak symbol added without allowlisting is caught pre-merge with no build.

## Project `just check` status — RESOLVED (2026-06-14)

The phase-247 gates pass standalone (`just check-weak-symbols`,
`just check-weak-symbols-image`, the `weak_symbol_audit` test). The lib-level
clippy blockers that previously made `check-workspace` abort *before* reaching
the wired `check-weak-symbols` gate have all been **cleared** — the exact
`check-workspace` recipe now exits 0 with **zero warnings**, so the gate runs
end-to-end under `just check`. The blockers (fixed this session, all out of
phase-247 scope):

| file | lint | status |
| --- | --- | --- |
| `nros-core/src/action.rs` `register_protocol_types` | `result_unit_err` | fixed (`#[allow]`) |
| `nros-node/src/executor/arena.rs` | `collapsible_if` ×2 | fixed (let-chains) |
| `nros-macros/src/lib.rs` | `items_after_test_module` | fixed (test mod → EOF) |
| `nros/src/node.rs` | `drop_non_drop` | fixed (`let _ =`) |
| `nros-rmw-zenoh/src/shim/service.rs` | `useless_conversion` | fixed (dropped `.into()`) |

(Test-target lints across `nros-tests/tests/*` surface only under `--all-targets`,
not in `check-workspace`'s lib/bin scope.) Phase-247 introduced none of these; its
own nit (`&PathBuf`→`&Path` in `weak_symbol_audit.rs`) is fixed. Verified:
`cargo clippy --quiet --workspace --no-default-features` (the recipe) → exit 0.

## W3 — Reduction (fix-up work)

Replace weak defaults that exist only to dodge a link-order problem (not a
genuine optional hook) with a define-once / explicit-registration structure
(RFC-0042 D3 pattern). Prioritise the highest-fragility sites:

- **W3.1 — RESOLVED (2026-06-15, [phase-249](phase-249-one-registration-trigger.md) P4a).**
  The weak `nros_app_register_backends` default (`nros-c`/`nros-cpp`
  `c-stubs/weak_register_backends.c`) is **deleted**: C/C++ registration is the cmake
  `nano_ros_link_rmw` generated STRONG def (universal per `nros_platform_link_app`,
  phase-249 P2b), so a missing strong def is a **link error**, not the silent no-op
  #48-class hazard. This image gate guarded it (the symbol was asserted strong in cmake
  images, then left the coverage map — it is now generated-strong or link-error, never
  weak). Validated: native C + C++ link clean; source/image/rust weak gates green.
  C/C++-only — the *linkme* registration path (native Rust) stays per
  [phase-244 D7](phase-244-example-source-cleanliness.md) Shape B; its deletion ("one
  registration path" for Rust) is phase-249 **P4b**, deferred (P4b ↔ D7 fork; RFC-0042 D3
  already permits linkme to remain the hosted-Rust impl detail).
- **W3.2 — DONE (2026-06-13).** The 155.A-class const-weak constants in
  `threadx_hooks.c` (`nros_board_app_stack_size`/`_priority`) — a weak *data*
  symbol gcc could fold at the use site before the strong override was seen
  (155.A worked around it by dropping `const`, but a weak-data symbol relying on
  link resolution is still the footgun). **Converted to weak getter *functions***
  (`uint32_t nros_board_app_stack_size(void)` …), matching the sibling
  `nros_board_*` weak hooks in the same file; the RISC-V overlay's strong *data*
  override became a strong *function*. A weak function cannot be const-folded
  across a TU boundary, so the override deterministically wins and the `drop
  const` workaround is no longer load-bearing. Allowlist count unchanged (7;
  1:1 data→fn). **Validated end-to-end with the real `riscv64-unknown-elf`
  toolchain:** `threadx_hooks.o` emits the getters as `W` and `tx_application_
  define` references them via `R_RISCV_CALL` relocs (a real call, *not* a folded
  constant); a strong-override `.o` emits them `T`; the link resolves to the
  strong defs and `objdump` shows `lui a0,0x80` (= 512 KB) at the call, *not* the
  weak 64 KB default. Both source gates green. Full board-file compile is
  covered by the `threadx-riscv64` fixture build (CI / on-demand).
- **W3.3 — DONE (re-audit, 2026-06-13).** Re-audited every remaining
  override-default for "is the strong def *guaranteed* (always linked) → drop the
  weak and let the link error speak". Conclusion: **all remaining ones are
  legitimately *conditional*** — their strong def is gated on a board/build
  *capability*, so the weak no-op is a valid feature-absent runtime, not dead
  weight:
  - `network_glue.c` `register_netif`/`poll_netif` — strong only on an Ethernet
    board (LAN9118); weak = no-Ethernet build. Keep.
  - `platform_aliases.c` `_z_*_serial_*` / `smoltcp_{init,cleanup}` — strong only
    when serial / smoltcp is present. Keep.
  - `threadx_hooks.c` `init_eth` (strong on both threadx overlays, but a
    board-less generic threadx link legitimately wants the no-network no-op),
    `log`/`compute_rng_seed` (optional diagnostic/seed hooks). Keep.
  - `tx_initialize_low_level.S` — strong only on the RISC-V port; the Linux
    overlay uses ThreadX's generic. Conditional. Keep.
  - `callback_default.cpp` `nros_orb_*` — strong only when the px4 glue links;
    weak `-1` is the graceful "no callback" path. Keep.
  None are a const-data hazard (all weak *functions* / asm post-W3.2), and none
  is a pure link-order dodge — that was only W3.1 (register-backends) + W3.2
  (now fixed). So the allowlist does not shrink further this phase; what remains
  is genuine optional-hooks + capability-conditional override-defaults.
- Each landed W3 change updates both gates' allowlist; the image gate (W1) proves
  the replacement still resolves correctly on every platform that builds it.

**Acceptance.** The const-weak constants (W3.2) no longer rely on weak *data*
resolution — validated at the link layer on RISC-V. The remaining override-
defaults are confirmed capability-conditional (W3.3). The one pure link-order
dodge (register-backends, W3.1) is scoped to RFC-0042 D3, kept audited by both
gates until then.

## Phase close

Issue 0050 → resolved: audit complete; source gate + image gate both green and
wired (source gate in `just check`; image gate standalone for per-platform CI,
with a static SSoT cross-check that runs anywhere). Fragile weak defaults
reduced — the const-data footgun (W3.2) eliminated, validated on real RISC-V;
the remaining override-defaults confirmed capability-conditional (W3.3); the
coverage map completed and de-drifted against the allowlist SSoT (W1.2/W1.3),
which already caught and corrected a `_tx_initialize_low_level` mis-class.
Carve-out: the one pure link-order dodge (`nros_app_register_backends`, W3.1) is
scoped to RFC-0042 D3 and stays audited by both gates until D3 lands — tracked
there, not a 0050 blocker. The cross-platform image rows (freertos / cmake C++ /
serial / px4-uorb) activate as those fixtures build on their CI; the gate
self-reports any still-uncovered declared symbol, so coverage gaps are visible,
not silent. Archive 0050 on the next cut.
