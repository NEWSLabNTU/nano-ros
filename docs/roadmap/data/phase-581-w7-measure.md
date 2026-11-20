# Phase-581 W7 - measurements for the demand and pool split

Measured 2026-11-20 in the `ros2` distrobox (Ubuntu 22.04, ROS 2 Humble,
Zephyr 3.7), per [phase-581 W7](../phase-581-ros-env-vars.md). Both images are
the C talker (`nros_zephyr_talker_c_bringup:zephyr_zenoh`) on
`native_sim/native/64`, built with `just zephyr build-one`.

- **derived**: the in-tree example, unchanged. It declares one publisher and
  no subscription, and states no entity count.
- **stated 0**: a copy of the same example outside the checkout whose image
  adds `[image.zephyr_zenoh.knobs.entity] max_publishers = 0` (the copy was
  built through `build-one`'s example path).

## The demand in `.config`, the pool in the build

| | derived | stated 0 |
| --- | --- | --- |
| `CONFIG_NROS_ENTITY_MAX_PUBLISHERS` | `-1` (derive) | `0` |
| `CONFIG_NROS_ENTITY_MAX_SUBSCRIBERS` | `-1` | `-1` |
| `CONFIG_NROS_ENTITY_MAX_QUERYABLES` / `_LIVELINESS` / `_LARGE_SUBSCRIBERS` | `-1` | `-1` |
| `CONFIG_NROS_ENTITY_MAX_PENDING_GETS` | `4` | `4` |
| resolved demand, publishers / subscribers (`NROS_RESOLVED_NROS_ENTITY_MAX_*`) | 1 (derived) / 0 (derived) | **0 (stated)** / 0 (derived) |
| resolved pool, publishers / subscribers (`NROS_RESOLVED_NROS_ZENOH_MAX_*`) | 1 / **1** | **1** / **1** |
| `-DZPICO_MAX_PUBLISHERS` / `_SUBSCRIBERS` in `build.ninja` (369 compile lines each) | 1 / 1 | 1 / 1 |
| `zpico-sys` `shim_constants.rs` (`ZPICO_MAX_PUBLISHERS` / `_SUBSCRIBERS`) | - | 1 / 1 |

No `.config` holds a `CONFIG_NROS_MAX_*` or `CONFIG_NROS_ZENOH_MAX_*` symbol:
the demand is the only Kconfig name, and the pool has none.

Issue 1015's case, twice over: a declared demand of **0 subscribers** (derived,
the stock talker) and a stated demand of **0 publishers** each become a zenoh
pool of **1**, the floor applied at the pool (`_nros_c_array_pool_floor` in the
module; the C array in `zpico.c` and the Rust constant agree). The stated-0
build also warns, because the demand it states is below what the image's own
inventory derives:

```
nros: NROS_ENTITY_MAX_PUBLISHERS=0, stated by Kconfig
(CONFIG_NROS_ENTITY_MAX_PUBLISHERS in a .conf fragment or -D), is BELOW the
1 this image's entity inventory derives (1 = 1 publishers + 0 action-server
publishers + 0 runtime publishers + 0 contract reporters).
```

The derived build's configure log names the demand rows:

```
nros: NROS_ENTITY_MAX_SUBSCRIBERS=0 (0 = 0 subscriptions + ...) DERIVED from this image's entity inventory
nros: NROS_ENTITY_MAX_PUBLISHERS=1 (1 = 1 publishers + ...) DERIVED from this image's entity inventory
nros: NROS_ENTITY_MAX_QUERYABLES=1 (1 = ... + 1 transient-local cache queryables + ...) DERIVED ...
nros: NROS_ENTITY_MAX_LIVELINESS=3 (3 = 2 node tokens + 1 publishers + ...) DERIVED ...
nros: NROS_ENTITY_MAX_LARGE_SUBSCRIBERS=0 DERIVED from this image's message-bound inventory
```

## A stated demand above the derivation

The island-shaped workspace's Zephyr image states
`knobs.entity.max_publishers = 6` and `knobs.entity.max_subscribers = 6`
(was `knobs.zenoh.*`). Rebuilt live (`tests/island-shaped-zephyr-tests.sh
--regen`), its cache resolves the demand to 6 and the pool to 6 on both, and
`check-knob-delivery` over the rewritten snapshot reports every knob reached
the compile as resolved.

## Gates

`check-c-array-pool-floors` (31 knob-sized C arrays: 27 guarded, 4
zero-legal, 0 unclassified) and `check-c-array-guard-probe` pass with no edit
to either gate. `check-config-names` passes with a baseline of 6 (was 12): the
six zenoh pool rows left it; the six left are A.3's `ZENOH_*` variables, which
phase-581 W13 retires.

## Not measured

The cargo and cmake roads off Zephyr carry a stated demand to its pool in the
CLI (`carry_demand_to_pools`); that is held by unit tests
(`a_stated_demand_is_carried_to_its_pool`, the board-rung tests), not by a
built image here.
