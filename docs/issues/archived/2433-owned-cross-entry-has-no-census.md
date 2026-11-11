---
id: 2433
title: "An owned C/C++ entry on a cross board gets neither its census nor its image's deploy, domain and locator on the workspace road"
status: resolved
type: limitation
severity: medium
area: [cli, cmake, sizing, census]
related: [1556, 1946, 2406, phase-531]
found: 2026-11-11
resolved: 2026-11-11
resolved_in: "an owned C/C++ entry on a cross board is sized by a census of the entry per image; the generated root states its deploy, domain and locator"
---

> **Resolved (2026-11-11).** Maintainer ruling: census the owned entry, per image. See [Resolution](#resolution).

**Home: [phase-531](../../roadmap/phase-531-workspace-first-project-layout.md)
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


## Resolution

**Maintainer ruling 2026-11-11.** A C/C++ entry that a migrated image owns
on a cross board is sized by CENSUSING THE ENTRY, once per image. The census
works the way issue 1556 censuses a native C/C++ application: a host build of
the entry, sized from what it registers. Two alternatives were not taken:
declaring the entities in the bringup, and accepting the defaults. The cross
C/C++ talker witnesses take RFC-0122 D10's road instead and become
components. `examples/threadx-linux/c/talker` is the exception, because
`linux_api`'s SIGTERM and exit-status cases exist for its hand-written
`main`. It is the owned entry this issue's road serves.

Implemented in three parts:

1. **Deploy, domain, locator.** For each cross configure that builds an owned
   entry, the generated CMake root sets `NROS_DEPLOY`, `NROS_DOMAIN_ID` and
   `NROS_ENTRY_LOCATOR` as plain variables. These are the values a leaf's
   `system.toml` and `-D` used to supply. If the owned images on one configure
   disagree on them, the build refuses.
2. **The census of the entry.** `nros dev image entity-census take --image
   <bringup>:<image>` takes the census when the image's program is an owned
   CMake entry.
   - It writes `<ws>/build/nros/census/owned/<bringup>/<image>.census.json`.
   - The host build goes in `build/nros/census-host/<bringup>/<image>`.
   - The host build and the run are `census_host_program`, which `take --leaf`
     now shares, so there is one way to build a census.
   - `--plan` prints an `owned-entry` line, so the fixture pre-pass takes this
     census as it does any image's.
3. **Sizing from it.** `sizing-descriptor --from-leaf <bringup> --leaf-image
   <image> --owned-entry <pkg>` builds the sizing descriptor.
   - The bringup states the deployment. The census states every entity, and
     its nodes stand in for components (Ruling R29's shape).
   - A missing or stale census refuses, naming the take.
   - The generated root calls `nros_record_owned_entity_facts` before the
     packages are added. That call takes the census, then writes the
     descriptor and the entity fragment before any reader runs.

Measured on the migrated `examples/threadx-linux/c/talker`:
- The census reads 1 node, 1 publisher and 1 timer.
- `entity_inventory.cmake` is `derived` (`PUBLISHER 1`, `TIMER 1`).
- The compile lines carry `-DNROS_ENTRY_DOMAIN_ID=107` on the Cyclone image and
  `tcp/127.0.0.1:9100` on the zenoh image.
- The zenoh image publishes to a router.
