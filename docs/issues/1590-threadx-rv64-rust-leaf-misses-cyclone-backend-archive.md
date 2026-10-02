---
id: 1590
title: "A `threadx_riscv64` RUST leaf built with `-DNROS_RMW=cyclonedds` links no
  Cyclone backend archive, so the generated typesupport's registration
  constructor has no `nros_rmw_cyclonedds_register_descriptor` to call"
status: open
type: bug
area: build, cmake, rmw, boards, threadx
severity: high
found: 2026-09-30
related: [1355, 1467, 0425, 1557, 1158]
---

## What happens

Nightly run **36682994178** (schedule, 07:19), job **109783090600**
(`threadx_riscv64`), step 12 `Build (threadx_riscv64)`. Six of the twelve rust
leaves fail, and all six are the Cyclone leg:

```
  ThreadX-RV64 rust leaves: 6/12 ok, 6 failed
```

| leg | leaves | result |
| --- | ---: | --- |
| `-DNROS_RMW=zenoh` (`build-zenoh/`) | 6 | all six link |
| `-DNROS_RMW=cyclonedds` (`build-cyclonedds/`) | 6 | all six fail at link |

Every one of the six fails on exactly one symbol, and on nothing else — the
string `undefined symbol:` occurs six times in the 42,255-line log and names the
same symbol each time:

```
rust-lld: error: undefined symbol: nros_rmw_cyclonedds_register_descriptor
>>> referenced by String_register_0.c
>>>               String_register_0.c.obj:(register_std_msgs_String_0_constructor)
>>>               in archive .../rust/talker/build-cyclonedds/libstd_msgs__cyclonedds_ts.a
ninja: build stopped: subcommand failed.
```

## Why it is a link-line finding and not a missing definition

The symbol is defined, in C++, in
`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/descriptors.cpp:282`
(`extern "C" void nros_rmw_cyclonedds_register_descriptor(...)`), and the
generated register TU is emitted with a matching `extern` declaration by
`NrosRmwCycloneddsTypeSupport.cmake:602` and by the CLI's
`codegen_cyclonedds_descriptors.rs:324`. Both halves are present in the tree.

What is absent is the archive that carries the definition. The failing link
command in the log is, in full order:

```
... -Wl,--whole-archive .../libstd_msgs__cyclonedds_ts.a -Wl,--no-whole-archive
    <5 .obj files>
    -o riscv64_threadx_rust_talker
    libstd_msgs__nano_ros_c.a
    librv_virt_threadx_talker.a
    libstd_msgs__cyclonedds_ts.a
    nano_ros/libthreadx_glue.a
    nano_ros/libvirtio_net_netx.a
    -lc
    nano_ros/nros_platform_threadx/libnros_platform_threadx.a
    nano_ros/libnetxduo.a
    nano_ros/libthreadx_kernel.a
    -lnosys -lgcc
```

There is no `libnros_rmw_cyclonedds*` on it, and no `libnros_cpp.a` either. The
typesupport archive that REFERENCES the symbol is whole-archived onto the line;
the backend that DEFINES it is not on the line at all.

## What this is NOT

- Not issue **1355**'s ThreadX port selection. `ports/linux/gnu` and
  `semaphore.h` do not appear in this log, the riscv port builds
  (`libthreadx_kernel.a` links), and six leaves link all the way through.
- Not issue **1355**'s `nros/app_config.h` stop either. That was fixed on main
  by `9d0393bfd5` and verified on a real riscv64 build (1355, phase-472 F5);
  `app_config.h` does not appear in this log's failures, and the 0/12 tally it
  was measured on is now 6/12.
- Not issue **1459** (`nros_board_network_wait`) — a different symbol, and that
  one is on the C entry, not a rust leaf.
- Not a codegen defect. The register TU is well formed and its `extern`
  declaration matches the definition's signature; the object is in the archive
  the linker names.

## The hypothesis worth testing first, and why it is only a hypothesis

`descriptors.cpp` is a C++ translation unit inside the Cyclone backend, and
issue **1467** (resolved 2026-09-27) stopped the generated C interface library
propagating the C++ umbrella to a binary that carries its own Rust staticlib —
exactly the shape these rust leaves have. If the umbrella was the only thing
putting a Cyclone C++ archive on a rust leaf's line, then removing it removed
this symbol's only provider, and the zenoh leg is green because it has no C++
descriptor registry to lose. That would make this the Cyclone-leg half of
1467's fix, unmeasured at the time because 1467 was verified on
`-DNROS_RMW=zenoh` (its own entry says so: "measured, `-DNROS_RMW=zenoh`").

It is a hypothesis because nothing here has been bisected against 1467's
commit. Whoever picks this up should establish it, or rule it out, before
changing a link line.

## Where the fix probably belongs, and the rule it must respect

`nros_link_runtime_umbrella(<t> <scope>)` and `nano_ros_link_rmw` are the two
places that decide what a leaf links. Issue **0425**/**1467**'s rule governs any
change here: a propagated usage requirement's condition must be a property of
the CONSUMER, never of the build tree — so the Cyclone backend must reach a
rust leaf because that leaf declares the Cyclone RMW, not because a target
happens to be defined. Putting the umbrella back would re-open 1467's 228
duplicate symbols.

Also relevant, and not to be re-learned the hard way: a library reached through
a raw `-Wl,` flag gets no rebuild edge (issue **0475**), so whatever archive is
added needs `LINK_DEPENDS` on the consuming target and must not be appended in
a way that reorders ld's single pass through the whole-archive group.

## What would close it

1. The six `-DNROS_RMW=cyclonedds` rust leaves of `examples/rv-virt-threadx/rust/*`
   linking, with the Cyclone backend reaching the line by a consumer-side
   condition.
2. A statement of whether 1467's fix is the cause. If it is, the same question
   must be asked of every other Rust-staticlib leaf on a Cyclone build, not just
   ThreadX/riscv64 — this lane is simply the one that builds them nightly.
3. Not closed by the tally reaching 12/12 alone: issue **1355** owns the lane's
   verdict and stays open for the cells.

## It is not nightly-only — tier 2 carries the same six (2026-10-02)

Filed from the nightly `threadx_riscv64` job. The **tier-2 1-wise matrix** fails
on it too, so the defect is in the lane's build and not in the nightly's
coordinates.

- nightly **36829686786** (schedule 07:19:46Z, `c7db50ad6`), job
  **110263557235** `threadx_riscv64`, step `Build (threadx_riscv64)`:
  `rust-lld: error: undefined symbol: nros_rmw_cyclonedds_register_descriptor`,
  `ThreadX-RV64 rust leaves: 6/12 ok, 6 failed`.
- run-matrix **36825211926** (schedule 06:31:16Z, `bc615cb84`), job
  **110249254587** `tier 2 (1-wise matrix)`, step `just build tier2`: the same
  symbol, **six** times (log lines 12259, 12382, 12503, 12626, 12749, 12871),
  then `error: recipe \`build-fixture-extras\` failed with exit code 1`.

Same count, same symbol, two different lanes and two different heads. Worth
noting for 1158's axis as well: this tier-2 run **reached the build stage** —
its failure is a link error, not a provisioning one — so a `failure` on
`run-matrix` is not automatically the never-reached-the-cells shape.
