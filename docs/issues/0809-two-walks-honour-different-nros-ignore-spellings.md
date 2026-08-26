---
id: 809
title: "`provider_scan` honours `NROS_IGNORE` while `nros-pkg-index` honours
  `.nros-ignore` — the only spelling that exists on disk is the one the
  order-walk ignores"
status: open
type: bug
area: build
related: [issue-0621, phase-383, rfc-0065]
---

## Problem

Two workspace walks disagree about how a directory opts out of discovery.

`packages/cli/nros-pkg-index/src/lib.rs:46`:

```rust
const COLCON_IGNORE_MARKER: &str = "COLCON_IGNORE";
const NROS_IGNORE_MARKER: &str = ".nros-ignore";
```

`packages/cli/cargo-nano-ros/src/provider_scan.rs:95`:

```rust
const IGNORE_MARKERS: &[&str] = &["COLCON_IGNORE", "AMENT_IGNORE", "NROS_IGNORE"];
```

`.nros-ignore` versus `NROS_IGNORE` — a dot and a case. **The spelling that
actually exists in this repository is the one `provider_scan` does not honour:**

```
$ find . -name '.nros-ignore' -o -name 'NROS_IGNORE'
./.nros-ignore
```

That file was added by issue 0621 so a vendored nano-ros checkout inside a
consumer's workspace stops polluting their package index. Its own header says
so:

> A VENDORED nano-ros is not part of the consumer's package graph.

## Consequence

`provider_scan::scan_workspace_packages` is what `nros ws order` uses, and
`nano_ros_workspace(ORDER_FROM_DEPENDS)` calls that at cmake configure time. So
a consumer who vendors nano-ros gets 0621's fix in the pkg-index walk and **not**
in the ordering walk: the ordering walk descends into the vendored checkout and
orders packages that are not theirs.

It is latent today only because `ORDER_FROM_DEPENDS` is used by in-tree
workspaces, where the vendored case does not arise. `autoware-safety-island`
and `nano-ros-rt-eval` both vendor nano-ros, so it becomes reachable as soon as
either adopts the ordering path — which phase-383 W2 does.

## Why it happened — the class, not the site

Issue 0621 fixed the walk where the symptom appeared and did not sweep the
siblings. This is the repository's most-repeated defect class, named in
CLAUDE.md: *"Fix the CLASS, not the reported site."* The two walks also differ
in three other ways nobody chose deliberately:

| | `nros-pkg-index` | `provider_scan` |
| --- | --- | --- |
| ignore markers | `COLCON_IGNORE`, `.nros-ignore` | `COLCON_IGNORE`, `AMENT_IGNORE`, `NROS_IGNORE` |
| pruned dirs | `target`, `build`, `.git`, `.cargo`, `node_modules`, `__pycache__` | + `install`, `log`, `third-party`, `generated` |
| pruned prefixes | `build-` | `build-`, `target-` |
| descends into a package | **yes** | **no** ("as colcon does not") |

Any consumer unioning the two — which phase-383 W2.a must — gets a
non-idempotent answer.

## Fix

Immediate: teach `provider_scan` the `.nros-ignore` spelling, so the marker that
exists on disk works in both walks. Landed with phase-383 W2.b.

Structural: the marker list wants to be ONE list both crates read, not two
lists that agree by inspection. `nros-pkg-index` is the lower-level crate and
the natural home. Deferred deliberately — phase-383 W2 needs the behaviour
fixed, and a cross-crate constant move is a separate, wider change that should
not ride inside a wave about something else.

The pruning and descent differences are NOT bugs to unify: they answer
different questions ("what packages exist" versus "what does colcon see"), and
`provider_scan.rs:31-33` already documents that. But a caller unioning them must
pick one as authoritative and say so.

## Sweep

```sh
grep -rn 'IGNORE_MARKER\|IGNORE_MARKERS\|COLCON_IGNORE\|nros-ignore\|NROS_IGNORE' \
  --include='*.rs' --include='*.py' --include='*.sh' packages/ scripts/
```

Every hit must agree on the marker set, or state why it does not.
