---
id: 1312
title: "`nros_system_generate.cmake` passes `--target zephyr-<rmw>`, which names no
  deploy block, so the tier resolver answers for the host, not Zephyr"
status: resolved
type: bug
area: zephyr, cli, codegen
severity: low
resolved_in: "fix(#1312): a build shim names its ENTRY, and the image that claims it answers"
related: [issue-1285, issue-1263]
---

## What happens

`zephyr/cmake/nros_system_generate.cmake` calls `nros codegen-system` with
`--target "zephyr-${_rmw}"` (line 182; the comment at line 145 says it maps the
Kconfig RMW "to a --target string the CLI understands"). The tier resolver
(`tier_resolver::derive_target_rtos`) uses `--target` to pick a `[deploy.*]`
block and then reads that block's board id. `zephyr-zenoh` and its siblings
name no block, so no board id is resolved and the resolver falls back to the
HOST default. A Zephyr image baked through this module therefore gets
host-flavoured tier values rather than `[tiers.*.zephyr]` ones.

## Why it is latent

The two bringups the module bakes today declare no tiers, so no image output
changes. It becomes wrong the first time a tiered bringup is baked through this
path.

## History

Before the issue-1285 follow-up (PR #938), the resolver matched the RTOS by
substring. That PR made the resolver read board ids from the board catalog, and
it recorded this case as out of scope rather than reading "zephyr" out of the
`--target` string, which would reintroduce the substring guess.

## Fix

Have the module pass the IMAGE's identity. That is either the image/deploy-block
name the bringup's `system.toml` declares for this Zephyr image, or the board id
directly, if `codegen-system` grows a flag for it. Then the resolver answers
from the catalog: `native_sim/native/64` → zephyr, the mps2/an385 Zephyr boards →
zephyr. Check how the non-Zephyr callers (`nros build` → `codegen-system`) pass
it, and use the same spelling.

## Acceptance

A test bakes a bringup with a `[tiers]` table through the Zephyr module path,
or through its CLI invocation verbatim, and the generated tier code carries the
`[tiers.*.zephyr]` values. The test must fail with today's `--target
zephyr-<rmw>`.

## Resolution

**`codegen-system` grew `--for-entry`, and the shims name their entry package
instead of synthesising a target.** `--target` names a BLOCK, and a framework
configure is the one caller that cannot know which block: a west or ESP-IDF
configure knows the application directory it was pointed at and nothing above
it.
So the question it can answer is the one `nros image-facts --for-entry` and
`nros ws board-facts` already ask — *which image claims this entry* — and
`nros_orchestration_ir::leaf_system::for_entry` is the single resolver all
three now go through. No substring of any target string is read, which is what
the 1285 follow-up removed.

Measured, on the module's own invocation
(`codegen_system::tests::a_zephyr_image_named_by_its_entry_bakes_the_zephyr_tier_table`):
one workspace, two bakes, tiers carrying `posix` 80/10 and `zephyr` 7/9.

| bake | `--target` / `--for-entry` | baked priorities |
| --- | --- | --- |
| the module, before | `--target zephyr-zenoh` | 80 / 10 (**the host**) |
| the module, now | `--for-entry <app dir>` | 7 / 9 |

The negative control is in the test body, not in prose: the legacy flag's half
asserts 80/10, so if that ever stops being true the other half stops being
evidence. Mutating `resolve_target_block` to ignore `for_entry` fails it with
`the zephyr sub-tables (7/9) must be the baked priorities`.

What else `zephyr-${_rmw}` was doing: **nothing**. It reached `nros-plan.json`'s
`"target"` field (no reader), `resolved_domain_id` / `resolved_rmw` /
`resolved_locator` and the per-image launch selection — each of which, naming no
block, fell through to the `[system]` default — and `check_executor_capacity`,
which reads it as a PLATFORM name (see below) and got `None` for
`zephyr-zenoh` and gets `None` for an image id too, so that check is unchanged.
`_rmw` itself is still live and still Kconfig's: it selects the backend's user
config (`nros_bake_rmw_user_config`) and is now also passed as `--rmw`, so the
baked `NROS_SYSTEM_RMW` matches what Kconfig compiled instead of re-deciding
from `system.toml` — the disagreement `NanoRosImageAgreement.cmake` exists to
catch, closed at the source for this one bake.

`--nano-ros-path` landed with it, for issue 1263's reason. Naming the image is
the first thing that makes one of these bakes resolve a board id, and resolving
one loads the board catalog: the CLI's ladder is `$NROS_REPO_DIR` → a walk up
from the workspace → a released toolchain's `share/nano-ros`, and the middle
rung finds nothing for a downstream project whose nano-ros sits below it. A
build shim always knows where its own module tree is, so both shims say (the
Zephyr module captures it at FILE scope — inside the function
`CMAKE_CURRENT_LIST_DIR` names the caller, and `NROS_REPO_DIR` is set after the
include). Without this the fix would have traded a silent wrong answer for a
`FATAL_ERROR` on some downstream configures.

### The sweep, and the two siblings left

    rg -n --no-heading -A8 'codegen-system' --glob '!docs/**' --glob '!book/**' . \
      | rg -- '--target' | grep -v target-dir

Four shims synthesise a `--target`, and NO in-tree bringup declares a block by
any of those names (`rg -n '^\[image|^\[deploy' $(find packages/testing/nros-tests/fixtures -name system.toml)`):

* **Zephyr** `--target zephyr-<rmw>` → **fixed** (`--for-entry
  ${APPLICATION_SOURCE_DIR}`). `multi_pkg_workspace_zephyr`'s image gains
  `entry = "zephyr_app"` and `zephyr_self_pkg/sibling`'s gains `entry =
  "caller"`; the `self/` variant needs none — the application IS the package,
  so its own `system.toml` answers first.
* **ESP-IDF** `--target "esp-idf"` → **fixed** the same way
  (`--for-entry ${CMAKE_SOURCE_DIR}`, the IDF project dir, which is the entry
  package `esp_idf_app/`; the fixture's image claims it). Not built in this
  worktree — but the change cannot subtract an answer: an entry no image claims
  degrades to exactly today's defaults, with a printed note.
* **NuttX** `--target nuttx` (`integrations/nuttx/apps-external-template/
  Makefile`) → **left, deliberately.** Its application lives in the NuttX apps
  tree (`apps/external/<bringup>`), not in the nano-ros workspace, so no
  `[image.*]` can claim it and `--for-entry` would resolve nothing. The fix
  there is for the staging fragment (`nros_bringup.mk`, which already pins the
  bringup and workspace) to pin the IMAGE id and pass it as `--target` — a
  change to the staging script plus the template, with no merge-gating lane that
  builds a NuttX app to accept it. `nuttx` IS a platform name, so this one
  string is also the only synthesised target that means something to
  `check_executor_capacity`; moving it needs that read looked at too.
* **PlatformIO** `--target platformio` (`integrations/platformio/
  nros_codegen.py`) → **left**: that invocation cannot run at all today. It
  passes `--ahead-of-vendor` with no value and a `--framework` flag
  `codegen-system` does not define, so the call fails argument parsing and the
  script swallows it ("continuing — verb may not yet exist"). Its target string
  is the least of its problems and fixing it would be pretending the path runs.

### Adjacent finding, not fixed

`--target` has a THIRD reading nobody declared: `check_executor_capacity` passes
it as `deploy_key` into `resolve_max_cbs_through_ladder(platform)` and
`board_honors_entry_sizing`, i.e. as a PLATFORM name. That is true of `native` /
`posix` only because an image is conventionally named after them; for every
other image id the ladder silently answers `None` and the capacity check falls
back to the built-in default. No platform declares `max_cbs` today, so the two
paths currently end at the same number and nothing is measurably wrong — which
is exactly how long a second reader stays invisible (`check-knob-single-reader`
is the gate for this class).
