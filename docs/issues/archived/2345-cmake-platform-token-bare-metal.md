---
id: 2345
title: "A generated cmake root names bare metal `bare_metal`, which `nros_feature_set` refuses; bootstrap names it `baremetal`, which `setup.sh` refuses"
status: resolved
type: bug
area: [cli, cmake, bare-metal]
severity: medium
found: 2026-11-07
related: [1512, phase-531, phase-530]
---

**Home:** [phase-531](../../roadmap/phase-531-workspace-first-project-layout.md) — the road
`nros new` put a C bare-metal image on.

## Measured

Replaying the C Tutorial on `mps2-an385-baremetal` (`nros new sensor_ws --board
mps2-an385-baremetal --template talker --lang c`, then `nros build`) stopped at
configure:

```text
NanoRosFeatureSet.cmake:288: nros_feature_set: unknown PLATFORM 'bare_metal'
(expected: posix, freertos, ..., baremetal/bare-metal)
```

With that fixed, the same build stopped one step later, in the submodule
bootstrap of a checkout with an uninitialised submodule:

```text
tools/setup.sh: unknown platform 'baremetal'. --list-targets for known set.
```

## Cause

Two translations of one platform name, each a rule that held for the platforms
it was written against and not for bare metal:

- `builder/cmake_root.rs` derived the cmake token as the kebab form with `-`
  replaced by `_` (true for `threadx-linux` → `threadx_linux`). For
  `bare-metal` it produced `bare_metal`; `nros_feature_set` accepts
  `baremetal` and `bare-metal`. Unreachable until phase-531 W3/W4 (`nros new`
  writes a workspace) put a C bare-metal image on the generated root.
- `cmake/bootstrap.cmake` maps `NROS_PLATFORM` to a `setup.sh` platform tag
  and had no bare-metal arm, so the module token `baremetal` (issue 1512)
  reached `setup.sh`, which keys on `bare-metal`.

## Fix

- `PlatformKind::cmake_feature_set()` (`orchestration/board_descriptor.rs`) is
  the one cmake spelling of a platform kind, beside `kebab()` and
  `cmake_deploy()`; `cmake_root.rs` translates through it
  (`PlatformKind::from_kebab`) and refuses a token no kind has. Test
  `every_platform_kinds_cmake_token_is_one_nros_feature_set_accepts` reads the
  accepted set from `cmake/NanoRosFeatureSet.cmake`, so it cannot drift from
  it; `a_bare_metal_root_names_the_platform_nros_feature_set_accepts` fails
  with the old line restored (measured).
- `bootstrap.cmake` maps `baremetal` to `bare-metal`.

Sweep: `git grep -nE "replace\('-', ?\"_\"\)" -- 'packages/cli/**/*.rs' | grep -i plat`
(one site), and every cmake comparison of a platform token against a
bare-metal literal (`git grep -nE '(NROS_PLATFORM|_PLATFORM|_plat[a-z_]*)\b.*(STREQUAL|MATCHES).*bare'`):
only `nros_feature_set`, which accepts both. `image_facts.rs` writes
`NROS_IMAGE_PLATFORM`, which no build reads.

## Not fixed here

The C Tutorial on bare metal stays red on the next layer:
`nros dev codegen entry-pack --lang c --board mps2-an385-baremetal` refuses
the board — the cmake road has no bare-metal C entry. C on pure bare metal
needs a Rust link root (issue 1512), so every in-tree
`examples/mps2-an385-baremetal/c/*` leaf is a cargo leaf, while `nros new
--lang c` on that board scaffolds a cmake package. Owner: phase-531.
