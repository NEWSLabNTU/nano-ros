---
id: 764
title: "The fixture staleness probe compares MTIMES while the build compares
  CONTENT — a source whose mtime moves without its content changing is STALE
  forever, and rebuilding cannot clear it"
status: resolved
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

## Resolved (2026-08-25, `493440c65`) — the cmake arm now decides by CONTENT

The Direction offered two options and called the content-based one "the honest
fix" against "the one that lands in an afternoon". It was neither expensive nor
a choice: **the content machinery already existed and two of the three probe
arms already used it.**

* `staleness::candidates_changed_content_policy` + the `.nros-srcbaseline`
  sidecar — written for #147 / phase-286 W2;
* the **zephyr** arm has used it since then;
* the **cargo dep-info** arm since phase-353 W2 (`dep_info_newer_source`);
* the **cmake/ninja** arm never got it. That is the arm this issue reproduces on.

So the fix was wiring, not construction, and no design call was needed.

### What changed

`cmake_dep_info_newer_source` gathers the whole candidate set instead of
returning on the first newer mtime, then lets the bytes decide through the same
helper, with the same three-way answer the cargo arm uses — `Some(true)`
genuinely edited, `Some(false)` an mtime-only rewrite, `None` keep the old strict
answer.

`zpico_c_source_newer` became `zpico_c_inputs`, returning BOTH the newer path and
the candidate set from ONE walk. Two functions resolving the same input set is
exactly how these arms diverged in #0442; the answer and the evidence for it have
to come from the same resolution.

### Verified in BOTH directions

The second direction matters more than the first: a content-aware probe that
forgives everything is strictly worse than the mtime probe it replaces, because
it turns museum binaries into silent passes (#0196's shape).

1. fresh baseline after a clean `build-test-fixtures lane=native` — 2/2 pass;
2. `touch zpico.c` — mtime newer than the binary, sha unchanged
   (`0f2fe91a91da07ed`) — **2/2 still pass**. This issue's exact repro;
3. a real one-line append (sha `9c776bd22c50e73c`) — **STALE**, citing
   `zpico.c`, on both fixtures;
4. `git checkout` the file — sha restored, tree clean, 2/2 pass again.

### The class, swept

This was the third turn of a shape CLAUDE.md cites as its own worked example of
the class rule — "#222 fixed 4 RTOS resolvers, left ~30 in `binaries/mod.rs` →
#328". Helper added, `binaries/mod.rs` not wired: #222 → #328 → #764.

Every remaining raw mtime comparison in that file now FEEDS the content decision
rather than returning a verdict (1377/1418 live in
`newest_path_after`/`newest_source_after`, whose only callers are inside
`zpico_c_inputs`), and all three early returns of the rewired function are
`None`. mtime finds the candidate; content decides.

    grep -nE '> *bin_mtime|path_newer_than' packages/testing/nros-tests/src/fixtures/binaries/mod.rs
