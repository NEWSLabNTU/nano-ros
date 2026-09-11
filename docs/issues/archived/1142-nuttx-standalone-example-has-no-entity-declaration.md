---
id: 1142
title: "A standalone (non-workspace) example declares no entities, so the guessed
  fallback budget is the only budget it can ever get — NuttX has no channel at all"
status: resolved
resolved_in: phase-412 W5 (2026-09-11)
type: tech-debt
area: [rmw, memory, build]
related: [1028, 0827, 1061, 0460, phase-412, 1301]
---

## Resolution (2026-09-11)

**A standalone CMake leaf now sizes its pools from its own declaration, and the
NuttX C++ action-client's `SERVICE_BUFFERS` went 35,584 B → 4,448 B.**

RFC-0098 D8 had already decided WHERE such a leaf states its surface —
`entities = [...]` on its `system.toml` `[[component]]`, the
`EntityDecl::parse` grammar, the same reader every other road uses. Three things
were missing, and two of them were carriers rather than derivations:

1. **The declaration.** All twelve `examples/qemu-armv7a-nuttx/{c,cpp}/*` leaves
   now declare, each list READ FROM ITS OWN `src/` (the action-client creates one
   node and one action client on `/fibonacci`, so its application queryable count
   is a real ZERO). `[system] features` is the infrastructure half — the same key
   a bringup carries, reaching a resolved model as `execution.features` — read
   through one predicate (`InfraServices::from_features`, which
   `InfraServices::from_model` now delegates to).
2. **Reading it at configure time.** `nros ws entity-facts --leaf <dir>` applies
   the MODEL road's counting rule (a service server is one queryable, an action
   server is three, either client is none) and prints the same three
   `KEY=VALUE` facts. `cmake/NanoRosLeafEntityFacts.cmake`'s
   `nros_record_leaf_entity_facts()` — called from `nano_rosConfig.cmake` right
   after `nano_ros_read_leaf_system()` — folds them through
   `nros_fold_entity_facts`, the model road's own fold, extracted for the
   purpose.
3. **A carrier that reaches this lane.** THIS is what made the measurement
   possible, and it was not in the issue's diagnosis. `nros_entity_facts_env`
   delivers through `corrosion_set_env_vars`, armed on `nros_cpp-static` /
   `nros_c-static` — and the root CMakeLists does not add those subdirectories
   for NuttX at all (no `-Z build-std` through Corrosion). The NuttX image is
   linked by one env-wrapped `cargo build` of its own in
   `packages/api/nros-c/cmake/nros-nuttx.cmake`, so the facts were composed
   correctly and delivered to nothing. `nros_entity_facts_env(<t> ENV_OUT <var>)`
   returns the SAME composition for that lane to splice in; only the carrier
   differs.

### Measured

`examples/qemu-armv7a-nuttx/cpp/action-client`, `build-zenoh`, arm-none-eabi
13.2.1, `nros-minsizerel`, the fixture row's own `-D` defs:

| | `SERVICE_BUFFERS` | image `.bss` |
| --- | --- | --- |
| leaf declares nothing (the state this issue describes) | 35,584 B (`0x8b00`) | 508,208 B |
| leaf declares its one action client | **4,448 B** (`0x1160`) | 467,248 B |
| delta | **−31,136 B (−87.5 %)** | **−40,960 B** |

The `.bss` delta exceeds the `SERVICE_BUFFERS` one because the C shim's
per-session queryable table is sized from the same knob. 35,584 / 8 = 4,448
exactly: the guess was 8 slots, the declaration is 1.

The configure line is the tell, as this issue said. Without the declaration:

```
nano-ros: …/system.toml declares no entities, so the queryable table keeps the
backend's FALLBACK budget (issue 1142). To size it from the declaration, add
`entities = [...]` to its `[[component]]` …
```

With it:

```
nano-ros: queryable table sized from the declaration — infrastructure none,
0 declared service server(s) (phase-392 W5)
```

The first line is new. `nros_entity_facts_env` stays silent when no facts were
seen — by design — so before this, the fallback deciding and the declaration
deciding were indistinguishable in the log.

### What a leaf that declares nothing gets

Exactly what it got before: the backend's fallback, `if hosted { 32 } else { 8 }`.
The verb ABSTAINS rather than reporting zeros — an entity list is what opts a
leaf into stating its own surface, and without one an infrastructure answer would
be a claim about a hand-written `main` nobody described. The difference is that
the configure now SAYS the fallback decided, and names the key to add.

Everything delivered is `NROS_DECLARED_*`, which the build script reads as a
DEFAULT: a leaf that states `ZPICO_MAX_QUERYABLES` still wins, and a stated value
below the derived floor is refused at build time rather than silently lowered
(`check_queryable_override`). phase-412's ladder is unchanged.

