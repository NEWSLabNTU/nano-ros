---
id: 1716
title: "`nros image-facts` is documented to produce no artifacts, and rewrites every image's generated west application as a side effect of the query"
status: resolved
type: bug
severity: low
area: [cli, build, zephyr]
related: [1707, rfc-0065]
resolved_in: "branch issue-1716-image-facts-no-write"
found: 2026-10-06
---

## What

`packages/cli/nros-cli-core/src/cmd/image_facts.rs` says, under "WHAT IT
DELIBERATELY DOES NOT DO": *"It produces no artifacts."* It answers a query by
calling `crate::cmd::build::plan_builds` — the same planner `nros build` uses —
and planning a workspace runs the entry generators, which WRITE each image's
generated files, including the generated west application, for every image of
the workspace, not only the one asked about.

## Why it mattered once, and why it is low now

Issue 1707 (PR #1721) measured the consequence: the Zephyr configure calls
`nros image-facts --for-entry` from `nros_check_image_agreement`, so the query
rewrote the very west application cmake had just read, after it read it.
`build.ninja` was then written newer than its input and ninja never
reconfigured — a capability change landed one lane run late.

PR #1721 made that harmless rather than impossible: the generated application
states its own digest, and a configure whose executed digest differs from the
file on disk now refuses, so the next build reconfigures from the current file.
For a sibling image the side effect is an ordinary early regeneration.

So nothing is known to be broken today. What remains is a query with a hidden
write, documented as having none — the shape that produced 1707 and that the
next caller of `image-facts` (a gate, a test, an editor integration) will
assume away.

## Fix direction (not decided)

Either make the query side-effect free — a no-write mode threaded through
`plan_builds` / `generate_entry` and every writer they reach — or correct the
module docs to say the query regenerates the workspace's entries, and name it
in the canonical build path's wiring table. PR #1721's issue doc judged the
first not needed for correctness; it is still the honest end state for a verb
named as a query.

## Acceptance

Running `nros image-facts --for-entry <e>` twice over a planned workspace
leaves every generated file's bytes AND mtime unchanged (or the docs state the
write and its scope), with a test that fails against the current behaviour.

## Resolution

The query is side-effect free. The docs were not just softened to match the old behaviour.

**Why `--dry-run` could not be reused.** `nros build --dry-run` writes files too. It plans through the same `plan_builds`, and stage 4 generates during a dry run. That is load-bearing: PR #1721's Zephyr fixture runner calls `nros build <image> --dry-run` on the ninja path *in order to* regenerate the west application without building it (`zephyr_fixture_app_regen.sh` R1). Making dry-run write nothing would have brought issue 1707 back. So there are two real modes: "generate, then print instead of run" and "write nothing". They are named on one private enum, `cmd::build::Planning { Generate, Query }`, which is not a second spelling of `dry_run`.

**What changed.**
* `plan_builds(args)` is now `plan_for(args, Planning::Generate)`, so its behaviour is unchanged. The new `plan_query(args)` is `plan_for(args, Planning::Query)`. A query runs stages 1–3: discovery, image and board resolution, driver choice, preflight, the output-collision refusal. It stops before stage 3.5 (`resolve_image` writes `resolved.toml` and the model), and before stage 4 (the entry package, settings file, west application, sizing descriptor, and retiring a stale generated root). Every field `image-facts` prints is set by then: qualified, board, platform, driver, rmw, entry package, triple, profile. The query plan carries `handoff: None` and `configure: None`.
* The single-package leaf road returns before `leaf_settings::write`. The bringup-less road returns no plans instead of writing each package's cmake root, because a workspace with no bringup declares no image.
* `image_facts.rs` calls `plan_query`, and its module doc now states the no-write property and names the test that pins it.
* Doc corrections. `plan_builds` was documented as having "NO side effects", and `drive`'s and `ResolvedBuild::configure`'s comments repeated that. All now say what is true: planning spawns no tool, but it does write generated files. The `--dry-run` help says that it still generates and why. `NanoRosImageAgreement.cmake` and `west_app::with_digest` no longer name image-facts as a rewriter. The 1707 digest check stays, because a concurrent `nros build` of a sibling image can still rewrite the application mid-configure.

**Test.** `build_verb_pipeline.rs::image_facts_writes_nothing` plans a workspace with a cargo image and a west image, makes the generated `main.rs` stale, and snapshots every file's bytes and mtime. It then runs `image_facts::run --for-entry native_entry --cmake` twice and requires the snapshot to be identical. It also requires `plan_query` to give the same identity facts as `plan_builds`, with no handoff, and to write nothing itself. With `image_facts` switched back to `plan_builds` the test fails: ``wrote to the workspace … ["build/posix-zenoh/native_entry/src/main.rs"]``. With the fix it passes, and all 37 tests in the file pass.

**Measured on a real workspace** (`examples/workspaces/features`, 26 images planned with `nros build --all --dry-run`, the generated `zephyr_rust_qos_entry/CMakeLists.txt` made stale, `image-facts --for-entry zephyr_rust_qos_entry --cmake` run twice, 7,402 snapshot lines covering path, mtime and sha1 of every file under `build/`):

| CLI | result |
| --- | --- |
| before (query via `plan_builds`) | the west application was rewritten (sha1 `596d318f…` → `02cf3191…`, mtime moved), and stderr shows `west application →` / `resolved →` for every image of the workspace |
| after (`plan_query`) | **UNCHANGED**, the same `set(NROS_IMAGE_*)` answer, nothing on stderr |

**1707 path still works**: `just check zephyr-app-regen` passes, all 6 cases (R1 ninja-path regeneration through `--dry-run`, R2/R2b, C1–C3 digest check).

Behaviour change, deliberate: an error that only stage 4 could raise no longer fails the query. Examples are a census mismatch (which never fired under `--dry-run` anyway) or an entry that cannot be generated. The query describes the declared image; building it still reports those errors.
