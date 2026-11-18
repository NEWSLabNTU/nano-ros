---
id: 2697
title: "A provisioned Zephyr workspace can sit at a different revision from the pin, and nothing says so"
status: open
type: bug
severity: medium
area: [zephyr, build, testing]
related: [2389, 1387, 1395]
found: 2026-11-18
---

## What happens

Issue 2389 (70bc8affb5, 2026-11-12) moved the Zephyr 3.7 line to v3.7.2:
`west.yml` pins `zephyr` at `revision: v3.7.2`. Six days later, the shared
3.7 workspace on host 175 (`~/.nros/workspaces/zephyr/3.7`) is still at
v3.7.0:

```text
$ git -C ~/.nros/workspaces/zephyr/3.7/zephyr describe --tags
v3.7.0
$ git -C ~/.nros/workspaces/zephyr/3.7/zephyr rev-parse 'v3.7.2^{commit}'
bbbf8b30094dacd0d6830946ba206bcec8af6b0c     # the tag is fetched, just not checked out
```

The workspace's own copy of the manifest (`nano-ros/west.yml`, the file
`.west/config` names, last written 2026-09-21) still says `revision: v3.7.0`,
so even a `west update` run inside the workspace would keep v3.7.0. The copy
has to be re-synced from the checkout first
(`scripts/zephyr/sync-workspace-manifest.sh`).

The 4.4 workspace there matches its pin (`v4.4.0`). So every local Zephyr 3.7
build on host 175 since 2026-11-12 used a kernel CI does not use. The image
says so in its banner, and nothing else does:

```text
CI (run-matrix 44030238880):  *** Booting Zephyr OS build v3.7.2 ***
host 175, same cell:          *** Booting Zephyr OS build v3.7.0 ***
```

That is how it was found: the XRCE C++ example cells
(`example_e2e::case_2{1,4,7}_xrce_cpp_*`) failed in CI and passed 10 times out
of 10 on this host. The difference in kernel revision was the first difference
between the two runs that anything measured.

## What does not catch it

- `nros doctor` reports `[OK] Zephyr workspace names no other checkout (issue
  1387)`. That asks WHOSE workspace it is, not WHICH REVISION it holds.
- `just _require-owned-provisioned-roots` (issue 1395) asks the same question.
- `west update` is the fix, but nothing runs it. A pin move reaches a host only
  when someone re-provisions by hand, and `west update` never runs implicitly
  (and never prunes, per CLAUDE.md).
- The build itself does not compare. A west build against a v3.7.0 tree with
  a v3.7.2 manifest succeeds.

The workspace also carries local edits from the lane's patch scripts
(`git status` there shows `drivers/net/nsos*.c` and `CMakeLists.txt`
modified, from `scripts/zephyr/*-patch.sh`). So "the checkout differs from the
pin" has two parts: HEAD is not the pinned commit, and the tree holds the
lane's patches. Only the first is a fault.

## Open questions

- Have other hosts drifted too? Any host provisioned before 2026-11-12 and not
  re-provisioned since has the same v3.7.0 tree, so the drift is probably
  general rather than specific to 175. Each host's
  `git -C <workspace>/zephyr describe --tags`, compared with `west.yml`,
  answers it.
- Should `nros setup zephyr` move a workspace that already exists to the pin,
  or only report it? A shared workspace is used by every session on its host,
  so moving it under a session that is building changes that build part-way
  through.

## Proposed fix

1. `nros doctor` (and so `just zephyr doctor`) compares each provisioned
   workspace's project HEADs with the manifest it was provisioned from
   (`west list -f '{name} {revision} {sha}'` against each project's
   `rev-parse HEAD`) and REFUSES on a mismatch. The message names the
   project, both revisions and the remedy (`nros setup zephyr --line 3.7`, or
   re-syncing the workspace manifest and then `west update`). The patch
   scripts' edits do not count: compare commits, not the work tree. Compare
   against the CHECKOUT's `west.yml`, not the workspace's copy of it, which can
   be stale too (above).
2. `_require-owned-provisioned-roots`, which every Zephyr fixture lane already
   runs first, asks the same question, so a fixture build stops before
   building against a kernel the pin does not name. The verdict should be a
   refusal, not a skip, because an image built against the wrong kernel is not
   a fact about this tree.
3. Moving a SHARED workspace stays a coordinated, explicit step: the check
   refuses and names the command, and never runs `west update` itself.

## Done when

On a host whose workspace HEAD differs from its `west.yml` pin, `nros doctor`
and the Zephyr fixture lane each fail, naming the project, both revisions and
the command that fixes it. On a host that matches, both are green.
