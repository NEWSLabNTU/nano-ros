---
id: 1736
title: "Six gates recognise their defect by ONE spelling; an equivalent spelling passes — and a live stale-toolchain literal"
status: resolved
type: bug
area: [tooling, build]
severity: medium
found: 2026-10-07
related: [phase-472, issue-0196, issue-1546, issue-1735]
resolved_in: "gate-reach follow-up, item 2"
---

## What the re-audit measured

From the 2026-10-07 gate-reach re-audit
([audit-findings-2026-10-07](../../development/audit-findings-2026-10-07.md)).
Each mutation passed (rc 0); each control, the gate's own spelling, failed.

| gate | spelling that passes | spelling the gate knows |
| --- | --- | --- |
| `nuttx-links-snapshot` | Rust `format!("{}/staging", d)`; a CMake `target_link_directories(… "${NUTTX_DIR}/staging")` (population is Rust only) | `.join("staging")` |
| `nuttx-shared-tree-headers` | Rust `format!("{}/include", NUTTX_DIR)` | `Path::new(&nuttx_dir).join("include")` |
| `rtos-target-os` | build-script `env::var("CARGO_CFG_TARGET_OS") != "none"` — the shape of the ORIGINAL site (`nros-zpico-build`'s hosted test) | `#[cfg(not(target_os = "none"))]` |
| `nested-cargo-lock-discipline` | `Command::new(std::env::var_os("CARGO").unwrap())` inline — the docstring's own example (`COMMAND_NEW` needs an identifier) | `let cargo = …; Command::new(cargo)` |
| `zephyr-workspace-resolvers` | `${NROS_ZEPHYR_WORKSPACE:-$repo_root/zephyr-workspace}` — a fourth spelling of the chain, fingerprinted only by the legacy `nano-ros-workspace` token | the same with `../nano-ros-workspace` |
| `staleness-probe-exemptions` | a copy of `is_config_header_stamp_with_header` in `binaries/mod.rs` — rule 1's predicate list is AUTHORED and lacks staleness.rs's third predicate | a copy named `is_cargo_out_dir_product` |
| `sdk-store-not-enumerated` | a hard-coded versioned store path (`~/.nros/sdk/ninja/1.11.1-nros1/…`) — the headline says "constructed from the pin", only enumeration is banned | `ls … \| sort -V \| tail -1` |

## Live defect behind the last row

`scripts/check-cpp-freestanding-mechanisms.py:69` hard-codes
`~/.nros/sdk/arm-none-eabi-gcc/13.2-nros1/…` while `nros-sdk-index.toml` pins
`13.2-nros5`. On a host with the pinned store only, the arm arm falls back to a
`PATH` compiler or SKIPs; on a host holding the stale `nros1` it measures the
wrong compiler. It should resolve through `nros sdk-path` / the pin readers.

## Fix direction

Harvest (W7 `harvest.reconcile`) where an authored predicate list stands in for
a derivable one; otherwise match the RULE (a path built from a nuttx root, a
`target_os` comparison in any form, a cargo spawned from `$CARGO`) rather than
one spelling, with a selftest row per spelling.

## Resolution

Each gate now matches the RULE, with a normal-path selftest row per spelling.

| gate | change (helper) | mutation | old rc | new rc |
| --- | --- | --- | --- | --- |
| `nuttx-links-snapshot` | Rust `.join("staging")` OR a literal whose segment after an interpolation is `staging`; every other build language (`file_kinds` cmake/shell/just/make, `comments` stripped) a `/staging` after a variable expansion | `format!("{}/staging", d)` in a build.rs | 0 | 1 |
| | | `target_link_directories(… "${NUTTX_DIR}/staging")` in a CMakeLists | 0 | 1 |
| `nuttx-shared-tree-headers` | Rust `format!` literal `"…}/include…"` on a line naming a nuttx binding (or a tainted one) | `format!("{}/include", NUTTX_DIR)` | 0 | 1 |
| `rtos-target-os` | rule 2b: a build script's `… == / != "none"` in a file reading `CARGO_CFG_TARGET_OS` (`comments`), classified rows for the two bare-metal questions; population via `file_kinds` | `env::var("CARGO_CFG_TARGET_OS").unwrap() != "none"` | 0 | 1 |
| `nested-cargo-lock-discipline` | `Command::new(<path>::var(_os)("CARGO")…)` inline | `Command::new(std::env::var_os("CARGO").unwrap()).args(["build",…])` | 0 | 1 |
| `zephyr-workspace-resolvers` | the override rung WITH a fallback (`${NROS_ZEPHYR_WORKSPACE:-<non-empty>}`, `environ.get(…, x)`, `var(…).unwrap_or`) is the ladder; `:-}` / `/nonexistent` are not | `ws="${NROS_ZEPHYR_WORKSPACE:-$repo_root/zephyr-workspace}"` | 0 | 1 |
| `staleness-probe-exemptions` | the predicate list is HARVESTED (`harvest.reconcile`) from what `exempt_probe_input` uses in `staleness.rs` | a copy of `is_config_header_stamp_with_header` in `binaries/mod.rs` | 0 | 1 |
| `sdk-store-not-enumerated` | shape 3: a hard-coded VERSION on a real-store path (`.nros`/`$NROS_HOME`/`$NROS_STORE` `/sdk/<tool>/<digit>`, `$NROS_SDK_STORE/<tool>/<digit>`); a selftest's synthetic `$d/store/…` is not | `ninja="$HOME/.nros/sdk/ninja/1.11.1-nros1/bin/ninja"` | 0 | 1 |
| controls | | `d.join("staging")`; `cp "$NROS_HOME"/sdk/zenohd/*/bin/zenohd` | 1 | 1 |

### Live defects fixed

- `scripts/check-cpp-freestanding-mechanisms.py` hard-coded
  `13.2-nros1`; it now constructs the path from the pin through
  `scripts/lib/sdk-pin.sh` (`nros_sdk_pinned_version`, the one reader; there is
  no Python twin, so it shells to it) under `$NROS_SDK_STORE` / `~/.nros/sdk`.
- The widened `zephyr-workspace-resolvers` found TWO more live fourth
  spellings: `scripts/ci/runner-sweep.sh` and `scripts/dev/two-tree-check.sh`
  (`${NROS_ZEPHYR_WORKSPACE:-$root/zephyr-workspace}`, which skips the 4.4 and
  store rungs). Both now call `scripts/lib/zephyr-workspace.sh --absolute
  resolve-or-default`.
