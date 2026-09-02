---
id: 987
title: "The mirror's `cargo/*/` glob took the FIRST candidate, not the newest,
  so a six-week-old residue target dir shadowed the current one"
status: resolved
type: bug
area: cmake, build
severity: high
found: 2026-09-02
related: [issue-0978, issue-0985, issue-0500, issue-0488, issue-0369]
---

## Symptom

`just build native` fails linking `action_raw_goal_probe` on issue 0369's size
anchor:

```
undefined reference to `nros_config_variant_sz_886681abade04db2'
```

The direction is the OPPOSITE of issue 0985: the mirrored HEADER is old and
every archive is current.

## Measured

Two cargo target dirs under one leaf
(`packages/testing/nros-tests/bins/action-raw-goal-probe/build-zenoh`):

| path | mtime | anchor |
| --- | --- | --- |
| `cargo/build/nros-c-generated/…` | **2026-08-19 06:48** | `sz_886681abade04db2` |
| `cargo/nano-ros_1147c/nros-c-generated/…` | 2026-09-02 14:00 | `sz_9a3e918900c9d46d` |

The mirror wrote the 2026-08-19 one into the include dirs; every archive the
link consumes was `sz_9a3e918900c9d46d`.

## Root cause: first-match, and glob order is by NAME

```sh
for cand in "$build_dir"/cargo/*/"$gen_subdir"/nros/"$name"; do
    [ -f "$cand" ] && { src="$cand"; break; }
done
```

`break` takes the first candidate and glob expansion is sorted, so
`cargo/build/` beats `cargo/nano-ros_1147c/` whatever their dates.
`cargo/build/` is residue from the pre-phase-340 target-dir layout (issue
0488's class); nothing rewrites it, so it is frozen and shadowed the live store
on every run.

Issue 0978's premise — "refreshed by ANY leaf's run, so it is always at least
as fresh as (1), never staler" — is true of THE shared copy. The glob can match
several, and the selection rule was never stated: it was inherited from a
comment reading "one entry in practice", which stopped being true silently,
because first-match still returns A file and the failure lands a layer down as
a link error naming a hash and no path.

Same shape as issue 0500, whose remedy is the rule missing here: a store that
ACCUMULATES needs an ordering. There the SDK prefixes are enumerated
newest-VERSION-first because `find_package` takes the first that resolves; here
the residue is not distinguishable by name, so the order is by MTIME.

## Fix

Pick the newest candidate by mtime (`stat -c %Y`, BSD `stat -f %m` second, 0 if
neither answers so an unreadable mtime cannot win over a known one).

Self-test case `the newest shared copy wins, not the first by name`: two shared
dirs where the stale one sorts FIRST by name (`aaa_old` before `zzz_new`) and is
older by mtime — issue 0987's tree in miniature. Proven non-vacuous: under the
old first-match selection exactly that case fails (`got OLD`) and the other five
pass, #0978's included.

## Verified, and one attribution corrected

The resolver, run against the real leaf, returns `sz_9a3e918900c9d46d` where it
used to return `sz_886681abade04db2`.

`action_raw_goal_probe` then linked — but **not because of this fix**, and the
distinction is worth recording. Its `main.c.o` resolves the header from the
*nros-cpp* include dir, which in this leaf happened to be current; the nros-c
copy was still stale. The clean attribution is to force the mirror edge itself:

```
BEFORE  nros-c/include/nros/nros_config_generated.h   sz_886681abade04db2
$ rm  <that file>
$ ninja -C <leaf> nros_c_config_header
AFTER   nros-c/include/nros/nros_config_generated.h   sz_9a3e918900c9d46d
```

That is this change, through the real cmake path rather than a direct probe.

## Left open

**An already-drifted `nros-c` copy does not self-heal.** Its mirror is an
`add_custom_command` whose OUTPUT is compared against
`$<TARGET_FILE:nros_c-static>`; when the two share a timestamp — which they do,
being written by the same build — ninja treats the output as up to date and
skips it, so a copy that is stale for some *other* reason stays stale until the
crate rebuilds. Issue 0985 gave `nros-cpp` a configure-time heal that now runs
this same resolver; `nros-c` has no equivalent. Adding one is a behaviour change
in a second file and belongs in its own change, not here.

## Acceptance

* [x] When the glob matches several candidates the newest wins, with a
      self-test case that fails under first-match ordering.
* [x] The mirror edge writes the current header for the leaf that failed.
