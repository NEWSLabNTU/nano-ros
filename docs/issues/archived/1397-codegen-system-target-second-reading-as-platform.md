---
id: 1397
title: "`codegen-system`'s `--target` names an `[image.*]` BLOCK, and
  `check_executor_capacity` spends the same string as a PLATFORM name and as a
  BOARD key - three vocabularies, one argument"
status: resolved
type: bug
area: cli, codegen
severity: medium
found: 2026-09-20
related: [issue-1312, issue-1285, issue-0257]
resolved_in: "fix(#1397): the capacity check takes the image's platform, not the name of the block that selects it"
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

Both measured, on the bake itself, before the fix - see Resolution.

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

## Resolution

**The capacity check takes the image's resolved PLATFORM, as a type, not a
string.** `check_executor_capacity`'s second argument is now
`board_descriptor::PlatformKind`, and each of the two facts is read off it
through its own accessor - `kebab()` for the ladder's platform name,
`honors_entry_sizing()` for the board rule. No single value carries two
meanings, and neither meaning is a block id.

The resolution is the one the bake already performs. `derive_target_rtos` had
resolved the block -> `[image.*] board` -> board catalog -> descriptor ->
platform since the issue-1285 follow-up (PR #938) and then thrown everything
but the tier key away; it is now a view of
`tier_resolver::derive_target_platform`, which is what `codegen_system::run`
calls once (`codegen_system.rs:278-310`) and hands to both consumers. No
substring of any name is read, which is what #938 removed.

Measured, in one bake over one workspace
(`codegen_system::tests::an_image_takes_its_capacity_rule_from_its_board_not_its_block_name`):
nine callback entities, derived twelve, build default four.

| image | board | platform | before | after |
| --- | --- | --- | --- | --- |
| `[image.rt_host]` | `linux` | posix (hosted) | **refused**, "raise `NROS_EXECUTOR_MAX_CBS`" | passes - the derived 12 covers 9 |
| `[image.native]` | `native_sim/native/64` | zephyr (firmware) | **passed**, vacuously | refused, "registers 9 callback entities ... holds 4 ... issue 0257" |

The two halves are each other's negative control: they take OPPOSITE verdicts,
so a fix that merely inverted the predicate fails one of them. Mutating
`PlatformKind::honors_entry_sizing` to `true` fails the Zephyr half (and three
unit tests); to `false` fails the hosted half (and
`sizing_honoring_board_derives_its_way_out`); making `derive_target_platform`
ignore the board fails it and the whole `target_rtos` suite (11 tests).

### The ladder half, and why it has a different kind of test

It still cannot be observed from a bake - no platform declares `max_cbs`, so
both paths end at four - and inventing a fixture platform to make it visible
would test the fixture. So the boundary is asserted directly, in
`the_ladder_is_handed_a_name_the_platforms_tree_knows`: every `PlatformKind` a
board can resolve to answers in the in-tree `PlatformsTree`, the block ids that
used to be passed instead (`rt_host`, `zephyr_native_sim`, `robot`, `fw`,
`zephyr-zenoh`) answer in no tree at all, and `resolve_max_cbs_through_ladder`
itself returns `Some` for the first and `None` for the second. Mutating
`PlatformKind::Zephyr.kebab()` to `"zephyr-image"` fails it, naming the path it
looked for.

### One fact, two namespaces

"Which boards honor per-entry sizing" is now
`BoardFamily::honors_entry_sizing` (`nros-entry-lower`), routed to from both
`executor_sizing::board_honors_entry_sizing` (the entry board KEYS, which
`nros::main!` reads - unchanged, `matches!(k, "native" | "posix")` is exactly
`board_family(k) == Some(Native)` over `BOARD_KEYS`) and
`PlatformKind::honors_entry_sizing` (the catalog's platform KINDS, which this
bake reads). `nros-orchestration-ir` does not depend on `nros-entry-lower`, so
nothing in either crate can hold the two together;
`the_two_namespaces_agree_about_who_honors_entry_sizing` does it from
`nros-cli-core`, the one crate that sees both, over every row of `BOARD_KEYS`
and every `PlatformKind`.

### What the fix subtracts, deliberately

A target naming NO block resolves no board, and the host is the documented
default for every other question such a bake answers (the tier tables
included - that is issue 1312's subject). So it is the default here too, and a
bake that used to get a firmware verdict from a target string that merely
LOOKED like a platform now gets the host's. Exactly one caller is in that
position: the NuttX shim's `--target nuttx`
(`integrations/nuttx/apps-external-template/Makefile`), which 1312 left in
place on purpose and whose bake ALREADY reads the host's tier tables. The
capacity check no longer disagrees with the tier resolver about what that image
is; the remaining gap is one gap, where 1312 filed it, and pinning the image id
closes it for both at once. (`integrations/**` is owned by another session
right now and is untouched here.)

### The 0257 regression test was resting on the same coincidence

`tests/executor_sizing_bake_gate.rs` bakes a fixture whose comment says
*"`deploy` targets a firmware board"* - and passed `--target freertos`, which
names no block in that bringup. Nothing ever reached the deploy: the firmware
verdict came from `board_honors_entry_sizing("freertos")`, i.e. from the string.
(Its `[deploy.qemu_freertos] board = "mps2-an385"` could not have resolved
either - no descriptor claims that id; the FreeRTOS board there answers to
`freertos`.) The target now names the block, the board is one the catalog
resolves, and `--nano-ros-path` is supplied because resolving a board loads the
catalog (issue 1263's ladder, which a `/tmp` workspace's walk-up cannot
answer). All three tests still pass, and the over-capacity one now fails when
`honors_entry_sizing` is mutated - it did not depend on it before.
