---
id: 764
title: "The fixture staleness probe compares MTIMES while the build compares
  CONTENT — a source whose mtime moves without its content changing is STALE
  forever, and rebuilding cannot clear it"
status: open
type: bug
area: testing, build
related: [issue-0445, issue-0196, issue-0475]
---

## Problem

The two halves of the freshness contract do not use the same definition:

* **The build is CONTENT-based.** Cargo rebuilds, and corrosion copies its
  output to the archive cmake links with `copy_if_different`. Identical bytes
  means no copy, which means the archive under `|` in the ninja graph never
  changes, which means no relink. That is *correct* — relinking to produce a
  byte-identical binary is waste.
* **The probe is MTIME-based.** `Test fixture is STALE — a source is newer than
  the built binary`.

So when a source's mtime moves and its content does not, the probe reports STALE
and **no amount of rebuilding clears it**. The build is right to do nothing; the
probe is right that the mtime is older; the two are answering different
questions.

This is NOT the documented mtime treadmill. That one (CLAUDE.md, "Fixture mtime
treadmill") says a pull/rebase/`stash pop` re-stales fixtures and prescribes
"rebase once → rebuild affected fixtures". That remedy works when content
changed. Here rebuilding is a no-op **by construction**, so the prescribed
remedy cannot succeed.

## Evidence

Reproduced 2026-08-23 by `touch`ing `packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c`
(mtime only, byte-identical) and running a full
`just build-test-fixtures lane=native`:

```
examples/native/cpp/action-server/build-zenoh/
  cargo/…/release/libnros_cpp.a                      14:56   <- cargo rebuilt it
  nano_ros/packages/api/nros-cpp/libnros_cpp.a       08:24   <- the copy cmake LINKS
  cpp_action_server                                  14:39   <- never relinked
```

The archive *is* an implicit (`|`) dependency in the ninja graph, so the relink
edge exists and issue 0475's fix is intact — the copy simply never ran, because
`copy_if_different` correctly saw identical bytes. Cargo did rebuild: the leaf's
`zpico-sys-*/output` records a `rerun-if-changed` on `zpico.c`, and its mtime
moved to 14:56.

Test-side, the same state reads:

```
Test fixture is STALE — a source is newer than the built binary:
  binary: …/cpp_action_server
  newer:  …/zpico-sys/c/zpico/zpico.c
  probe:  examined 112 input(s)
  NOT RUN: 7th consecutive stale verdict for this fixture
```

16 tests failed this way in one tier-1 run, all reporting as real failures
rather than skips.

## Why it is easy to misdiagnose (it took three wrong turns here)

1. A `touch` that produces no relink looks *identical* to a broken rebuild edge,
   and is in fact evidence the build is correctly content-based.
2. A semantic change DOES propagate — verified by adding a function to `zpico.c`
   and finding the symbol in the linked binary — so "the edge is fine" and "the
   verdict is unclearable" are both true, of different scenarios.
3. The absorbing-verdict behaviour of issue 0445 hides whatever the fixture
   would have done at runtime, so the symptom is always the same message
   whatever the cause.

## Direction

Two options; the choice is a design call.

* **Make the probe content-based** (hash the inputs it already enumerates,
  compare against a recorded manifest). Correct — it would then agree with how
  the build actually decides — and it retires this class rather than the
  instance. Costs hashing on every resolution, though the probe already stats
  every input.
* **Make the build touch its output when cargo reports a rebuild.** Cheaper and
  local, but it re-introduces exactly the waste `copy_if_different` exists to
  avoid, and it makes the binary's mtime a lie about when its content last
  changed.

The first is the honest fix; the second is the one that lands in an afternoon.

## Workaround used meanwhile

`find examples/native -path '*nano_ros/packages/api*' -name 'libnros_c*.a'
-exec touch {} \;` then rebuild — forces the copy-skipped relink for 106
archives. The binaries' CONTENT was correct throughout; only their mtimes lagged.