### Not closed by this, deliberately

* The six `examples/qemu-armv7a-nuttx/rust/*` leaves still declare nothing. Their
  channel is not missing — issue 1061's `system.toml` `entities` reaches them
  through `nros sync`'s `[env]` sidecar — they simply have not filled it in, and
  the sidecar withholds `ZPICO_MAX_QUERYABLES` anyway
  (`NOT_DERIVED_NEEDS_INFRA_COUNT`), so the queryable budget this issue is about
  would not move for them without RFC-0098 D7. Filling them in is a chore under
  1061 and needs a build to verify, which this wave did not run for the Rust half.
* **Issue 1301** — the same NuttX link lane delivers no BOARD facts either, and
  `check-board-facts-delivery` cannot see it: the gate reads `cmake/**` and
  `zephyr/cmake/**`, and that lane is in `packages/api/nros-c/cmake/`. Found one
  line away from this fix.

## What

phase-392 W5's end state is that **every image declares its entities and the
fallback budget stops mattering**. Three delivery channels exist today and a
NuttX standalone example is in the gap between all three:

| channel | who it serves | mechanism |
| --- | --- | --- |
| CMake workspace | a workspace entry with a resolved SystemModel | `nros ws entity-facts` → `cmake/NanoRosEntityFacts.cmake` → `corrosion_set_env_vars` |
| Zephyr | any Zephyr image | `zephyr/Kconfig` `CONFIG_NROS_MAX_QUERYABLES` default `-1` (the DERIVE sentinel) → `nros_cargo_build.cmake` |
| cargo leaf | a probeable cargo leaf, or one that declares (issue 1061) | `nros sync` → `<leaf>/metadata/<component>.json` → `leaf_entity_env.rs` → the leaf's `[env]` sidecar |

`examples/qemu-armv7a-nuttx/cpp/action-client` is none of them. It is a standalone
copy-out CMake project (`find_package(nano_ros)` + `nano_ros_add_executable`),
so:

* it has no bringup and no SystemModel, so `nros_record_entity_facts` returns
  early — by design, and `nros_entity_facts_env` then says nothing rather than
  warning, because "a configure whose models are not resolved yet has always
  sized the table from the backend's own default";
* it is not Zephyr, so the Kconfig sentinel does not apply;
* it is not a cargo leaf, so the `[env]` sidecar does not apply.

So `NROS_DECLARED_SERVICE_SERVERS` and `NROS_DECLARED_INFRA_QUERYABLES` are
both absent, and `queryable_default_from` takes its last arm:

```rust
None => return if hosted { 32 } else { UNDECLARED_HEADROOM },
```

which is a guess by construction. Issue **1028** is what that guess costs when
the predicate feeding it is also wrong: 142,336 B of `.bss` on an image with
ZERO queryables. 1028 fixed the predicate. It did not — and could not — remove
the guess.

## Why this is its own wave

The declaration has to come from somewhere, and for this image shape there is
no "somewhere" yet:

* The **CMake** answer would be `nano_ros_node_register(... ENTITIES ...)` on a
  standalone target, which today only workspace members use. Making a
  copy-out example carry it is a user-facing API decision (RFC-0026 says these
  are standalone templates a user copies), not an internal plumbing change.
* The **cargo** answer for a NuttX leaf is issue 1061's
  `[package.metadata.nros.component] entities` — NuttX leaves are exactly the
  `*.json.unprobeable` case that mechanism exists for (a foreign `[build]
  target` with `[unstable] build-std` cannot be host-compiled by the probe).
  None of the six `examples/qemu-armv7a-nuttx/rust/*` leaves declares one today,
  so the Rust half of the platform is in the same gap for a different reason.
* Whichever is chosen, **the acceptance is a BUILD** — a measured
  `nm -S <elf> | grep SERVICE_BUFFERS` before and after — not a gate. That
  needs the NuttX toolchain plus a ROS install for `example_interfaces`, which
  is why it did not ride along with 1028.

## Confirm cheaply

For any candidate image, the configure line is the tell. A configure that has a
declaration prints:

```
nano-ros: queryable table sized from the declaration — infrastructure …, … (phase-392 W5)
```

Absence of that line means the fallback decided the budget.

## Related

* **1028** — the fallback's hosted/embedded predicate was wrong for NuttX
  (fixed; `target_os_is_hosted` + `check-rtos-target-os`).
* **0827** — deriving a cargo leaf's pool budgets from the probe.
* **1061** — a leaf DECLARES what the probe cannot read.
* **0460** — the failure direction: an 8-slot table against eleven
  infrastructure queryables, discovered at boot.
