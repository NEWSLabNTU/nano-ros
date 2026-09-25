---
id: 1497
title: "Tier 2's zephyr module dies in `codegen-system`: the `zephyr_self_pkg`
  fixture leaves declare system semantics and no SystemModel is generated for
  them, so the lane stops in the build with no cell verdict"
status: open
type: bug
area: ci, build, cli, testing
severity: high
found: 2026-09-25
related: [1158, 1457, 0533, 1501]
---

> **Root cause found and fixed — issue 1501 (2026-09-25).** None of the three
> candidates below was the answer, and the first one was already implemented:
> `west-fixtures.sh` has run `nros sync` per bringup since issue 0533. It could
> not work, because these two leaves carry no `package.xml`, and sync scans
> `src/<pkg>/package.xml` or a root `package.xml` and rejects a dir with
> neither. So the fix is a FOURTH option — make the leaves syncable, which is
> what every converted example leaf (`examples/zephyr/rust/*`) already was.
> Kept OPEN until a `run-matrix` run reports a stage past the build: this was
> one of at least two independent build-stage blockers, the other being issue
> 1457's `rosidl_adapter` in the cyclonedds leaves.

## What happens

`run-matrix` (tier 2, 1-wise) run **36103083615** (schedule, 2026-09-25T06:29),
job **107969611335**, step **`just build tier2`**. The lane reaches its fixture
build and the zephyr module fails:

```
== zephyr == FAILED (rc=1)
  3267:CMake Error at .../zephyr/cmake/nros_system_generate.cmake:316 (message):
  3298:FATAL ERROR: command exited with status 1: /usr/bin/cmake ... \
       -B.../build/west-fixtures/zephyr_self_pkg_rust ... \
       -S.../packages/testing/nros-tests/fixtures/zephyr_self_pkg/self/alpha_pkg
  3358:CMake Error at .../nros_system_generate.cmake:316 (message):
  3389:FATAL ERROR: command exited with status 1: /usr/bin/cmake ... \
       -B.../build/west-fixtures/zephyr_self_pkg_sibling ...
```

and the message the CMake error carries:

```
codegen-system: target `zephyr` — the image claiming entry `caller`
(--for-entry; issue 1312)
codegen-system: ROS edition = humble (RFC-0056; type-hash + keyexpr format)
Error: codegen-system:
  .../fixtures/zephyr_self_pkg/sibling/alpha_pkg/system.toml
  declares system semantics but no SystemModel was found.  It is a BUILD
  ARTIFACT (phase-330 W4), so generate it rather than committing one:

    nros sync      # writes <ws>/build/nros/models/<bringup>/
```

Two west fixtures are affected, `zephyr_self_pkg_rust` (the `self/` tree) and
`zephyr_self_pkg_sibling` (the `sibling/` tree).

**The lane's own stage report agrees and is worth quoting**, because it is issue
1158's remedy working: the coverage job is named
`tier 2 — NO VERDICT: stopped in the build`. No cell ran; there is no runtime
verdict in this run to interpret.

## What this is NOT

- **Not a committed-model problem.** The refusal is the phase-330 W4 guard
  firing correctly: `system.toml` declares semantics, the model is a build
  artifact, and none was generated. Committing one is explicitly the wrong fix
  and `check-no-tracked-models` bans it.
- **Not issue 1457.** That is the same lane and the same step on the SAME NIGHT
  — the 05:12 nightly's tier-2 job died on `rosidl_adapter is not importable by
  this build's interpreter` in the cyclonedds leaves. Different leaves,
  different mechanism. Tier 2 currently has TWO independent build-stage
  blockers, and fixing either alone leaves the lane red.
- **Not issue 1158 itself.** 1158 is the umbrella "tier 2 produces no verdict";
  this is one concrete cause of it, and 1158's stage reporting is what made the
  cause findable.
- **Not archived issue 0533**, though it is the closest relative: that was the
  west fixture lane never resolving its bringups' models at all, hidden by a
  `|| true`. Here the failure is loud and reaches a named leaf.
- **Not `provision-zenohd` exit 78** at line 908 of the same job — that is
  issue 1477's tolerated skip and the build continued past it.

## What would close it

The choice is where the model for a west FIXTURE leaf is meant to come from,
and there is more than one defensible site, which is why this is filed rather
than guessed:

1. **The fixture build runs `nros sync` for these leaves** before the west
   configure, the way a workspace entry gets its model.
2. **`zephyr_self_pkg`'s leaves stop declaring system semantics** if the
   fixture's purpose does not need them — they exist to exercise the self/
   sibling package shapes, not bringup resolution.
3. **`nros_system_generate.cmake` resolves the model itself** for a leaf whose
   workspace has not been synced, if that is meant to be in scope.

Acceptance: a `run-matrix` run whose coverage job reports a stage past the
build — i.e. cells run and the lane produces a verdict, green or red.
