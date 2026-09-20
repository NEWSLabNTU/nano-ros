---
id: 1397
title: "`codegen-system`'s `--target` names an `[image.*]` BLOCK, and
  `check_executor_capacity` spends the same string as a PLATFORM name and as a
  BOARD key - three vocabularies, one argument"
status: open
type: bug
area: cli, codegen
severity: medium
found: 2026-09-20
related: [issue-1312, issue-1285, issue-0257]
---

## The two readings

`nros codegen-system --target <t>` names an `[image.<t>]` / `[deploy.<t>]`
BLOCK in the bringup's `system.toml`. That is the author's namespace: nothing
ties a block id to a platform, a board, or anything else.

One consumer reads it as something else. At
`packages/cli/nros-cli-core/src/cmd/codegen_system.rs:377` (origin/main) the
bake passes the block id straight into the capacity check:

```rust
crate::orchestration::model_ingest::check_executor_capacity(
    &model,
    target.as_deref(),          // <- an [image.*] / [deploy.*] BLOCK id
    ...
```

and `check_executor_capacity`
(`packages/cli/nros-cli-core/src/orchestration/model_ingest.rs:473`) spends it
twice, on two questions that are neither the block's:

| site | reads the string as | vocabulary it really wants |
| --- | --- | --- |
| `model_ingest.rs:500` -> `resolve_max_cbs_through_ladder(platform)` (`:439`) | a **PLATFORM** name | the RFC-0049 platforms tree: `posix`, `zephyr`, `freertos`, `nuttx`, `threadx-linux`, ... (`PlatformsTree::chain`) |
| `model_ingest.rs:512` -> `sz::board_honors_entry_sizing(deploy_key)` | an entry **BOARD key** | `nros_entry_lower::BOARD_KEYS`: `native`, `posix`, `zephyr`, `native_sim/native/64`, `mps2-an385-freertos`, ... |

Both are right for `native` / `posix` and for nothing else, and only because an
image is conventionally named after the platform it builds. The block id is a
third namespace that answers neither question.

## Why nothing is red today

**The ladder half is invisible.** No platform declares `max_cbs` in its
`[knobs.executor]` (measured: `grep -rn max_cbs packages/platform/*/nros-platform.toml
config/*/nros-platform.toml` is empty). So `PlatformsTree::chain("rt_host")`
fails, `resolve_max_cbs_through_ladder` returns `None` by design - it is
deliberately tolerant - and the check falls back to
`executor_env_only("max_cbs", DEFAULT_MAX_CBS)`, which is the same number the
ladder would have produced for a correctly-named platform. Two paths, one
answer, no diagnostic. The first platform to declare a `max_cbs` splits them
silently, and the split lands in a check whose whole job is to be believed.

**The board half is not invisible; nobody had looked.** It is wrong in both
directions RIGHT NOW, for any image whose block id is not its platform's name:

- `[image.rt_host] board = "linux"` - a HOSTED image. Hosted boards override
  `BoardEntry::run_with_deploy_sized` (phase-271 / issue #110), so the derived
  sizing applies and the check should be vacuous. Read as a platform name,
  `rt_host` is not `native`/`posix`, so the bake REFUSES a system that is
  fine: *"set `NROS_EXECUTOR_MAX_CBS` (currently 4) to at least 12"*.
- `[image.native] board = "native_sim/native/64"` - a ZEPHYR image whose block
  carries the host's name (legal: `native_sim/native/64` is exactly how
  `examples/workspaces/rust` spells a Zephyr image's board). Firmware boards
  take the default trait body and open at the compiled `MAX_CBS`, so nine
  entities against four is the boot-time `create_* (code=-6 Full)` issue 0257
  exists to catch. Read as a platform name, `native` says "hosted, it derives
  its way out" and the image SHIPS.

Both measured on the bake itself
(`codegen_system::tests::an_image_takes_its_capacity_rule_from_its_board_not_its_block_name`):
nine callback entities, derived twelve, build default four -> `rt_host`
refused, `native` waved through.

## Why `--for-entry` widens it

Issue 1312 (PR #1071) added `--for-entry <pkg>`: a build shim names the ENTRY
PACKAGE and `leaf_system::for_entry` resolves the `[image.*]` that claims it.
The resolved id then takes `--target`'s place everywhere below.

Under `--target`, a caller who wanted a firmware verdict could get one by
accident, by spelling the target after the platform - which is what every
synthesised target in the tree did (`--target zephyr-<rmw>`, `--target nuttx`,
`--target esp-idf`, `--target platformio`; 1312's sweep). Under `--for-entry`
the caller does not choose the string at all: it is whatever the bringup author
named the block, resolved from the entry. `zephyr_native_sim`, `rt_host`,
`fw`, `robot` - an id that looks like a platform is now the exception, not the
convention. The accident stops being available, and it was load-bearing.

## Acceptance

A bake whose image id is not its platform's name gets the capacity verdict its
BOARD earns: a hosted image derives its way out, a firmware image is refused
with the 0257 message and the count. Both halves in one workspace, so neither
is evidence without the other.
