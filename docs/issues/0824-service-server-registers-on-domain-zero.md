---
id: 824
title: "A service server registers its queryable and liveliness tokens on domain 0
  while the same image's node token is on the configured domain"
status: open
type: bug
area: rmw
related: [issue-0801, phase-390]
---

## Problem

`examples/zephyr/c/service-server` on mr_canhubk3/s32k344, built with
`CONFIG_NROS_DOMAIN_ID=10`, zenoh over serial. The board boots, prints
`Waiting for service requests`, and the router shows its serial transport up —
but the host on domain 10 never sees the service:

```console
$ ros2 node list        # empty
$ ros2 service list     # only /… /parameter_* — no /add_two_ints
$ ros2 service call /add_two_ints example_interfaces/srv/AddTwoInts "{a: 3, b: 4}"
waiting for service to become available...
```

## The declared keys say why

From the board's own zenoh debug (`-DNROS_ZENOH_DEBUG=3`, read over RTT):

```
@ros2_lv/10/2aad34b9…/0/0/NN/%/%/node                              <- domain 10
Allocating queryable for (0/add_two_ints/example_interfaces/srv/AddTwoInts/…)   <- domain 0
@ros2_lv/0/2aad34b9…/0/0/NN/%/%/add_two_ints_server                <- domain 0
@ros2_lv/0/2aad34b9…/0/3/SS/%/%/add_two_ints_server/%add_two_ints/…  <- domain 0
```

**Three of the four are on domain 0.** The boot executor's own node token
(`node`) gets the configured 10; everything the service registration emits —
the component's node token, the queryable, and the `SS` liveliness token —
gets 0. A host on domain 10 cannot match any of them.

## Contrast with the talker, which is correct

`examples/zephyr/c/talker`, same board, same config, same transport, all four
keys on domain 10:

```
@ros2_lv/10/05b83c0c…/NN/%/%/node
@ros2_lv/10/05b83c0c…/NN/%/%/talker
@ros2_lv/10/…/MP/%/%/talker/%chatter/std_msgs::msg::dds_::String_/…
Allocating interest for (10/chatter/std_msgs::msg::dds_::String_/…)
```

So this is not a board or transport problem, and not a general domain-plumbing
problem: pub/sub is right and services are not, in the same image shape.

## Root cause found and half-fixed (2026-08-27)

All **seven** `ServiceInfo::new` sites in
`packages/core/nros-node/src/executor/spin.rs` set `.with_namespace()` and
`.with_node_name()` and **never** `.with_domain()`. The identical defect
[issue 0801](archived/0801-domain-id-split-across-entities.md) fixed for
`TopicInfo`, in the service path. `executor/action.rs` already had it right on
all three of its `ServiceInfo`s, so these were simply missed.

Fixed. All four keys are now on domain 10 and `ros2 node list` reports
`/add_two_ints_server` where it previously reported nothing.

### The false trail, recorded so it is not re-walked

nros-cpp's `nros_cpp_service_server_create` *does* carry
`.with_domain(ctx.domain_id)`, and so does the publisher — which is why pub/sub
worked and made the whole C++ layer look innocent. The example calls
`nros_cpp_service_server_register`, a **different entry point** at
`service.rs:176` that delegates to `Executor::register_service_raw_sized*` and
lets the executor build the `ServiceInfo`. That is the path with no domain on
it. Two rebuild-and-flash cycles were spent instrumenting the wrong function
before a probe that never fired showed the live path was elsewhere.

## STILL BROKEN: the service type name is not DDS-mangled

`ros2 service list` still shows nothing and calls still hang. Compare the keys
now that the domain is right:

```
service  …/SS/%/%/add_two_ints_server/%add_two_ints/example_interfaces/srv/AddTwoInts/…
topic    …/MP/%/%/talker/%chatter/std_msgs::msg::dds_::String_/…
```

The topic type is mangled to the DDS form (`::msg::dds_::`); the **service type
is not**, so it reaches the key as `example_interfaces/srv/AddTwoInts` — and
`/` is a zenoh keyexpr **separator**. The service type therefore splits the key
into three extra segments and cannot match what rmw_zenoh expects at those
positions.

Expected shape is presumably `example_interfaces::srv::dds_::AddTwoInts_`,
matching how message types are handled, but that has NOT been confirmed against
rmw_zenoh's own parser — do that before fixing, rather than assuming the
mangling rule is identical for srv.

## Where it is NOT

Read but not confirmed as the cause, recorded so the next person does not
re-walk it:

- `nros_cpp_service_server_register` (`packages/api/nros-cpp/src/service.rs:119`)
  does build its `ServiceInfo` with `.with_domain(ctx.domain_id)`, so the call
  site is not simply missing the domain the way
  [issue 0801](archived/0801-domain-id-split-across-entities.md) was.
- `nros_cpp_init` writes `ctx.domain_id` from the resolved `BootConfig`, and
  `zephyr_run_tiers.c:513` passes the configured domain into it — which is
  consistent with the `node` token correctly showing 10.

Which leaves the question this issue exists to answer: **why is
`ctx.domain_id` 0 at service-registration time when the same context resolved
domain 10 at init?** The prime suspects are that the service path reads a
different `CppContext` than the one `nros_cpp_init` stamped (there are THREE
`CppContext` constructors, and the third one's own comment warns that fields
get missed there — `lib.rs:2340`), or that `create_node` hands the component a
node whose executor pointer is not the boot context.

Cheapest next step is not more reading: print `ctx.domain_id` at
`service.rs:119` and at `nros_cpp_init`'s write, and compare.

## Impact

Services are unusable over any non-zero domain. Pub/sub on the same image is
fine, so this is invisible until someone tries a service — which on this board
was blocked behind
[issue 0821](archived/0821-zenoh-pico-faults-at-lease-expiry-on-zephyr.md)
until it was fixed, which is why it surfaced only now.
