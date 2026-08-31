---
id: 955
title: "Six lane guards can never fire — they test `-z` on a variable `sdk-env.just` always exports"
status: open
area: build
severity: medium
found: 2026-08-31
related: [0599, 0650, phase-407]
---

# A guard predicate that cannot be true

Six platform-lane skip guards are written:

```sh
if [ -z "${NUTTX_DIR:-}" ] && [ ! -d third-party/nuttx/nuttx ]; then
    nros_lane_skip_note nuttx "NUTTX_DIR unset and third-party/nuttx/nuttx absent"; exit 0
fi
```

`just/sdk-env.just:19` exports it unconditionally, with a default:

```
export NUTTX_DIR := env("NUTTX_DIR", justfile_directory() / "third-party/nuttx/nuttx")
```

So `-z` is never true, so the `&&` is never true, and **the skip can never
fire**. 23 variables are exported this way; six guards depend on one of them
being unset.

Sites:

```
just/threadx-linux.just:119
just/nuttx.just:204, 343, 403, 442, 476
```

## Why it matters, and which direction it breaks

Under a broad lane, that step does not skip — it proceeds into cmake and
**fails** where the author intended a skip. So an unprovisioned host gets a hard
build failure with a cmake-level message instead of
`== nuttx == SKIPPED (NUTTX_DIR unset …)`.

That is the opposite of this repo's usual skip defect and worth stating plainly:
these do not launder a failure into a pass, they turn an intended skip into a
confusing red. phase-407 W2 (a NAMED platform must fail) does not fix it —
under W2 the failure is now *correct* when the platform was named, and still
wrong when it was merely included.

## The two idioms, one of which works

The same files contain guards that DO fire, testing the resolved directory
rather than the variable:

```
just/nuttx.just:96          if [ ! -d "$NUTTX_DIR/include" ]; then
just/threadx-linux.just:72  if [ ! -d "$THREADX_DIR/common/inc" ] || [ ! -d "$NETX_DIR/common/inc" ]; then
just/threadx-linux.just:190 if [ ! -d "$THREADX_DIR/common/inc" ]; then
```

This is the "second spelling instead of one shared helper" that CLAUDE.md files
under #282 -> #326: two idioms for one question, and the newer one is the broken
one. The fix is to converge on the working shape — probe the resolved path, not
the variable — and ideally through ONE helper rather than a third spelling.

## Work

1. Replace the six `-z`-on-an-exported-var guards with a resolved-path probe.
2. Prefer a single helper (`nros_sdk_present <var> <marker-subdir>`) over
   correcting six sites in place, so a seventh site cannot reintroduce the dead
   form.
3. Gate it: a `-z "${X:-}"` test on any variable `sdk-env.just` exports is
   statically detectable, and the export list is right there to read.
4. Verify on a host where the SDK is genuinely absent — env overrides exercise
   the code path but not the real condition.

Found while implementing phase-407 W2, which is also why it is filed rather than
folded in: changing which lanes skip versus fail is exactly what W2 was
scoped to do deliberately and narrowly, and this changes it for a different
reason.
