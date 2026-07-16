# Phase 290 — Hierarchical platform/board configuration (RFC-0049)

**Goal.** Implement RFC-0049: one knob schema declared in `nros-platform`,
`nros-platform.toml` per platform package (capabilities + defaults +
`[build.zenoh]`), the extended `nros-board.toml` per board package, the fixed
four-rung ladder with native lane front-ends, `nros config explain`, the
`nros new platform/board` scaffolders — and retire the central
`zenoh_platforms.toml`. First tenant: the phase-282 `zenoh.tx` knobs, whose
promotion decision (zephyr batch+split on, everywhere else off until
measured) ships as platform defaults.

**Status.** Planned (design locked 2026-07-16, RFC-0049 Draft).

**Implements.** [RFC-0049](../design/0049-hierarchical-platform-board-config.md)
(design-of-record — read it first; this doc is the work breakdown only).

**Depends on.** RFC-0042 (`nros-board.toml` descriptor), RFC-0014/phase-201
(out-of-tree board resolution), phase-282 (the TX knobs + measurements),
issue 0135 (shared-config ABI rule the emitter must preserve).

## Waves

### W1 — Schema + loader (no behavior change)

- [ ] W1.a `nros-platform` crate: knob schema + built-in defaults as code
  (typed; `deny_unknown_fields` on the toml side). Tables: `[capabilities]`,
  `[knobs.zenoh.tx]` (first tenant), `[build.zenoh]`.
- [ ] W1.b Loader: given (board dir, platform dir) — the explicit two-hop
  chain from the deploy key — parse + ladder-resolve
  `builtin < platform < board < env/-D`. Explicit-off semantics: a set
  front-end value always wins, including `0`.
- [ ] W1.c Capability cross-check: knob-vs-capability contradictions →
  build-time warning + downgrade, naming both files.
- [ ] W1.d Unit tests: ladder order, explicit-off override, unknown-key
  rejection, empty/absent tomls == builtins (byte-identity guard input).

### W2 — `[build.zenoh]` relocation; retire `zenoh_platforms.toml`

- [ ] W2.a Move each `[platform.X]` block verbatim (RFC-0049 open question 1:
  keys unchanged) into the platform package's `nros-platform.toml`;
  `nros-zpico-build` + the cmake glue read the per-package file through the
  loader. Delete `zenoh_platforms.toml`; grep for stragglers (docs,
  CLAUDE.md pitfall index line, build comments).
- [ ] W2.b The resolved knob set feeds the SAME generated config header /
  `-D` emission as today (issue-0135 ABI rule untouched). ZPICO_SERIAL /
  ZPICO_NO_SMOLTCP runner special-cases keep working (#189 regression
  guard: serial + xrce baremetal lanes).
- [ ] W2.c Default-off byte-identity regression: with no toml `[knobs]`
  entries and no env, the generated config header is byte-identical to
  pre-phase-290 for every platform. Full fixture rebuild + the emulator /
  rtos_e2e sweeps green.

### W3 — Board layer + front-end completion

- [ ] W3.a Extend `nros-board.toml` parsing with `[capabilities]` /
  `[knobs.*]` (additive; existing descriptor fields untouched). Confirm the
  phase-201 out-of-tree path serves the cmake lane too (RFC-0049 open
  question 3).
- [ ] W3.b Zephyr Kconfig forward → tri-state: always pass
  `-DZPICO_TX_BATCH=0|1` (+ split/flush) from Kconfig; fragment `default`
  lines mirror the zephyr platform toml; add the **drift test** asserting
  the mirror. NuttX / ESP-IDF fragments: same pattern where those
  packagings expose nros options (hand-wired, only where the host is
  Kconfig-native — no generator).

### W4 — Porter UX

- [ ] W4.a `nros config explain --board <b>`: every knob — final value +
  the rung that set it. (Optional per-knob `why` string: RFC-0049 open
  question 2 — decide here.)
- [ ] W4.b `nros new platform <name>` / `nros new board <name>
  --platform <p>`: scaffold crate skeletons + tomls with the schema as
  comments. Book page: "Porting nano-ros to a new RTOS" checklist
  (2 crates + 2 tomls, no central edits).

### W5 — First tenant policy flip (the phase-282 promotion)

- [ ] W5.a zephyr `nros-platform.toml`: `batch = true, split_lock = true,
  flush_ms = 50` + `[capabilities] per_fd_tx_ceiling = true`. All other
  platforms: knobs absent (off). Update the phase-282 archived doc's
  promotion table: decided — option C scoped to zephyr, via platform
  default.
- [ ] W5.b Re-run the phase-282 benches on zephyr with defaults-on images
  (streaming ≈180 msg/s expected; knob-off image byte-identical check);
  zephyr family e2e sweep green; release-note line: +flush_ms latency on
  non-express topics, `tx_express` escape.
- [ ] W5.c Docs: tx-tuning book page gains the "platform defaults" section;
  control-tier examples demonstrate `tx_express`.

## Acceptance

- Porter path: `nros new platform` + `nros new board` produce a compiling
  skeleton; a knob-less port builds green on built-ins.
- `nros config explain` matches the generated header for every in-tree
  board (spot-checked in CI for one board per platform).
- `zenoh_platforms.toml` gone; no lane regressions (full sweep).
- Default-off byte-identity holds everywhere except zephyr, whose flip is
  measured and release-noted.
- Kconfig drift test green; no nros-owned Kconfig outside framework
  integration layers.
- `just ci` green.
