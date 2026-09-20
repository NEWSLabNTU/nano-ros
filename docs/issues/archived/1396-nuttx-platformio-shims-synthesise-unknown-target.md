---
id: 1396
title: "The NuttX and PlatformIO build shims synthesise a `--target` string that
  names no `[image.*]` block, so every answer it selects falls back to the host
  defaults — silently, including the tier tables"
status: resolved
type: bug
area: [cli, integrations]
severity: low
found: 2026-09-20
resolved_in: "fix(#1396): the last two shims that named a block nobody declares"
related: [issue-1312, issue-1285]
---

## What happens

`nros codegen-system --target <id>` names an `[image.<id>]` / `[deploy.<id>]`
BLOCK in the bringup's `system.toml`. Issue 1312 measured what a string that
names no block does: `tier_resolver::target_board_id` finds nothing,
`derive_target_rtos` returns `BoardFamily::Native.tier_rtos_key()`, and the bake
reads `[tiers.*.posix]` — the HOST — for an image that is not the host. The same
`None` also reaches `resolved_domain_id` / `resolved_rmw` / `resolved_locator`
and the per-image launch selection, each of which then takes the `[system]`
default.

1312 fixed the Zephyr and ESP-IDF shims by giving `codegen-system` a
`--for-entry <PKG|dir>` flag — the question a framework configure can actually
answer — and left two shims that still synthesise a `--target`:

| shim | line | passes |
| --- | --- | --- |
| NuttX | `integrations/nuttx/apps-external-template/Makefile:51` | `--target nuttx` |
| PlatformIO | `integrations/platformio/nros_codegen.py:54` | `--target platformio` |

Neither carries a tracked issue id in its comment, which is what CLAUDE.md
requires of a tolerated gap and why 1312 could add no gate for the class.

## What is measured, per shim

### NuttX — the string is wrong where it is used and right where it is not

The template's `context::` rule passes `--target nuttx` unconditionally. The
staging script (`scripts/nuttx/stage-external-apps.sh --bringup <dir>`) already
writes `nros_bringup.mk` pinning `NROS_BRINGUP_NAME` and
`NROS_BRINGUP_WORKSPACE` next to it, so the template stays bringup-agnostic —
but it pins no image, and the Makefile has no other way to know one.

The only in-tree caller that stages a bringup through this template is
`packages/testing/nros-tests/tests/cli_bringup_nuttx.rs`, with the fixture
`packages/testing/nros-tests/fixtures/multi_pkg_workspace_nuttx/src/demo_bringup`.
That bringup declares exactly one image, and it is **not** `nuttx`:

    [image.qemu-armv7a-nuttx]
    board = "qemu-armv7a-nsh"

So for the one bringup this shim bakes, `nuttx` names no block and the bake
takes the host defaults.

**1312's sweep was narrower than its claim, and this is the correction.** It
reported that "NO in-tree bringup declares a block by any of those names",
having looked only under `packages/testing/nros-tests/fixtures`. Six
example-workspace bringups DO declare `[image.nuttx]`:

    examples/workspaces/c/src/demo_bringup/system.toml:140
    examples/workspaces/realtime-c/src/demo_bringup/system.toml:108
    examples/workspaces/realtime-c/src/smp_bringup/system.toml:128
    examples/workspaces/realtime-rust/src/demo_bringup/system.toml:167
    examples/workspaces/realtime-cpp/src/demo_bringup/system.toml:170
    examples/workspaces/rust/src/demo_bringup/system.toml:160

Nothing stages any of those six through this Makefile — the two other
`stage-external-apps.sh` callers (`scripts/nuttx/build-nuttx.sh:298`,
`just/nuttx.just:598`) pass no `--bringup` at all. So the literal is correct
exactly where it is never exercised and wrong exactly where it is, and a reader
who checks "does `nuttx` name a block?" is told yes. That is worse than being
uniformly wrong: two of those six (`realtime-c`, `realtime-rust`) declare
`[tiers.*]`, so the string reads as a working example of the thing it breaks.

