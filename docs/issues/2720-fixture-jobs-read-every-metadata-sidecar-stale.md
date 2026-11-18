---
id: 2720
title: "In the nightly's fixture jobs every metadata sidecar reads stale, so ~170 images there are resolved without the probe's counts"
status: open
type: bug
severity: medium
area: [sizing, ci, testing]
related: [issue-2520, issue-2721, issue-1694, issue-1018]
found: 2026-11-19
---

**Home:** sizing / CI. Found while measuring issue 2520.

## What

In the fixture builds of 2026-11-18 (nightly 44242045728, run-matrix
44250365361) about **170 resolved images** printed "entity counts not
derived; configured defaults kept", and every metadata sidecar the resolve
read was reported "stale against its sources (re-run `nros configure`), so
neither its counts nor its registration observations are used".

## What it is not (corrected 2026-11-19)

This issue was first filed as "the example workspaces declare almost no
contracts". That is wrong, measured: on a fresh worktree of main, with no
contract added, a forced `nros configure` of each canonical workspace
derives every image and keeps defaults for none:

| workspace | resolves derived | defaults kept |
| --- | --- | --- |
| `examples/workspaces/rust` | 89 | 0 |
| `examples/workspaces/c` | 87 | 0 |
| `examples/workspaces/cpp` | 95 | 0 |
| `examples/workspaces/mixed` | 11 | 0 |

The counts come from the metadata probe's per-component sidecars
(`src/<pkg>/metadata/*.json`, issue 1694's composition), not from contracts.
A contract adds topology and QoS on top. The sidecars are gitignored build
output written by `nros configure`, so in CI they were written earlier in the same
job and then judged stale by a later reader.

## Why they read stale is not known yet

The freshness key is the probe's inputs: the component's sources, the CLI's
planning stamp (baked into the binary at build time), the nano-ros crates the
probe compiles (resolved from a nano-ros checkout; with none resolved the key
cannot be computed and the sidecar is never fresh), the probe's pinned knobs,
and the workspace packages the component depends on. Candidates: two CLI
binaries with different planning stamps in one job (a cached
`packages/cli/target` and a rebuilt one), or a reader that resolves no
nano-ros checkout.

For C/C++ components the probe also never re-ran, which is issue 2721.

## Next step

The reader now names the input that moved, with the recorded and current
values ("Moved: CLI planning stamp: recorded `…`, now `…`", or why the current
key cannot be computed). The next nightly's log answers which input it is;
the fix follows from that.

## The verdict is misclassified too

Measured on 2026-11-19 with issue 2520's resolve. Every sidecar of
`examples/workspaces/cpp` was made stale (its recorded digest altered),
which is the CI state. Then `NROS_PROBE_FAILURE=refuse nros build
pubsub_bringup:linux_talker --dry-run` exited 0. The image was classified
"nothing declared to derive sizes from … State what each node creates … in
the contract sidecar". The components ARE declared: their sidecars exist and
are stale. So the note is false, and it advises writing a contract, which
fixes nothing: do not follow it for this case (we did, and drafted
contracts that changed nothing). The honest verdict is a failure ("the probe's counts for N
components are stale", naming the moved input that the stale-sidecar
diagnostic reports), which refuses under `refuse`.

Reclassifying now would turn the nightly red on every one of the ~170 images
at once, and a uniformly red lane hides any real regression. So the
reclassification lands with the root-cause fix, not before it.
