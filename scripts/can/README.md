# Running zenoh over CAN on the host

The MCU end of the CAN link lives in the vendored zenoh-pico tree
([RFC-0080](../../docs/design/0080-can-link-for-zenoh-pico.md), phase-377). The
host end is a Rust link in a fork of zenoh
([RFC-0081](../../docs/design/0081-can-link-for-zenoh-rs.md), phase-378). This
directory builds the piece that connects the second one to ROS 2.

## Why a script and not a patch

`rmw_zenoh_cpp` never sees the Rust code except through `libzenohc.so`, so the
CAN link reaches ROS by rebuilding that library with the `transport_can` feature
and substituting it. Three things make that more than a one-line patch:

- the redirection to the zenoh fork is a set of **absolute paths**, so a diff
  would not be portable;
- the version is **not a free choice** — see below;
- there is a trap that costs an hour to find, also below.

## Usage

```sh
scripts/can/build-zenohc-can.sh --zenoh /path/to/zenoh-fork
```

The fork must be checked out on the branch carrying `zenoh-link-can`, at the
version matching the installed ROS packages. The script refuses to proceed
otherwise rather than producing a library that would corrupt memory.

Then, with no ROS rebuild at all:

```sh
source /opt/ros/humble/setup.bash
export LD_LIBRARY_PATH=<out>:$LD_LIBRARY_PATH
```

`librmw_zenoh_cpp.so` and `rmw_zenohd` name `libzenohc.so` as a plain
`DT_NEEDED` with no `RPATH` or `RUNPATH`, and the vendored library carries no
`DT_SONAME`, so prepending a directory substitutes it wholesale. A cargo feature
adds no C API, so the ABI does not move.

## The version is not a free choice

`rmw_zenoh_cpp` is compiled against the vendored zenoh-c headers. The
replacement must be the **same zenoh-c version**, and the zenoh fork must be on
the matching release branch. The script reads the installed version from
`zenoh_cpp_vendor` and checks the fork against it.

It also matches the **feature set** from the vendored `zenoh_configure.h`.
`unstable` and `shared-memory` change struct layouts, and because there is no
soname to catch a mismatch, getting this wrong is silent memory corruption
rather than a link error. Transport features do not affect the ABI.

## The opaque-types trap

zenoh-c's build script builds a helper crate under `build-resources/opaque-types`
to compute type sizes, and hands it the **parent's** `Cargo.lock`. That crate has
its own manifest. Patch only the parent and the two disagree about where zenoh
comes from, the size probe yields nothing, and the build fails much later and
unrecognisably:

```
no sigatures found for building generic z_take_from_loaned
```

The script patches both manifests. Patching only the parent is the obvious thing
to do and it does not work.

## Talking to the island

A zenoh-pico peer must be built with its multicast batch size set to the CAN
MTU, or the two never associate:

```sh
cmake -S <zenoh-pico> -B build -DZ_FEATURE_LINK_CAN=1 -DBATCH_MULTICAST_SIZE=63
```

`Z_BATCH_MULTICAST_SIZE` is a compile-time constant in zenoh-pico, advertised in
`Join` regardless of what the link beneath can carry, and its receiver rejects
any peer whose value differs. The symptom is one `INFO` line on the pico side
and nothing at all on the Rust side.

## What works over CAN, and what does not

**Topics work.** Two ROS 2 nodes exchange a topic over CAN with no router and no
TCP, and a ROS 2 node publishes to a zenoh-pico peer on the same bus.

**Services, actions, parameters and graph introspection do not.** A zenoh
multicast transport routes pushed data only — `mcast_groups` appears solely in
`pubsub.rs`, never in `queries.rs` or `token.rs` — and rmw_zenoh resolves
services through queries and builds the graph from liveliness tokens. No CAN
link can fix this; see RFC-0081 §3.

Put the CAN endpoint in the **session** config, not the router's, and give every
peer a distinct `id`. A lower identifier wins bus arbitration, so `id` is a
real-time decision rather than a name.
