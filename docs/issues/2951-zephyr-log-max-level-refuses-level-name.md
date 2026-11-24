---
id: 2951
title: "The Zephyr image hook refuses a level name for `log.max_level`, which knobs.toml and the cargo road both accept"
status: open
type: bug
severity: low
area: [zephyr, config, logging]
related: [phase-478, 2952]
found: 2026-11-24
---

## What is wrong

An image on a Zephyr board that writes

```toml
[image.fvp.knobs]
log.max_level = "info"
```

is refused at configure:

```
[image.fvp] `knobs log.max_level = "info"`: on Zephyr this knob is CONFIG_NROS_LOG_MAX_LEVEL, an int -- write a decimal number
```

The same statement is accepted everywhere else:

- `packages/tooling/nros-zephyr-build/knobs.toml`, row `log.max_level`:
  `type = "int"` with
  `names = ["trace", "debug", "info", "warn", "error", "fatal", "off"]` and
  the comment "A project or board may also write the level's name; the
  index is the value."
- `packages/cli/nros-cli-core/src/cmd/leaf_settings.rs`, `knob_env_row`: an
  `int` row with `names` admits a name (case-insensitive) and passes it on
  unchanged as the env row's value (`NROS_LOG_MAX_LEVEL=info`).
- `packages/core/nros-log/build.rs`: reads `NROS_LOG_MAX_LEVEL` as text and
  `parse_log_level` accepts a name or a number.

The west road is the only reader that wants a number:
`packages/cli/nros-cli-core/src/cmd/leaf_kconfig.rs` hands the row's text to
`kconfig_value`, whose `SymType::Int` arm is a bare `parse::<i64>()`. Both
callers produce the error above (the per-row loop over the image's rows, and
the registry-knob loop that renders `[image.<id>.knobs]` as Kconfig
defaults). Neither consults the row's `names`.

## Consumer impact

A Zephyr consumer (the island port, nano-ros pin
`55d9a6d761b64d4633d3ec6820ad37f68f2acfe7`) writes
`knobs.log.max_level = 2` to get past it. The value is the same, but the
spelling the registry documents fails on one road only, and a board or
project file shared between a Zephyr and a non-Zephyr image cannot use the
name.

## Fix direction

Map a name to its index once, where the row is validated: `knob_env_row`
already matched the name against `row.names`, so it can emit the index
(`"2"`) instead of the name. Every road then carries a number: the Kconfig
default parses, `nros-log` still accepts it, and the CMake-side reader asked
for by issue 2952 gets an integer it can compare. The alternative,
documenting ints only, would drop `names` from the row and break images that
already write a name on the cargo road.

## Test

A `leaf_kconfig` render test with `log.max_level = "info"` in
`[image.<id>.knobs]` renders `CONFIG_NROS_LOG_MAX_LEVEL=2`; the same for an
upper-case name. A name not in `names` is still refused with the
"an integer or one of trace/..." text.
