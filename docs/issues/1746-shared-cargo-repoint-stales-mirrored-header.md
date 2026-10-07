---
id: 1746
title: "A re-configure that re-points the shared cargo dir twice leaves a native C++ leaf's mirrored sizes header from the WRONG key, and its first build links `undefined reference to nros_config_variant_sz_*`"
status: open
type: bug
area: [build, cmake]
severity: medium
found: 2026-10-07
related: [0088, 0268, 0834, 0945, 0987, 1729]
---

## What

Found while proving issue 1729: an edit to a cmake module re-configures every
native leaf, and on the next `just build-test-fixtures lane=tier1` the native
stage failed on `examples/native/cpp/talker/build-{zenoh,xrce,cyclonedds}` —
one per run, deterministic, never a race:

```
/usr/bin/ld: CMakeFiles/cpp_talker.dir/src/main.cpp.o:(.data.rel.ro+0x8):
  undefined reference to `nros_config_variant_sz_bd634ed6d9ad21fb'
```

`libnros_cpp.a` defines `nros_config_variant_sz_c8b5e57577881998`
(`NROS_EXECUTOR_SIZE 20592`, sized from the leaf's sizing descriptor); the
mirrored `<build>/nano_ros/packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h`
said `NROS_EXECUTOR_SIZE 90576` — the crate default, i.e. a build with NO
descriptor — and was newer than the archive, so the restat'd mirror rule did
not run and `main.cpp.o` recompiled against it.

## The lead

The configure log of the failing dir prints the shared-cargo link flipping
TWICE in one configure (`cmake/NanoRosCorrosion.cmake`,
`nros_share_corrosion_cargo_dir`):

```
-- nano-ros: shared-cargo key changed, re-pointing <build>/cargo
  was: ...|NROS_DECLARED_INFRA_QUERYABLES=none|NROS_DECLARED_NODES=1|NROS_SIZING_DESCR...
  now: ...|profile=release|target=x86_64-unknown-linux-gnu
...
-- nano-ros: shared-cargo key changed, re-pointing <build>/cargo
  was: ...|target=x86_64-unknown-linux-gnu
  now: ...|NROS_DECLARED_INFRA_QUERYABLES=none|NROS_DECLARED_NODES=1|NROS_SIZING_DESCR...
```

The key is computed once before the descriptor knobs are known and once after,
so for a window of the configure the leaf points at the no-descriptor key's
directory, whose header is the 90576 one. Not yet traced: which step copies
the header in that window. A second plain `ninja -C <dir>` converges (the
cargo step re-runs and the mirror follows), which is why it reads as a flake.

## Acceptance

The key is computed once (after the knobs it names are known), or the mirror
has a real edge on the key, so a re-configured leaf's FIRST build links. A
regression test re-configures a native C++ leaf and builds it once.
