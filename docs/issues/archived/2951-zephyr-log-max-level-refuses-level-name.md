---
id: 2951
title: "The Zephyr image hook refuses a level name for `log.max_level`, which knobs.toml and the cargo road both accept"
status: resolved
type: bug
severity: low
area: [zephyr, config, logging]
related: [phase-478, 2952]
found: 2026-11-24
resolved: 2026-11-25
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

## Fix

`KnobRow::name_index` (nros-zephyr-build) maps a name to its index in the
row's `names`, case-insensitively. `knob_env_row` renders a name as that
index, so the image rung, `[image_defaults]` and the board rung carry the
number on every road: the Kconfig default parses, and `nros-log` takes a
number as it took a name. The `-DNROS_OVERRIDE_LOG_MAX_LEVEL=<name>` path,
which feeds Kconfig without `knob_env_row`, maps a name the same way.

`log.max_level` is the only registry row with `names`.

## Test

`leaf_kconfig::a_log_level_name_renders_as_its_index`: `"info"` renders
`CONFIG_NROS_LOG_MAX_LEVEL=2`, `"WARN"` 3, `2` 2; `"loud"` is refused with
"an integer or one of trace/..."; the override `error` renders 4.
