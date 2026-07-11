---
id: 177
title: "native C cyclonedds fixture fails to link — duplicate register_<Type>_0 symbols across std_msgs & example_interfaces ts libs"
status: open
type: bug
area: cyclonedds
related: [issue-0138, issue-0175]
---

## Summary

The `fixture-native-c-cyclonedds` fixture (native C Cyclone lane, built by
`just build-test-fixtures` → `native`) **fails to link** with a wall of
multiple-definition errors — the Cyclone IDL typegen (`idlc`) `register_<Type>_0`
functions are defined in **both** `libstd_msgs__cyclonedds_ts.a` and
`libexample_interfaces__cyclonedds_ts.a`:

```
/usr/bin/ld: libstd_msgs__cyclonedds_ts.a(Int32_register_0.c.o): in function `register_Int32_0':
Int32_register_0.c:(.text+0x0): multiple definition of `register_Int32_0';
  libexample_interfaces__cyclonedds_ts.a(Int32_register_0.c.o):...: first defined here
… (Int32MultiArray, Int64, Int8, MultiArrayDimension, MultiArrayLayout, String,
   UInt16, UInt32, UInt64, UInt8, and the *MultiArray variants — all duplicated)
collect2: error: ld returned 1 exit status
ninja: build stopped: subcommand failed.
make[1]: *** [fixture-native-c-cyclonedds] Error 1
```

This blocks the whole `native` stage of `build-test-fixtures` (fails with
`build-fixture-extras` rc=2), which in turn blocks the `test-all` e2e lane's
fixture-staleness gate.

## Root cause (suspected)

`example_interfaces`'s generated Cyclone typesupport archive
(`libexample_interfaces__cyclonedds_ts.a`) carries the **std_msgs** primitive
message types (Int32/String/UInt*/MultiArray…) in addition to its own — the
same `register_<Type>_0` TUs that `libstd_msgs__cyclonedds_ts.a` already
provides. When a fixture links both archives, the common register TUs collide.

Same class as archived **#0138** (threadx-riscv64 `--allow-multiple-definition`)
and the cyclone typegen dedup concerns.

## Reproduce

```
just build-test-fixtures        # native stage
# or, narrower, the native cyclonedds fixture target that the driver invokes:
#   fixture-native-c-cyclonedds  → ninja link step
```

## Fix direction (needs a decision)

1. **De-duplicate the typegen** — only one archive should emit the shared
   std_msgs primitive `register_<Type>_0` TUs (generate example_interfaces'
   ts against std_msgs as an *external* dependency, not a bundled copy).
2. **Link with `-Wl,--allow-multiple-definition`** for the native cyclone
   fixture (the #0138 escape hatch) — unblocks the link but keeps two copies.
3. **Split the archives** so the fixture links only one provider of each
   register symbol.

Not caused by the concurrent #175 Cyclone descriptor work (that touches
`descriptors.{cpp,hpp}` / `service.cpp`, not the ts codegen/archive layout).

## Notes

- Surfaced while rebuilding all fixtures for the RTIC e2e run (see #176). The
  `native` stage was already broken on trunk independent of #167 / the RTIC
  Send fix.
