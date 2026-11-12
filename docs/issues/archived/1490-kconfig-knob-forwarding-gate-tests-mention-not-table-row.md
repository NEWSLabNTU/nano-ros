---
id: 1490
title: "check-kconfig-knob-forwarding proves a knob is MENTIONED, not that it is a table row"
status: resolved
resolved: 2026-11-12
resolved_in: "phase-468 W4 (one knob ladder, one pairing table) and phase-484 W5a (the table generated from the knob registry); re-measured 2026-11-12"
area: build
severity: medium
phases: [468]
rfcs: [0049]
related: [0460, 0751, 1233, phase-478]
---

**Home: [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)** — homed 2026-10-17; the phase doc already tracks it in its body.

# `check-kconfig-knob-forwarding` proves a knob is MENTIONED, not that it is a table row

## The measurement

phase-468 W4's first box says the population comes first and "the claim 'one
reader' is not assumed". Measuring it turned up a live instance of issue 0460
that the gate built for issue 0460 passes.

`examples/zephyr/rust/talker`, on `native_sim`, with one line appended to
`prj.conf`:

```
CONFIG_NROS_SUBSCRIBER_RING_DEPTH=7
```

The merge order is `prj.conf` -> `prj-zenoh.conf` -> `cmake/zephyr/native-sim-line-3.7.conf`,
last wins, and none of the two later fragments sets this symbol, so the leaf
value stands. After `just zephyr build-one rust/talker zenoh`:

| | |
| --- | --- |
| `<build>/zephyr/.config` | `CONFIG_NROS_SUBSCRIBER_RING_DEPTH=7` |
| the Rust half's `out/buffer_config.rs` | `pub const SUBSCRIBER_RING_DEPTH: usize = 4;` |

That is issue 0460 exactly: the C lane took the Kconfig value and the Rust lane
compiled the crate default.

The baseline could not have shown this. Unset, the knob's Kconfig default and
its crate default are both `4`, so "delivered" and "fell back to the same
number" are the same observation. A non-default value is the only probe that
separates them.

## Why the gate passes

`scripts/check-kconfig-knob-forwarding.sh` walks every
`_nros_resolve_knob(<NAME>` in `zephyr/cmake/nros_cargo_build.cmake` — 52 of
them, `ZPICO_SUBSCRIBER_RING_DEPTH` included — and for each asks whether a
reader mentions it:

```bash
for f in "${READERS[@]}" "${DERIVED_READERS[@]}"; do
    if nros_grep_q -F "\"$knob\"" "$f"; then
        found=1
        ...
        break
    fi
done
```

For a DERIVED reader that is the right question: those files build the Kconfig
name from the env name (`CONFIG_{name}`), so a knob they name is a knob they
resolve. `nros-node`'s `env_usize` is the shape —
`nros_zephyr_build::dotconfig_usize(&format!("CONFIG_{name}"))`, no table.

For a TABULATING reader it is the wrong question. `nros-rmw-zenoh/build.rs`
resolves through an AUTHORED `KCONFIG_KNOBS` table because its env names and
its Kconfig names are different words (`ZPICO_SUBSCRIBER_RING_DEPTH` <->
`CONFIG_NROS_SUBSCRIBER_RING_DEPTH`), and no derivation can bridge that. So a
knob appears in the file — in a `rerun-if-env-changed` line, in the
`env_usize_min(...)` call — while having no row, and the gate's per-knob arm
is satisfied by the mention.

Issue 0751 is this defect one arm over: its whole finding was "the name
APPEARING is not the name being resolved", and its fix hardened the DERIVED
arm. The tabulating arm kept the mention test. The `env::var("<KNOB>")` probe
0751 added cannot reach this file either — `env_usize` calls
`std::env::var(name)` with a VARIABLE, so the literal the probe greps for is
never written.

## The population

Of the 52 forwarded knobs, seven are mentioned by a tabulating reader with no
table row. Four are cmake-DERIVED facts (`NROS_DECLARED_*`,
`NROS_ENTITY_APP_QUERYABLES`) with no Kconfig symbol at all — legitimately
table-less, since there is no `$DOTCONFIG` rung for a number cmake computed.
The other three have a Kconfig symbol and are splits:

| knob | Kconfig symbol | readers |
| --- | --- | --- |
| `ZPICO_SUBSCRIBER_RING_DEPTH` | `CONFIG_NROS_SUBSCRIBER_RING_DEPTH` | nros-rmw-zenoh (no row) |
| `NROS_PARAM_SERVICE_INBOX_BYTES` | `CONFIG_NROS_PARAM_SERVICE_INBOX_BYTES` | nros-rmw-zenoh (no row), nros-node (derived) |
| `NROS_PARAM_SERVICE_INBOX_DEPTH` | `CONFIG_NROS_PARAM_SERVICE_INBOX_DEPTH` | nros-rmw-zenoh (no row), nros-node (derived) |

