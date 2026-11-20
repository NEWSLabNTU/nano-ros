---
id: 2782
title: "A C component's 41 declared parameters are refused by the image's 8-slot parameter store, and the refusal prints nothing: the image exits -6"
status: open
type: bug
area: [params, c-api, testing]
severity: high
found: 2026-11-21
related: [2780, 2777, 2640]
---

## What fails

`rmw_coordinate_truth::a_rows_entry_registers_its_nodes_at_runtime`, row
`workspace-probe-seeds` (`platform = "linux"`, cmake, mixed C/C++,
`packages/testing/nros-tests/fixtures/probe_seeds_ws`). It reproduces 3 of 3
solo on newslab-118 at main `008eee9312`, on a freshly built fixture, failing
in 0.8 s. It passed on the merge lane at `2cfe9f0094` (run 44090176782) and
failed at `b13a5d4b18` (run 44768834808).

The image prints, and then exits with code 250 (`-6`, `Full`), saying nothing
more:

```
[INFO] nros: session open (rmw=zenoh)
[INFO] nros: parameter store: 8 slots (68928 B) + state and services (28104 B) carved from the executor backing
```

## Where it stops

Under gdb (breakpoints on the entry's calls, then `exit`): the launch
seeds succeed, `rclcpp::create_node` succeeds, and
`__nros_c_component_seeds_c_pkg_configure` returns `-6`, which
`__nros_entry_setup` returns from `main`.

`seeds_c_pkg`'s `configure` runs an rclc parameter server with
`max_params = 41` and declares `ctrl_period_ms` plus 40 `gain_NN` integers.
The fixture says so itself: 40 gains are "more than a default parameter store
holds (32)". Issue 2640 built it so the metadata probe would record all 41
parameters, and the probe does.

So there are two defects:

1. **The carve ignores what the image declares.** The image's own metadata
   states 41 parameters on one node, but the store is carved at 8 slots. At
   `2cfe9f0094` the store held all 41.
2. **The refusal is silent.** The declaration past the carve is refused with
   `Full`, and nothing names the parameter, the slot count or the knob to
   raise. The commit that introduced the carve refusal (`f93e16805f`, "a
   declaration past the stock store carve is refused, naming the knob") does
   name it on another road; on this one (an rclc declaration from a C
   component), nothing is printed.

Ruled out: the stock node interfaces default (`6229085857`, issue 2777's
cause). With `node_interfaces = "none"` on the image, it fails the same way.

## Same family

Issue 2777 is a ThreadX image refusing a 283,392-byte parameter-store
allocation: a carve that does not follow from what the image declares,
reported only as an exit. The fix for both is with the phase-556 store-carve
work.

## Closes when

The probe-seeds row registers its nodes again on the merge lane, its store
sized from the declared parameter count, and a declaration past the store's
capacity on every road prints the parameter, the capacity and the knob.
