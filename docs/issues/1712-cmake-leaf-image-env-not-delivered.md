---
id: 1712
title: "A C/C++ leaf's `[image.<id>] env` never reaches cargo on the cmake road — the APP rung exists only for Rust leaves"
status: open
type: bug
area: [build, config, cmake]
severity: medium
found: 2026-10-06
related: [1037, rfc-0049, rfc-0098, phase-445, phase-479]
---

## Summary

RFC-0049's APP rung — `[image.<id>] env` in a leaf's `system.toml` — is
delivered on the cargo road only. `nros sync` / `nros build` render it into
`<leaf>/build/<image>/nros-cargo.toml` (`cmd::leaf_settings`, layer 5), and
`nros ws leaf-system` reports that file as `NROS_LEAF_SETTINGS`, "empty for a
leaf that road does not build (a C/C++ leaf, a Zephyr one)". On the cmake road
the cargo builds of `nros-c` / `nros-cpp` receive the BOARD facts
(`cmake -E env NROS_BOARD=… NROS_BOARD_TOML=… NROS_PLATFORM_NAME=…` in
`build.ninja`) and nothing from the image's `env`.

phase-479 W5's commit says the opposite ("Cargo and cmake need no new carrier:
image env and board facts already reach build scripts through `nros-cargo.toml`
`[env]` / `corrosion_set_env_vars`"). The board half is true; the image half is
not.

## Measured (2026-10-06, issue 1037's sweep)

`examples/native/c/talker` copied out to a scratch dir, its `[image.native]`
given `env = { NROS_LOG_MAX_LEVEL = "warn", NROS_LOG_BUFFER_SIZE = "512" }`,
configured with the fixture lane's own arguments and built:

- both `nros-log` build-script outputs: `MAX_LEVEL = 0`, `BUFFER_SIZE = 256` —
  the builtins. The image's statement was dropped.
- `DYNAMIC_LOGGER_CAPACITY = 32` — the `native` board's `[board.knobs.log]`,
  so the board rung DOES arrive.
- the same build with `NROS_LOG_MAX_LEVEL=warn` exported in the shell running
  `cmake --build`: `MAX_LEVEL = 3`. The env rung works; only the image file is
  not read.

So on the cmake road an image can override its board only through the shell
that happens to run the build, which is not a declaration anyone can commit.

## Scope

Every knob a C/C++ image might state per image — not only the logging tenant.
Rust leaves (cargo road) and the workspace roads are unaffected as far as
measured.

## Fix direction

`NanoRosBoardFacts.cmake` already attaches the board facts with
`corrosion_set_env_vars`; the image's `env` rows are one more source for the
same call (`nros ws leaf-system` could print them, or the leaf-settings file
could be read), applied BELOW an exported variable so the ladder's top rung
still wins.
