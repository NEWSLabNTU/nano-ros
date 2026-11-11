---
id: 2433
title: "An owned C/C++ entry on a cross board gets neither its census nor its image's deploy, domain and locator on the workspace road"
status: open
type: limitation
severity: medium
area: [cli, cmake, sizing, census]
related: [1556, 1946, 2406, phase-531]
found: 2026-11-11
---

**Home: [phase-531](../roadmap/phase-531-workspace-first-project-layout.md)
W6.** Found migrating the C/C++ talker witnesses on cross boards
(`threadx-linux`, `mps2-an385-freertos`, `qemu-armv7a-nuttx`, `rv-virt-nuttx`,
`rv-virt-threadx`) the way the linux C/C++ talkers went (owned entry, RFC-0071
D19). Those witnesses need not take that road: RFC-0122 D10 keeps a
hand-written `main` only in a manual-entry demo, and phase-535 W1's inventory
lists none of them, so they become components on a generated entry, which has
a model and a census. What stays open is the road itself: any owned C/C++
entry on a cross board (a manual-entry demo that later migrates in place, or
a user's) meets both gaps below.

## Measured

2026-11-11, migrating `examples/threadx-linux/{c,cpp}/talker` with
`nros dev migrate-bringup`, keeping each hand-written `main` as an owned entry
built by two images (`threadx-linux` zenoh, `threadx-linux-cyclonedds`).

1. **No deploy, domain or locator.** The generated CMake root adds the owned
   package as `pkg/<name>/` and states none of the image's facts, so the
   package's own `nano_ros_add_executable` takes `NROS_HOST_DEPLOY`. On an
   embedded board that leaves the locator/domain bake inactive: the Cyclone
   image's compile line carried no `NROS_ENTRY_DOMAIN_ID` (the bringup states
   107; the paired listener in `linux_api.rs` runs on 107), and the zenoh image
   baked `tcp/127.0.0.1:7447` where the image states `:9100`. A leaf got these
   from its `system.toml` and the fixture row's `-D`.
   A fix was written and measured (not merged): the root sets `NROS_DEPLOY`,
   `NROS_DOMAIN_ID` and `NROS_ENTRY_LOCATOR` as plain variables for its owned
   entries, refusing owned images on one configure that disagree. After it the
   compile lines read `-DNROS_ENTRY_DOMAIN_ID=107` / `=108` and the 9100 / 9200
   locators. Kept on the local branch `phase-531-w6-threadx-linux-cc-wip`.
2. **No census.** A C/C++ cross leaf is census-sized by default (RFC-0120 D4,
   `census-leaves`): its configure takes `nros dev image entity-census take
   --leaf` over a host build of the same `main`. An owned entry's bringup has
   no `[[component]]` rows, so no launch file and no model; the workspace
   fixture pre-pass lists the image (`census-images`) and `take --plan` fails:

   ```
   Error: cannot resolve the SystemModel of `threadx_c_talker_bringup:threadx-linux`:
   cannot resolve `config/system_model.yaml`: its launch file
   `.../build/nros/launch/threadx_c_talker_bringup/launch/system.launch.xml` does not exist
   ```

   `nros build <image>` itself succeeds (nothing on the workspace road
   requires the census of an owned entry), so the image builds at the crate
   default pool sizes where the leaf built census-derived ones (the leaf's
   census is what states its entity counts to the sizing descriptor,
   phase-530's measurements on `threadx-linux/c/talker`).

## Open question

How an owned C/C++ entry on a cross board is sized on the workspace road:

- run the leaf's census over the owned package (a host build of its `main`,
  issue 1556), keyed by image rather than by leaf; or
- declare the entities in the bringup some other way (the `[[component]]`
  rows that would make it generated, phase-531 finding 4); or
- accept the crate defaults for these witnesses and make the pre-pass skip an
  image with no model (`take --plan` answering "nothing to reconcile").

The talker witnesses do not wait on it (D10, above); the Rust witnesses are
unaffected either way.
