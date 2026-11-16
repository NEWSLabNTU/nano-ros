---
id: 2618
title: "The knob delivery check refuses a build after link: a list separator and an override row read as mismatches"
status: resolved
type: bug
area: [cli, build, knobs]
severity: medium
found: 2026-11-15
resolved: 2026-11-16
resolved_in: "fix(#2618): the knob delivery check compares one spelling and leaves a stated value to configure"
related: [phase-478]
---

**Owner:** nros 139. The derived-knob resolution is phase-530's work.
**Home:** phase-478 (found by the NXP island).

## Symptom

On main 31846e9a7c the island's build fails AFTER the link step, although the
ELF is produced. The knob delivery check reports two mismatches:

1. `NROS_SUBSCRIBED_TYPE_BOUNDS`: the derived value separates entries with `;`
   and the resolved value with `,`. The two lists carry the same entries, so
   this is a false mismatch.
2. A `max_publishers` override row is flagged, although an override is an
   allowed way to state that knob.

Reported by the island (main 31846e9a7c, Zephyr `mr_canhubk3/s32k344`,
`CONFIG_NROS_MAX_PUBLISHERS=13` in a `.conf` fragment, `just BOARD_BUILD_DIR=…
board-build`; 3 builds of 3, and a rerun does not converge). Configure printed:

```text
NROS_MAX_PUBLISHERS=13, stated by Kconfig …, is above the 11 this image's entity inventory derives … (over-provisioned by the difference)
NROS_SUBSCRIBED_TYPE_BOUNDS=A=28,B=116,… DERIVED from this image's message-bound inventory
```

and the post-link check refused:

```text
NROS_DERIVED_MAX_PUBLISHERS=11 but NROS_RESOLVED_NROS_MAX_PUBLISHERS=13 -- the resolver did not carry the derived value through.
NROS_DERIVED_SUBSCRIBED_TYPE_BOUNDS=A=28;B=116;… but NROS_RESOLVED_…=A=28,B=116,…
board-build: REFUSING this image … Run `just board-build` again; the second pass converges.
```

Logs: `/mnt/mx500/aeon/worktrees/k3937-build-fix{2,3,4}.log` on the island
host.

## Resolution (2026-11-16)

- **Separator.** The message-bound fragment writes the per-type table as a
  cmake LIST (`;`). `nros_cargo_build.cmake` joins it with `,` before
  resolving, because a `;` would split into two `cmake -E env` arguments on
  the Rust lane. `scripts/check-knob-delivery.py` now compares both sides
  through one function, `knob_value()`, so the same entries in either
  separator are one value. A list with a different entry is still a mismatch.
- **A stated value.** The check's question is whether the RESOLVED value
  reached the compile. Whether a stated number may differ from the derivation
  is configure's question, and `_nros_compare_stated_to_derived` already
  answers it, which is the "over-provisioned" line above. So a mismatch with
  the derivation is excused for every stating rung, `kconfig` as well as
  `environment` and `image`. On this road a derivable knob records `kconfig`
  only when a CONFIG_ value other than the derive sentinel was stated. It is
  still reported when the resolver recorded `derived`, when the value is the
  sentinel (the double-resolve shape this gate caught three times), or when
  the compile was handed a different number.
- **The rerun advice** ("the second pass converges") is printed by the
  island's `board-build` recipe, not by nano-ros. That recipe must stop
  claiming a rerun fixes a mismatch it cannot tell from a stale pass. This is
  tracked with the island, not here.

Self-test: 6 new cases (stated-over-derived clean, stated-but-not-compiled
reported, sentinel-as-kconfig reported, derived-rung mismatch reported, list
across separators clean, list with a different entry reported). Each was
mutation-checked: removing its rule fails exactly that case.
