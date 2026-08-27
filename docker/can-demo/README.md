# ROS 2 over CAN, in one container

A ROS 2 topic crossing a CAN bus to another ROS 2 node **and** to a zenoh-pico
peer, with no router and no TCP endpoint anywhere. Implements
[RFC-0082](../../docs/design/0082-a-demonstrable-can-stack.md) / phase-387.

```sh
sudo modprobe vcan                                    # once, on the host
docker/can-demo/run.sh --zenoh /path/to/zenoh-fork
```

The zenoh fork must be on `feat/can-link-ros`, which carries the CAN link on the
zenoh revision `rmw_zenoh` actually builds. `run.sh --help` lists the options;
`--negative` runs the control described below.

## Prerequisite: the vcan module, on the host

The container creates its own `vcan0` inside its own network namespace and needs
only `--cap-add=NET_ADMIN` — no `--privileged`, and no CAN interface on the host.
What it cannot do is load the kernel module, deliberately: that would need
privileges the demo does not take. So the host needs `vcan` available.

Debian and Ubuntu may need `linux-modules-extra-$(uname -r)` for it to exist.

## What it does

| peer | CAN identifier | role |
| --- | --- | --- |
| ROS 2 `talker` | `0x100` | publishes `/chatter` |
| ROS 2 `listener` | `0x101` | subscribes |
| zenoh-pico `z_sub` | `0x200` | subscribes, standing in for an MCU |

Both ROS sessions are configured with the `connect` endpoint removed and a CAN
endpoint as the **only** `listen` endpoint, so there is no path other than the
bus. The run asserts that the listener heard what the talker published, that the
pico peer heard it too, and that frames appeared on all three identifiers. It
exits nonzero if any of that fails.

## The control, which is what makes it evidence

```sh
docker/can-demo/run.sh --zenoh /path/to/zenoh-fork --negative
```

Puts the two ROS peers in **disjoint identifier bands**, so the CAN link's own
filter separates them, and asserts the listener hears **nothing** while both
peers keep transmitting. Without it, "the listener heard the talker" only tells
you they communicated — not that CAN carried it. With it, the same setup goes
silent when and only when the CAN filter is told to separate them.

## Two links, and which ROS semantics each one carries

The image carries **both** CAN links in one `libzenohc.so`, because the
interesting thing is the difference between them.

| | `can` (RFC-0080) | `isotp` (RFC-0083) |
| --- | --- | --- |
| shape | multicast | unicast, ISO 15765-2 |
| MTU | 64 (CAN FD frame) | 4095 |
| topics | yes | yes |
| services, actions, parameters, graph | **no** | **yes** |
| peers per link | the whole bus | one, a directed identifier pair |

A zenoh **multicast** face routes pushed data only: `mcast_groups` appears in
`pubsub.rs` and never in `queries.rs` or `token.rs`, while rmw_zenoh resolves
services through queries and builds the ROS graph from liveliness tokens. That
is a property of zenoh's multicast transport and **not a limitation of CAN** —
the same failure reproduces on stock ROS over UDP multicast. Give CAN a real
unicast face and all of it comes back.

```sh
docker/can-demo/run.sh --zenoh /path/to/zenoh-fork --unicast
```

runs a ROS 2 **service call** across the bus over ISO-TP and asserts it returns
`sum=42`, and runs the same call over the multicast link and asserts it returns
**nothing**. Both halves run, because a demo that shows only the working case
leaves the reader to take the broken case on trust. It also prints the ISO
15765-2 framing `candump` saw — first frames, flow control, consecutive frames.

`--unicast` additionally needs the `can-isotp` kernel module on the host
(`sudo modprobe can-isotp`); `vcan` alone is not enough.

## What this does not show
- **Nothing about timing.** `vcan` has no bit rate and no arbitration, so every
  latency and bandwidth figure remains analytic. Real hardware is untested.
- **Bus load is the publisher's whole output**, not the subscribers' interest,
  and no interceptor runs on a multicast face to filter it.

## What it pins, and why those numbers

`rmw_zenoh` does not consume released zenoh-c. Every live branch — humble
through rolling — pins the same commit on a fork:

| | |
| --- | --- |
| zenoh-c | `05bd370343b5161ca9269649b9a914c9c2dc4170` |
| zenoh | `2687c51352121f006e3a603ce07925a8ad0b295c` |

That zenoh commit is on **neither `main` nor `release/1.8.0`** — it is `main` as
of 2026-04-01 plus one patch, labelled 1.8.0. The container reproduces it
exactly, so what runs here is what ROS runs. Distro choice does not change this:
every rmw_zenoh branch resolves to the same zenoh core.

The library is substituted by `LD_LIBRARY_PATH`, which works because
`librmw_zenoh_cpp.so` and `rmw_zenohd` name `libzenohc.so` as a plain
`DT_NEEDED` with no `RPATH` or `RUNPATH`, and a cargo feature adds no C API. The
feature set matches what `zenoh_cpp_vendor` passes — `unstable` and
`shared-memory` move struct layouts, and with no `DT_SONAME` a mismatch would be
silent memory corruption rather than a link error.

zenoh-pico is built with `BATCH_MULTICAST_SIZE=63`, which is load-bearing rather
than a tuning knob: it advertises that value in its `Join` regardless of the link
MTU beneath it and rejects any peer whose value differs, so a stock build never
associates and the only symptom is one INFO line on its side.
