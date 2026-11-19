---
id: 2720
title: "In the nightly's fixture jobs every metadata sidecar reads stale, so ~170 images there are resolved without the probe's counts"
status: resolved
resolved_in: "PR #4190 (9afb5c8a19), issue 2689; confirmed by nightly 44463027311"
type: bug
severity: medium
area: [sizing, ci, testing]
related: [issue-2520, issue-2689, issue-2721, issue-2734, issue-1694, issue-1018]
found: 2026-11-19
---

**Home:** sizing / CI. Found while measuring issue 2520.

## What

In the fixture builds of 2026-11-18 (nightly 44242045728, run-matrix
44250365361) about **170 resolved images** printed "entity counts not
derived; configured defaults kept", and every metadata sidecar the resolve
read was reported "stale against its sources (re-run `nros configure`), so
neither its counts nor its registration observations are used".

## Root cause: issue 2689

The CLI's typed sidecar mirror, `SourceMetadata`, is
`#[serde(deny_unknown_fields)]`. Issue 2650 moved the probe recorder to
schema v6 (two new top-level keys) without touching the reader, so:

1. `stamp_provenance` failed to parse the sidecar the probe had just written;
2. the C/C++ refresh called it as `let _ = stamp_provenance(..)`, so the
   failure vanished and the sidecar stayed unstamped;
3. `sidecar_is_fresh` parses through the same struct and answers `false` on
   any parse error, so every sidecar was stale on every read.

PR #4190 (merged 2026-11-18 06:52Z) fixed all three and added a golden
sidecar written by the real producer and read strictly by the CLI. Nightly
44242045728 started at 05:07Z that day, before the fix; it is the run this
issue was measured on.

On a fresh worktree the canonical workspaces derived every image (rust 89,
c 87, cpp 95, mixed 11; none kept defaults), because a local run writes and
reads its sidecars with one CLI after the fix. That ruled out the first
reading, "the examples declare no contracts": the counts come from the
probe's sidecars (issue 1694's composition), not from contracts.

## Confirmed

Nightly 44463027311 (2026-11-19, head `0a4973731d`, after #4190): the whole
run log has **0** "stale against its sources" lines. Its 85 fixture-job
"configured defaults kept" lines all carry a NothingDeclared reason: 73 are
owned entries with no launch (issue 2482's shape), and 12 are launch trees
that declare neither wiring nor parameters. None is a stale sidecar.

The stale-input attribution added while this was open
(`stale_key_attribution`, which names the probe input that moved, with the
recorded and current values) stays: it is what a future stale report
will be diagnosed from.

## Sweep: discarded results on the freshness path

`git grep -nE '^\s*let _ = [a-z_:]*(stamp|provenance|fresh|sidecar|write|refresh|probe)[a-z_]*\(' -- 'packages/cli/nros-cli-core/src/**.rs'`

| site | discards | verdict |
| --- | --- | --- |
| `builder/cmake_configure.rs` `ConfigureStamp::record` | the stamp write | safe: a missing stamp makes the next run configure again |
| `orchestration/metadata_refresh.rs` unprobeable marker | the marker write | safe: a missing marker makes the next sync re-probe |

No `let _ =` on `stamp_provenance` or `sidecar_is_fresh` remains. Both
discards fail toward re-doing the work, never toward trusting stale output,
so neither is 2689's shape.

## Moved out

The stale verdict was also misclassified: a stale sidecar reads as "nothing
declared", which is a note, so under `NROS_PROBE_FAILURE=refuse` it does not
refuse. That is now issue 2734. It could not land while every sidecar read
stale, because it would have turned ~170 images red at once.
