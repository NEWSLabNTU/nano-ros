---
id: 2615
title: "A model whose launch loads a param file from a sibling package is refused as non-portable"
status: resolved
type: bug
area: [cli, launch]
severity: high
found: 2026-11-15
resolved: 2026-11-15
resolved_in: "the change that filed it"
related: [phase-478, issue-2598, issue-0320]
---

**Home:** phase-478 (I17). Found by the NXP island on main 31846e9a7c. It blocked
the nros11 release.

## Symptom

The island stopped building. Every consumer of its model refused it:

```
non-portable absolute input path `…/src/autoware_mrm_comfortable_stop_operator/config/mrm_comfortable_stop_operator.param.yaml`
```

## Cause

Issue 2598 made a `<param from>` file a model input (play_launch 191d949f,
pinned by dd571ca75f). The resolver records an input relative to the bringup
root when the input lies under that root, and as an absolute path when it does
not. A param file in a sibling package has no relative spelling, so it is
recorded absolute.

`model_gate::provenance_stale_with` refused any absolute input path. That rule
dates from issue 0320, when committed models carried machine-specific absolute
paths for files inside the bringup. A current resolver never writes those. So
the rule could not tell a legacy model from a model with a sibling input, and
re-resolving could not help, because the resolver writes the same path again.
Sibling includes had the same latent problem; the param files made it common.

## Fix

Only an absolute path under the bringup or its launch root is the legacy shape,
so only that one is stale on sight. An absolute path outside it is read and
hash-checked like any other input. It is current while it matches and stale
once it changes.

Test: `model_gate::tests::absolute_sibling_input_is_checked_by_hash`. The
legacy rule keeps `absolute_path_is_stale_even_when_file_matches`.