The second reading of `--target` is unaffected by moving it.
`executor_sizing::board_honors_entry_sizing` matches `"native" | "posix"` only,
so `"nuttx"` and `"qemu-armv7a-nuttx"` are both `false`; no platform or board
TOML in the tree declares `max_cbs`, so the ladder answers `None` for either.
(That second reading is issue 1397's; this issue does not change it.)

### PlatformIO — the invocation cannot reach the target string at all

`integrations/platformio/nros_codegen.py:52-55` builds:

    [nros, "codegen-system", "--ahead-of-vendor",
     "--workspace", workspace, "--bringup", bringup,
     "--target", "platformio", "--framework", _framework(),
     "--out", out_dir]

Three defects, in the order clap hits them. The first two are MEASURED against
the in-tree CLI, not read off the source:

1. `--ahead-of-vendor` is `#[arg(long = "ahead-of-vendor", value_enum)]` over
   `Option<AheadOfVendor>` (`Pio` | `Px4`) — it REQUIRES a value, and passed
   bare it is rejected before anything else is looked at:

       $ nros codegen-system --ahead-of-vendor --workspace /tmp/ws \
             --bringup demo_bringup --target platformio --framework zephyr --out /tmp/out
       error: a value is required for '--ahead-of-vendor <AHEAD_OF_VENDOR>' but none was supplied
         [possible values: pio, px4]

2. `--framework` is not defined on the verb. `codegen_system::Args` has no such
   field (the word appears only in doc comments and a `TODO(E.3)`). Supplying
   the missing enum value reaches it:

       error: unexpected argument '--framework' found
         tip: a similar argument exists: '--rmw'

3. `--target platformio` names no block: no `system.toml` in the tree declares
   `[image.platformio]` or `[deploy.platformio]` (the only in-tree occurrence of
   that spelling is an inline TOML literal inside
   `cargo_metadata_schema.rs::accepts_platformio_framework_field`, a unit-test
   fixture string).

And the failure is swallowed:

    except (FileNotFoundError, subprocess.CalledProcessError) as e:
        sys.stderr.write("[nros] codegen-system failed: %s (continuing — verb may not yet exist)\n" % e)

`codegen-system` has existed since Phase 212.E, so the excuse in that message is
stale; what the handler actually hides now is an argument-parse error in the
shim's own command line. The script is reachable — `library.json` names it as
the shipped `build.extraScript` — so this is broken code, not unreachable code.

## Why it is latent

No merge-gating lane builds a NuttX app, and there is no PlatformIO lane at all
(`#186` deleted the PlatformIO smoke; `build-test-fixtures` has zero references
to it). Both paths therefore fail, or answer wrongly, only on a user's machine.

## Fix

- **NuttX** — `nros_bringup.mk` already pins the bringup and the workspace; pin
  the IMAGE id there too and pass it as `--target`, which is 1312's own
  suggestion. The staging script is the one place that has both the image id
  (from its caller) and the `system.toml` to validate it against.
- **PlatformIO** — repair the invocation or delete it. Repairing means
  `--ahead-of-vendor pio`, dropping `--framework`, and asking the ESP-IDF
  shim's question (`--for-entry <project dir>`) instead of naming a block the
  shim cannot know.

## Acceptance

The staged `nros_bringup.mk` carries an image id the bringup declares, and the
codegen invocation the template builds from it resolves that block rather than
falling through to the host. A gate refuses a new synthesised `--target`
literal in an in-tree shim unless a bringup declares it or the line carries a
tracked issue id.

## Resolution

**Both shims were fixed; neither needed a new spelling.**

### NuttX — the staging script pins the image, and CHECKS it

`stage-external-apps.sh` gained `--image <id>`, written into the same
`nros_bringup.mk` that already pins the bringup and the workspace, and the
template's `context::` rule passes it:

    NROS_CODEGEN_TARGET_ARG = $(if $(NROS_BRINGUP_IMAGE),--target $(NROS_BRINGUP_IMAGE))

This is 1312's own suggestion, using the existing `--target` vocabulary. It is
NOT `--for-entry`: the application lives in `apps/external/<bringup>/`, outside
the workspace, so no `[image.*]` can claim it and the resolver would answer
`None`. No RTOS name is read out of any string (issue 1285's substring guess,
removed by #938) — the id is a block key, checked against the bringup's own
`system.toml`, and the staging script is the one place that holds both.

The check matters as much as the pin, because the failure being fixed is
SILENT: `--target` takes an unknown id without complaint. A typo would buy back
exactly the bug. Unpinned stays legal and is no worse than before — the rule
then passes no `--target`, prints `NO IMAGE PINNED`, and the CLI takes the
documented defaults.

Measured, two bakes of one workspace differing only in the `--target` string.
The workspace is the shape of 1312's own
`write_zephyr_tiered_workspace` retargeted at NuttX — two node packages with
callback groups, `[tiers.high|low]` carrying both a `posix` and a `nuttx`
sub-table, one `[image.qemu-armv7a-nuttx]`, and a committed
`config/system_model.yaml` so no `nros sync` is needed:

| `--target` | baked tier priorities |
| --- | --- |
| `nuttx` (the retired synthesised string) | high **80**, low **10** — the `posix` sub-table, i.e. the HOST |
| `qemu-armv7a-nuttx` (the pinned image id) | high **7**, low **9** — the `nuttx` sub-table |

Verbatim:

    === --target nuttx ===
      plan target : nuttx
      tiers       : [{"name": "high", "priority": 80, "spin_period_us": 1000},
                     {"name": "low",  "priority": 10}]
    === --target qemu-armv7a-nuttx ===
      plan target : qemu-armv7a-nuttx
      tiers       : [{"name": "low",  "priority": 9},
                     {"name": "high", "priority": 7, "spin_period_us": 1000}]

Bound by two tests in `cli_bringup_nuttx.rs` that need neither NuttX nor `nros`:
`staged_bringup_pins_the_image_the_codegen_target_names` (the pin is written AND
the template spells it in the `--target` position — a pin nothing reads is what
`--target nuttx` already was) and `staging_refuses_an_image_the_bringup_does_not
_declare`. The first reads the id OFF the fixture and asserts it is not `nuttx`,
so if the fixture is ever renamed after the platform the test says so instead of
passing by luck.

### The correction to 1312's sweep

`[image.nuttx]` DOES exist — in six example-workspace bringups (listed above);
1312's sweep looked only under `packages/testing/nros-tests/fixtures`. Nothing
stages any of the six through this template, so the literal was right exactly
where it was never used. Recorded because a reader checking the claim by
grepping would have been told the string was fine.

Moving the string does not disturb `--target`'s second reading (issue 1397):
`board_honors_entry_sizing` matches `"native" | "posix"` only, and no TOML in
the tree declares `max_cbs`, so `nuttx` and `qemu-armv7a-nuttx` both answer
`false` / `None`.

### PlatformIO — repaired, not deleted

Deleting was the other candidate and was rejected: `library.json` ships this
file as `build.extraScript`, so it is reachable by a downstream user. It is
broken code, not unreachable code, and removing it would silently withdraw a
documented integration rather than fix it. All three defects were repaired:
`--ahead-of-vendor pio`, `--framework` dropped, and `--for-entry <PROJECT_DIR>`
in place of `--target platformio` — the same question the ESP-IDF shim asks of
its IDF project dir, degrading to the documented defaults with a printed note
when no image claims the entry.

The `except` that swallowed the failure also went. Its message said "continuing
— verb may not yet exist", which stopped being true when Phase 212.E shipped
`codegen-system`; what it hid afterwards was this script's own broken argv.

Measured: the repaired argv runs to completion and writes the bake tree plus
the `--ahead-of-vendor pio` artifacts (`Cargo.toml`, `nros-plan.json`,
`system_config.h`, `system_config.cmake`, `vendor_hint.json`).

**What is NOT fixed, and is not this issue's:** the bake the hook now reaches
still prints its own warning —

    nros codegen system: WARNING — PlatformIO output is a SKELETON (H.6 pending):
    library.json carries no transport/framework selection and no extra_script.py
    is emitted. Manual PIO project wiring is required before this bake is usable.

so the hook runs, and the integration behind it is still incomplete. There is
no PlatformIO lane (`#186` deleted the smoke), so the hook itself is verified
only at the CLI-invocation level: the argv was run by hand, the PIO hook was
not.

### The gate

`check-shim-codegen-target` (`just check shim-codegen-target`, fast line,
buildless). **A `codegen-system --target` in a tracked build file may not be a
LITERAL at all.** A shim serves whatever bringup it is pointed at, so it cannot
know that bringup's block key: it passes a VARIABLE and whatever ASSIGNS the
variable does the checking, or it asks `--for-entry`. A variable value is out
of scope by construction — the gate cannot evaluate it, and `stage-external
-apps.sh --image` is the assigning producer's check, which is what this fix
added.

The one way out is an authored marker on the invocation naming the VALUE and a
tracked issue id, which is CLAUDE.md's rule for a tolerated gap and precisely
what these two shims lacked:

    # nros-shim-target-gap: platformio (issue 1396) — no PIO lane exists

**Two weaker rules were written first and both are measured wrong.** They are
recorded because each looked obviously right:

* *"a literal must name a block SOME bringup declares"* — `nuttx` IS such a
  block (the six example workspaces above), so re-injecting `--target nuttx`
  into the template left the gate **green**. The gate could not catch the bug
  it was written for. A literal that names a block is now reported with a
  SHARPER note than one that names none: it is the dangerous case, because it
  reads as correct to anyone who greps.
* *"a tracked issue id within six lines excuses it"* — the template's comment
  explains this very fix and cites issue 1396, so the same re-injection was
  **excused by the prose describing its repair**. Proximity cannot distinguish
  "this gap is tolerated" from "here is what we fixed"; hence the marker.

Scope is drawn by file KIND (the four build-file names plus the build-language
suffixes; Rust through `build.rs` alone, `Kconfig` excluded) because an
ordinary `.rs` and a `Kconfig` `help` block both quote whole invocations as
prose and neither runs anything. The gate excludes its own file by exact path,
and says so in a self-test: its fixtures are the literals it refuses. That
exclusion is also the only finding it has ever made on the real tree — while
the script was untracked `git ls-files` could not see it, so the gate read
green, and the first run after the commit reported seven findings in itself.

The `--self-test` (40 checks, run on the NORMAL path, not only behind the flag)
carries both real pre-fix shim bodies as positive controls, including the
argv-list spelling `"--target", "platformio"` that a whitespace-only pattern
misses — which the first draft of the scanner did.

Three live mutations, each reverted:

| mutation | result |
| --- | --- |
| template back to `--target nuttx` | gate FAILS, `staged_bringup_pins_…` FAILS |
| staging stops writing `NROS_BRINGUP_IMAGE` | `staged_bringup_pins_…` FAILS |
| staging stops validating the id | `staging_refuses_…` FAILS |
