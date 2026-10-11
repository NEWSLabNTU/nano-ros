---
id: 1796
title: "On a Zephyr Rust image a DERIVED pool size reaches the C lane only — the Rust lane builds the crate default"
status: resolved
type: bug
area: [zephyr, build, sizing]
severity: medium
found: 2026-10-11
resolved: 2026-10-11
related: [0460, 1033, 1505, 1721, phase-481, rfc-0100]
---

## Resolution (2026-10-11)

Fixed by #1949, along the issue's first road: no second Kconfig render, no
configure re-run. `nros_derived_knob_env()` (`zephyr/cmake/nros_cargo_build.cmake`)
collects every knob resolved on the `derived` rung as `KEY=VALUE`, and
`zephyr/CMakeLists.txt` appends the list to the Rust lane's existing
`cmake -E env` carrier, `NROS_BOARD_FACTS_ENV` (issue 1662). That puts the value
on rung 1 of `nros_zephyr_build`'s ladder, ahead of the `-1` in `$DOTCONFIG`.
Only the derived rung goes: a value is derived only when nobody stated one, so
the forward cannot outrank a statement.

Measured, against the acceptance below:
- `NROS_XRCE_MAX_SUBSCRIBERS` is deleted from `rust/talker`'s `zephyr_xrce` image.
- The Rust lane's command in `build.ninja` carries
  `NROS_XRCE_MAX_SUBSCRIBERS=0;NROS_EXECUTOR_MAX_CBS=1;NROS_EXECUTOR_ACTION_CLIENTS=0`.
- `case_19_xrce_rust_pubsub_e2e` passes.
- With the append commented out, the same rebuild fails it 3/3.

The fix moves every derived knob on every Zephyr Rust image, not only this one,
so every `build-rust-*` Zephyr fixture was rebuilt and all Zephyr Rust tests
run: 17/18 pass. The one red (`zephyr_rust_zenoh_action_goal_completion_rate`)
is an unmet precondition: its native peer was not built in that tree.

Not done here: the OTHER stated pool rows in Zephyr Rust images that the
inventory now answers on both lanes. Each needs its own measurement before it
goes, as this one had.

## Measured

phase-481 W3 (PR #1895) tried to delete a stated pool row that the image's own
entity inventory already answers. `examples/zephyr/rust/talker`, image
`zephyr_xrce`, `NROS_XRCE_MAX_SUBSCRIBERS`:

- the configure's inventory DERIVES `0` (`<build>/nros/entity_inventory.cmake`,
  `derived`), equal to the stated `0`, so the `env` row was deleted;
- the image then booted
  `HEAP EXHAUSTED (TOO SMALL): request 283912 bytes, arena 131584` — the XRCE
  session sized at the crate default, not at 0 — and
  `case_19_xrce_rust_pubsub_e2e` stayed red until the row came back.

So on this road a derived pool value is not a substitute for a stated one, and
every Zephyr Rust image keeps its stated pool rows for that reason alone (the
phase-481 doc's pool table, "restored").

## Cause (read)

`_nros_resolve_derivable_knob` (`zephyr/cmake/nros_cargo_build.cmake`) puts a
derived value through `_nros_resolve_knob`, whose only sink is the cache pair
`NROS_RESOLVED_<KNOB>` / `NROS_KNOB_SOURCE_<KNOB>`. The **C lane** reads that:
its cargo command bakes every resolved knob in. The **Rust lane** does not —
zephyr-lang-rust's `rust_cargo_application` builds its own cargo command, and
the crates' build scripts resolve the knob through `nros_zephyr_build`'s ladder
from `$DOTCONFIG`, where the symbol still holds the derive sentinel `-1`. A `-1`
there means "derive", nothing derived it on that lane, so the crate takes its
builtin. Issue 0460's split (Kconfig reaches C, not Rust), one rung over: the
DERIVED rung reaches C, not Rust.

Stated values do not have this problem: a Kconfig value is in `$DOTCONFIG`, and
an `[image.<id>] env` row with no Kconfig symbol rides issue 1712's `--config`
`[env]` file, which the Rust lane gets as `NROS_IMAGE_ENV_CARGO_FLAGS`.

## What would close it

Carry the derived rung to the Rust lane by the road stated values already use:
write each DERIVED knob into the image's unforced `--config` `[env]` file (so
an exported variable and a stated row still win), or into the Kconfig fragment
the module renders, so `$DOTCONFIG` holds the number instead of `-1`. The second
keeps one carrier for both lanes, but the fragment is rendered at
`module_ext_root` time, BEFORE the inventory exists, so it needs a second
render after the inventory or a configure re-run.

**Acceptance:** delete `NROS_XRCE_MAX_SUBSCRIBERS` from `rust/talker`'s
`zephyr_xrce` image; the image boots and `case_19_xrce_rust_pubsub_e2e` passes,
and the Rust lane's build reports the derived value (not the crate default).
Then the other Zephyr Rust pool rows the inventory answers can go.
