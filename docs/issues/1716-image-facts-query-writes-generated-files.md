---
id: 1716
title: "`nros image-facts` is documented to produce no artifacts, and rewrites every image's generated west application as a side effect of the query"
status: open
type: bug
severity: low
area: [cli, build, zephyr]
related: [1707, rfc-0065]
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
