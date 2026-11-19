---
id: 2635
title: "Nothing writes the starter image's context before a release, so a book edit broke the 0.6.0-nros11 images after the release had published"
status: open
type: bug
severity: medium
area: [release, ci, book]
related: [phase-551, 2612]
found: 2026-11-16
---

**Home: [phase-551](../roadmap/phase-551-starter-image.md)** W4.

# The starter image's context was first written after the release published

## Symptom

`release-nros.yml` run 43806377053 published `nros-0.6.0-nros11` (tag at
09219edc75, three CLI assets, the notes-page body), then its `starter image`
job failed before any docker build:

```
book/src/starters/zephyr-native-sim.md: expected one untabbed probe=34 block, found 3
```

Nothing was pushed: no `ghcr.io/newslabntu/nano-ros-starter:0.6.0-nros11`,
and `latest` stayed on `0.6.0-nros10`.

## Cause

`_nros_starter_zephyr_steps` (`scripts/ci/lib/image-context.sh`) turns the
Zephyr starter page's `probe=` blocks into the `starter-zephyr` stage's
scripts, and read each step as exactly ONE block. #3989 (c7c5dba93c) split
step 34 into three: the `mkdir`, a `file=manifest/west.yml` block, and the
`west init` commands. The probe runner already joins every block of one
order into one step in page order, so the probe stayed green; the image's
reader had its own, narrower rule.

Two reasons it reached a release:

- No merge gate writes the starter context. The runner's context has a golden
  gate (`runner-context-golden`); the starter's had none.
- The release dry run skips the image job (it needs a published tag), so the
  first write of the context was after publish, from a tag that can no
  longer change.

## Fix

- `scripts/probe/extract-book-steps.py` gains `order_body(steps, order, where)`:
  one step is every block of its order, in page order, the rule `main` already
  applies. It refuses a step with no block, a step inside a language tab, and
  a `probe-run` block that is not the step's last. Self-tested.
- The image's context writer uses it instead of its own one-block reader.
- `just check starter-image-context` (fast line, about 1 s) writes the starter
  context for the current tree and asserts each Zephyr step script exists and
  is non-empty, and that the Dockerfile names both targets. With the old
  reader restored it fails with this issue's message.
- `release-nros.yml` writes the starter context in its build job, on every
  run, dry runs included, before anything is published.
- `starter-image.yml` takes `scripts_from_main` (dispatch only): build a
  released version's image with main's context writer over the tag's tree.
  The book it reads, the index, `install.sh` and every copied input stay the
  tag's; only the five writer files come from main, and their revision goes in
  a label. User ruling 2026-11-16, for `0.6.0-nros11`.

## Next

Dispatch `starter-image.yml` with `version=0.6.0-nros11`, `target=starter`,
`scripts_from_main=true` once this is on main, then `starter-zephyr`, then
move `latest` through W5's `image` probe track.

## Status (2026-11-19): both images rebuilt; `latest` not moved

The fix merged as #4029 (0ffe141668, 2026-11-16 07:30Z). The two dispatches
"Next" asks for then ran green:
- `starter-image.yml` run 43825309370, `build + push starter 0.6.0-nros11`;
- run 43829081957, `build + push starter-zephyr 0.6.0-nros11`.

Both skipped the retag job, and no dispatch since has run it. What is left
is moving `latest` through W5's `image` probe track. Then this issue closes.
