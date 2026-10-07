---
id: 1736
title: "Six gates recognise their defect by ONE spelling; an equivalent spelling passes — and a live stale-toolchain literal"
status: open
type: bug
area: [tooling, build]
severity: medium
found: 2026-10-07
related: [phase-472, issue-0196, issue-1546, issue-1735]
---

## What the re-audit measured

From the 2026-10-07 gate-reach re-audit
([audit-findings-2026-10-07](../development/audit-findings-2026-10-07.md)).
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
