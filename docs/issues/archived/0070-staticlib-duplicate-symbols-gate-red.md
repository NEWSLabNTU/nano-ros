---
id: 70
title: staticlib duplicate-symbol gate red — both link-determinism tests fail on main
status: resolved
type: bug
area: build
related: [issue-0062, phase-241, phase-249]
resolved_in: phase-249 / #62 single-runtime
---

> **RESOLVED (2026-06-16).** #62/phase-249 settled the 241.D3-rev single-runtime
> link model, so the test was rewritten for the single archive (the rewrite was
> blocked on #62's outcome — now landed). `staticlib_duplicate_symbols.rs`:
> dropped the obsolete 2-archive dup-diff (moot — one archive) + the
> `host_pair_links…` 2-archive proof; replaced with
> `single_archive_links_via_u_force_without_allow_multiple_definition` — asserts
> the single `build/link-determinism/libnros_c.a` (zenoh bundled) links a host
> binary with `-u nros_rmw_zenoh_register` and **NO `--allow-multiple-definition`**,
> register entry pulled, exactly ONE `REGISTRY`. `nm` helper now prefers `llvm-nm`,
> falls back to GNU `nm`. Fixture header comment corrected (one archive, not a
> pair). Verified: fixture builds `libnros_c.a` (zenoh bundled); the test passes
> (links clean, 1 REGISTRY) — `check.yml` link-determinism gate green.

## Root cause (diagnosed 2026-06-16) — test stale vs 241.D3-rev single-runtime

Not a real link regression: both tests `skip!` (→ panic → `cargo test` counts it
FAILED, so `check.yml`/`just check-staticlib-symbols` go red) because the staticlib
**pair they expect no longer exists**.

`scripts/build/link-determinism-fixture.sh` was updated for the **241.D3-rev
single-runtime** model — it now builds ONE archive,
`build/link-determinism/libnros_c.a` (with `--features platform-posix,rmw-zenoh`,
so the zenoh backend is bundled INTO `libnros_c.a`), and no longer produces
`libnros_rmw_zenoh_staticlib.a`. But `staticlib_duplicate_symbols.rs` still:
- `find_archive_pair` looks for the `(libnros_c.a, libnros_rmw_zenoh_staticlib.a)`
  **pair** → not found → both tests skip.
- its premise (diff duplicate symbols ACROSS the pair; prove the C-only pair links
  with `-u` WITHOUT `--allow-multiple-definition`) assumes the pre-single-runtime
  2-archive link.

Under single-runtime the backend is bundled into `libnros_c.a`, so "duplicate
symbols across a pair" is moot, and **whether `--allow-multiple-definition` is
removable IS the open question owned by [#62](0062-d3-completion-one-registration-path-and-link-manifest.md)
(D3 completion, R2/R3)**. Rewriting the test now would pre-judge #62's outcome.

## Direction — do this WITH #62 (not standalone)

Rewrite `staticlib_duplicate_symbols.rs` for the single-runtime model as part of
#62: assert the single `libnros_c.a` links standalone with `-u
nros_rmw_zenoh_register`, exactly ONE `REGISTRY`, no `--allow-multiple-definition`
(the D3 goal). Drop the obsolete 2-archive dup-diff. Until #62 lands the final
link model, the gate stays red on this skip; tracked here + cross-linked from #62.

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
