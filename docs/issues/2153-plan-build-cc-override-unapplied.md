---
id: 2153
title: "A plan's `[build.cc]` override parses and changes no build"
status: open
type: bug
area: [cli, orchestration]
severity: low
found: 2026-10-30
related: [0202, phase-515]
---

## What

`PlanBuildOptions::cc` (`packages/cli/nros-cli-core/src/orchestration/plan.rs`)
deserialises a per-layer `[build.cc]` block — `debug`, `opt_level`, `cflags` —
and its doc comment promised it reached every `cc::Build`: `debug`/`cflags` as
`CFLAGS`/`CXXFLAGS`, `opt_level` as `NROS_CC_OPT` (phase-204.15 / 204.9).

Nothing reads it. The generator that exported those variables was retired with
the standalone-package pipeline (#202, `11a00b0f8`), and no build script reads
`NROS_CC_OPT` any more. Measured 2026-10-30:

```text
git grep -nE 'PlanCcOverrides|NROS_CC_OPT' -- packages scripts cmake just
```

finds only the struct, its field and its own unit test. A user who writes
`[build.cc] opt_level = "s"` gets a plan that carries it and an image built
exactly as without it, with no diagnostic.

Found by the phase-515 W0.d comment-name rewrite pass: the doc comment named
`NROS_CC_OPT`, which the gate struck as gone.

## Options

- Delete the block (and refuse the key with a hint), since no road applies it.
- Or wire it to a road that can carry it, per `canonical-build-path.md`'s
  "which roads carry it" question — cargo `[env]`, cmake, west each need their
  own carrier.

Either way, a key that parses must not be a silent no-op.
