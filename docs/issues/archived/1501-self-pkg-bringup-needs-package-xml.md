---
id: 1501
title: "A self-pkg bringup that declares `[[component]]` with no `package.xml`
  has NO producer for its SystemModel — `nros sync` cannot scan it, so every
  bake refuses at configure"
status: resolved
type: bug
area: build, cli, testing
severity: high
found: 2026-09-25
resolved: 2026-09-25
related: [1497, 1158, 0533, 0380]
---

## The rule

Since phase-330 W4 a SystemModel is a BUILD ARTIFACT, and `codegen-system`
refuses to bake a bringup that declares system semantics without one
(`codegen_system.rs:648`). The ONLY producer of that artifact is `nros sync`,
and sync's package scan has exactly two shapes (`cmd/ws.rs`):

* a colcon workspace — `src/<pkg>/package.xml`;
* a single-package dir — `package.xml` at the root.

A directory with neither is rejected outright:

```
sync: no `src/<pkg>/package.xml` and no `package.xml` at root under <dir>
  — expected colcon-style workspace or single-pkg dir
```

So: **a directory whose `system.toml` declares `[[component]]` rows, and which
is a package (`Cargo.toml` or `CMakeLists.txt` beside it — the shape
`_nros_system_detect_self_pkg` accepts as a self-pkg bringup), must carry a
`package.xml`.** Without one there is no road from the declaration to the
artifact the declaration makes mandatory, and the failure surfaces one tool and
one step away from the cause: the configure names the missing MODEL, not the
missing manifest.

## How it got in

phase-445 W5 (RFC-0098 D3/D5, commit `2bb20c231`, 2026-09-11) moved every
leaf's `[package.metadata.nros.{component,deploy.zephyr}]` tables into a
`system.toml` beside the manifest. Every converted EXAMPLE leaf already had a
`package.xml` (`examples/zephyr/rust/*` all do), so the migration was
model-resolvable there. The two `zephyr_self_pkg` FIXTURE leaves did not, and
nothing asked: the move silently reclassified them from "configless self-pkg,
take the default bake" to "declares system semantics, must resolve a model",
and from that commit on neither could CONFIGURE.

It stayed invisible for a fortnight because the only lane that builds them
(tier 2's west fixtures) was itself stopped earlier — issue 1458's missing
`-DZEPHYR_EXTRA_MODULES` killed four of five west rows at Kconfig, and 1476
false-failed the checkout before that. When phase-466 cleared both, this became
the lane's next stop: issue 1497.

## Fix

1. `package.xml` added to
   `packages/testing/nros-tests/fixtures/zephyr_self_pkg/{self,sibling}/alpha_pkg`.
   The `nros sync` that `west-fixtures.sh` has run per bringup since issue 0533
   then succeeds and writes
   `<leaf>/build/nros/models/alpha_pkg/system_model.yaml` — exactly the rung
   `model_search_paths`' standalone-leaf arm reads.
2. Gate `check-self-pkg-package-xml` (fast lane, `scripts/`), so the class
   cannot come back through the next migration. It carries its own
   both-directions negative control.
3. `west-fixtures.sh` no longer discards sync's output. It printed one line
   naming neither cause nor remedy while `>/dev/null 2>&1` ate the refusal
   that explains everything.

Sweep (0 remaining, and exactly these 2 before the fix):

```
git ls-files '*system.toml' | while read -r f; do d=$(dirname "$f");
  grep -q '^\[\[component\]\]' "$f" || continue
  { [ -f "$d/Cargo.toml" ] || [ -f "$d/CMakeLists.txt" ]; } || continue
  [ -f "$d/package.xml" ] || echo "$d"
done
```

## Measured

With the `package.xml` in place, on this checkout:

```
$ (cd .../sibling/alpha_pkg && nros sync)
sync: resolved system.launch.xml → build/nros/models/alpha_pkg/system_model.yaml

$ nros codegen-system --workspace <alpha_pkg> --bringup <alpha_pkg> \
      --for-entry <caller> --rmw zenoh --out <tmp>
codegen-system: target `zephyr` — the image claiming entry `caller`
codegen-system: using committed SystemModel .../build/nros/models/alpha_pkg/system_model.yaml
nros codegen system: wrote bake tree at <tmp>/nros-system
```

Same for the `self/` leaf, whose `--for-entry` is the app dir itself.

## Not verified here

The west CONFIGURE itself (`west build --cmake-only`) was not run on this host —
no provisioned Zephyr workspace. What was verified is the pair of commands the
configure shells, with the arguments `nros_system_generate.cmake` passes, which
is where the failure was.