The param-inbox pair is worse than the first row, and is issue 1233's shape:
TWO crates read one knob and size the SAME geometry, one of them gets the
Kconfig value and the other does not. A Zephyr Rust image that sets the symbol
gets `nros-node` sizing to N and `nros-rmw-zenoh` sizing to the default.

While measuring, the phase doc's own claim that
"`nros_zephyr_build::knob_usize` appears in exactly ONE build script in the
tree" was refuted: four build scripts call `nros_zephyr_build::knob*`
(`nros`, `nros-platform`, `nros-rmw-zenoh`, `rmw/cffi`), and `nros-node`,
`nros-params` and `nros-rmw-xrce-cffi` reach `$DOTCONFIG` through the derived
spelling.

## The fix

Two parts, and the gate half is the one that matters — the three rows are
this week's instances, the reach gap is what produces next week's.

1. The three missing `KCONFIG_KNOBS` rows.
2. The gate's tabulating arm asks for a ROW, not a mention: a forwarded knob
   that a tabulating reader mentions, and for which a Kconfig symbol exists,
   must be a row in that reader's table. A knob with no Kconfig symbol (a
   cmake-derived fact) is exempt BY SHAPE — no symbol, no rung — never by
   name.

## Acceptance

- [ ] The probe above re-run: `CONFIG_NROS_SUBSCRIBER_RING_DEPTH=7` yields
      `SUBSCRIBER_RING_DEPTH: usize = 7` in the Rust half.
- [ ] The widened gate fails on a table row removed, and passes on the tree
      with the rows present — both directions run.

## Resolved: re-measured on main, 2026-11-12

Nothing in this issue's fix list is still open. Phase-468 W4 replaced the
eight per-crate knob ladders and the two per-crate `KCONFIG_KNOBS` tables
with one ladder, `nros_zephyr_build::knob()`, and one pairing table,
`KCONFIG_PAIRS`. Phase-484 W5a then generated that table from the knob
registry (`packages/tooling/nros-zephyr-build/knobs.toml`). The gate's
tabulating arm, which asked whether a reader MENTIONED a knob, is gone. Arm 2
now harvests each pairing from the producer
(`_nros_resolve_knob(<ENV> "${CONFIG_<SYM>}")` in
`zephyr/cmake/nros_cargo_build.cmake`) and holds `KCONFIG_PAIRS` to it in
both directions.

Both acceptance boxes, measured at main `8ea4a8f2cb` on newslab-118 against
the Zephyr 3.7 workspace:

- [x] **The probe.** `examples/zephyr/rust/talker` was copied out and given
  `subscriber_ring_depth = 7` for the `zephyr_zenoh` image. Since phase-481 a
  nano-ros knob is stated in `system.toml` (`[image.zephyr_zenoh.knobs.zenoh.limits]`),
  never in a leaf conf, and the module renders it into the Kconfig fragment.
  `nros build zephyr_talker_node_bringup:zephyr_zenoh` produced:
  - `<build>/zephyr/.config`: `CONFIG_NROS_SUBSCRIBER_RING_DEPTH=7`
  - the Rust half's `nros-rmw-zenoh-*/out/buffer_config.rs`:
    `pub const SUBSCRIBER_RING_DEPTH: usize = 7;`

  The non-default value reached both lanes, so the knob was delivered, not
  defaulted.
- [x] **Both directions of the gate.** `check-kconfig-knob-forwarding` passes
  on the tree (`87 forwarded knob(s) … 65 cmake-declared pairing(s) matched
  against KCONFIG_PAIRS`). With the `NROS_ZENOH_SUBSCRIBER_RING_DEPTH` row
  deleted from `KCONFIG_PAIRS` it exits 1, saying the cmake module forwards
  it from `CONFIG_NROS_SUBSCRIBER_RING_DEPTH` while the Rust lane would
  resolve `CONFIG_NROS_ZENOH_SUBSCRIBER_RING_DEPTH`. With the row pointing at
  the wrong symbol (`CONFIG_NROS_MAX_SUBSCRIBERS`) it also exits 1. Restored,
  it passes.

What the gate still does not reach is written in its own header: a knob whose
cmake value is a computed variable declares no symbol at the call site, so
its row is authored and not checked by arm 2.

