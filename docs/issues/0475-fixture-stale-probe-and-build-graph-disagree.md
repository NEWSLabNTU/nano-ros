---
id: 475
title: "A cmake fixture can be permanently STALE: the probe says rebuild, the build graph says nothing to do, and only `rm -rf` clears it"
status: open
type: bug
severity: high
area: testing, cmake
related: [issue-0196, issue-0445, issue-0466, issue-0268]
---

## Symptom

Touch `packages/core/nros-rmw-abi/include/nros/rmw_ret.h` (any ABI header edit
does it — mine was adding one constant for #0468). Every C/C++ CycloneDDS
fixture then reports:

```
Test fixture is STALE — a source is newer than the built binary:
  binary: examples/native/c/talker/build-cyclonedds/c_talker
  newer:  packages/core/nros-rmw-abi/include/nros/rmw_ret.h
  probe:  examined 8503 input(s); …
  NOT RUN: 12th consecutive stale verdict for this fixture, first 64m ago.
```

`just native build-fixtures` does not clear it. Neither does `cmake --build` on
the leaf directly — it runs, rebuilds `libnros_c.a`, exits 0, and **does not
relink the binary**, whose mtime is unchanged. The only thing that clears it is
`rm -rf <leaf>/build-cyclonedds` followed by a full rebuild: **687 s for one
leaf**, because CycloneDDS self-provisions from source (phase-186).

There are ~8 such leaves. A one-constant header edit therefore costs an hour of
wiping, or the cells stay red.

## Both sides are individually correct

* The **test probe** is conservative by construction: any input newer than the
  binary is stale. It examined 8503 inputs and found the header among them.
* The **build graph** is precise: that C example never `#include`s
  `nros/rmw_ret.h`. Make has no edge from it to `c_talker`, so there is
  genuinely nothing to rebuild.

Neither is wrong in isolation; together they deadlock. This is issue 0196's rule
("build-side stale probes must watch the same inputs as test-side gates")
pointing the OTHER way from usual — there the build probe was too narrow, here
the test probe is too broad, and the failure is worse because no build command
can satisfy it.

The probe's own text anticipates this: *"If the rebuild does not clear it,
suspect the probe before trusting the verdict."* That instruction was correct
and I still spent three cycles reading these as regressions, because the message
is per-fixture and the presentation is 100+ simultaneous failures across
unrelated suites.

## Why it presents as a wall

`just ci`'s `_check-fixtures-stale` PASSED on the same tree, so the run reached
`test-all` and produced 115 failures whose text is identical and whose subject
lines look unrelated (`actions`, `params`, `qos`, `services`, …). The stamp that
gate reads answers "was this lane built", never "is it still fresh" — see the
follow-up in #0466.

## Reproduce

```console
$ touch packages/core/nros-rmw-abi/include/nros/rmw_ret.h
$ cmake --build examples/native/c/listener/build-cyclonedds -j    # rc=0
$ stat -c %y examples/native/c/listener/build-cyclonedds/c_listener   # UNCHANGED
$ cargo nextest run -p nros-tests --test native_api -E 'test(cyclonedds)'  # all STALE
$ rm -rf examples/native/c/listener/build-cyclonedds && just native build-fixtures  # 687s, clears it
```

## Fix directions

1. **Narrow the probe's input set to the leaf's real dependency graph.** cmake
   already writes depfiles; the probe could read them instead of walking 8503
   paths. Most faithful, most work.
2. **Exempt headers the leaf demonstrably does not include** — the same shape as
   the existing "regenerated-in-place header" and "cargo OUT_DIR product"
   exemptions the probe already reports. Cheaper, and it is the class those
   exemptions were invented for.
3. **Make the wipe the remedy the message names.** If the probe cannot know
   whether a rebuild will help, "Run `just build-test-fixtures` first" is
   actively wrong here; it should say to wipe the build dir when the input is a
   header outside the leaf's graph.

(1) or (2) are the real fixes. (3) alone would at least stop sending people to a
command that cannot work — which is what burned the time.
