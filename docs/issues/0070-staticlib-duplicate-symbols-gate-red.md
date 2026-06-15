---
id: 70
title: staticlib duplicate-symbol gate red — both link-determinism tests fail on main
status: open
type: bug
area: build
related: [issue-0062, phase-241, phase-249]
---

## Symptom

`cargo test -p nros-tests --test staticlib_duplicate_symbols` (the `check.yml`
link-determinism gate, now also `just check-staticlib-symbols`) fails both tests
after `scripts/build/link-determinism-fixture.sh` builds the host staticlib pair:

```
test host_pair_links_via_u_force_without_allow_multiple_definition ... FAILED   (:325)
test staticlib_duplicate_symbols_are_only_shared_deps ... FAILED                (:225)
```

The skip-guards pass (llvm-nm present, archive pair found), so the assertions
themselves fail — a real link-set mismatch, not a missing-fixture skip. Standing
red on `check.yml`.

## Cause (to confirm)

The dup-symbol allowlist / `-u`-force link proof no longer matches the actual
staticlib symbol closure. Candidates, in order of likelihood:
- phase-248/249 link/registration rework (the `.init_array` ctor + single-runtime
  changes — RFC-0042 §D3, issue #62) shifted which symbols the host pair shares.
- the #67 `nros/rmw-cyclonedds` descriptor-hook marker changed the link graph for
  the cyclone leg of the fixture.

## Fix direction (not started)

1. Run the fixture + dump the two archives' symbol diff
   (`llvm-nm` per `staticlib_duplicate_symbols.rs`); compare against the test's
   expected shared-dep set (`:225` allowlist) and the `-u`-force expectation
   (`:325`).
2. Reconcile: either the allowlist is stale (update it to the post-241/249 closure)
   or a real ODR/dup regression slipped in (fix the link). The phase-247 weak-image
   gate + #62 (D3 completion) are the adjacent context.

Surfaced by the CI reorg (`just check` SSoT); see
`docs/development/ci-workflow-reorg.md`.
