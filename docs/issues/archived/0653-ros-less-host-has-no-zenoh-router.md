---
id: 653
title: "A ROS-less host has no zenoh router at all, so the getting-started path for the DEFAULT rmw cannot be run — RFC-0075 scoped this consequence to the interop lanes only"
status: resolved
type: design
severity: high
area: provisioning, docs, rmw
related: [issue-0374, issue-0654, issue-0485, rfc-0075, phase-362, rfc-0056]
---

## The gap

[RFC-0075](../design/0075-zenoh-router-provenance-and-the-unstable-seam.md)
retired the vendored `zenohd` and made the router come from ROS
(`rmw_zenoh_cpp/rmw_zenohd`). Its Consequences section accepts exactly one
casualty:

> * **A ROS-less host cannot run the zenoh interop lanes.** Correct: those lanes
>   test interoperation with ROS 2. They should `skip!` with a reason rather than
>   run against a router no user deploys.

That reasoning is sound **for the interop lanes** — a lane that tests
interoperation with ROS 2 has no business passing without ROS 2. But the
consequence is not confined to those lanes, and the RFC does not say so.

**zenoh-pico runs in client mode and needs a router for any two-process
example**, interop or not. A native talker and a native listener are two
processes. So after phase-362 the getting-started path — `nros setup native`,
default `--rmw zenoh`, run the talker, run the listener — has no router on a
host without ROS, and nothing in the tree provides one:

```
$ nros setup native --rmw zenoh --dry-run
nros setup: native (rmw zenoh) needs 2 package(s):
  zenoh-pico             source 1.7.2 — submodule …
  mbedtls                source 3.x — submodule …
```

No router. And the recipe that used to start one now resolves the ROS binary
or fails:

```
$ just native zenohd          # scripts/dev/zenohd.sh
ERROR: no `rmw_zenoh_cpp/rmw_zenohd` under /opt/ros (ROS_DISTRO=unset).
```

## Why this was not caught

The claim it contradicts is in the book's own getting-started page, under a
heading that asks the question directly (`book/src/getting-started/installation.md`):

> ## Do I need ROS 2 installed?
>
> **For the getting-started path, no.** Verified on a host with no ROS 2 at
> all: … the Rust/C/C++ first node builds and publishes against the in-tree
> `zenohd`.

The verification named in that sentence was real — and its subject, "the
in-tree `zenohd`", no longer exists. phase-362 W5 updated
`book/src/design/rmw.md` (the pairing matrix) and stopped there; the
getting-started page was never swept, so the page still promises a no-ROS path
and still points at a binary that was deleted. That is the
[issue-0196](README.md) shape one level up: the *documentation* half of a
retirement was narrower than the retirement.

`nros setup native --dry-run` does not warn, because from its point of view
nothing is missing — the router is simply not a package it provisions any more.

## What is actually true today

| host | `--rmw zenoh` | `--rmw cyclonedds` |
|---|---|---|
| ROS 2 installed | works — router is `rmw_zenohd` from `/opt/ros` | works |
| no ROS 2 | **no router exists** | works — Cyclone is in-process, no daemon |

So there IS a working ROS-less getting-started path; it is
`--rmw cyclonedds`, which is not the default and which the page does not
present as the answer to its own question.

## Direction

Not exclusive; (1) is the honest-documentation floor and is being done with
this issue, (2) and (3) are the actual decision.

1. **Say what is true.** The page must stop promising a no-ROS default path and
   name `--rmw cyclonedds` as the ROS-less route, with `NROS_RMW_ZENOHD` as the
   escape hatch for a user who has a router by some other means. Done alongside
   [issue 0374](archived/0374-zenohd-has-no-prebuilt-so-nros-setup-native-source-builds-it.md).
2. **Decide whether zenoh should remain the default RMW for a getting-started
   host.** It is the right default for a ROS user and now the wrong one for
   everybody else. Making the default conditional on ROS being present is a
   discoverability trade, not an obvious win — a user who follows two different
   pages and gets two different RMWs is worse off than one who is told once.
3. **Or restore a router for ROS-less hosts**, as an explicitly non-interop
   convenience. RFC-0075's argument against a vendored router is that
   *validating a router nobody runs* is worthless — which is an argument about
   what the TEST LANES should use, and does not by itself forbid shipping one
   for local single-host use. If taken, it must not become a second thing that
   drifts: the interop lanes would still be required to use the ROS router, and
   the convenience router would need to be visibly labelled as not-under-test.

## Not this issue

Whether the tree's `zenohd --listen …` invocations still work. They do not —
`rmw_zenohd` ignores argv and reads `ZENOH_CONFIG_OVERRIDE` — but that is a
separate defect with a separate fix, tracked as
[issue 0654](0654-zenohd-invocations-name-a-retired-binary.md).

## Resolution 2026-08-18 — direction (1) + (2) confirmed, (3) declined; and the real defect was narrower than the framing

**The decision, taken by the maintainer:** keep the ROS-shipped router. nano-ros
does not ship one. So of the three directions above, (1) was already done
alongside issue 0374, (2) is answered — zenoh stays the default RMW and
`--rmw cyclonedds` remains the documented ROS-less route — and (3) is declined.

That leaves the part of this issue that was a genuine bug rather than a
documentation gap, and it is not the one the title names.

### "Has ROS" was implemented as "has `/opt/ros`"

Both resolvers — `nros_zenohd_bin` in `scripts/dev/zenohd.sh` and
`nros_tests::process::ros_zenohd_path` — searched `$ROS_DISTRO` and then
`/opt/ros/*`, and nothing else. A user who **sources a working ROS** gets told
there is no router whenever that ROS is not under `/opt/ros`:

* a ROS built from source — which this repo documents doing, for
  Arch/Fedora/NixOS (`docs/development/ros2-on-non-ubuntu.md`);
* a colcon overlay, whose prefix is wherever the user put it;
* anything installed under `/usr`, a container layout, or a Nix profile.

The fix is to read what the sourced environment itself says. Resolution order is
now, most explicit first:

```
1. NROS_RMW_ZENOHD     an explicit path
2. PATH                where a caller who put it there expects it found
3. AMENT_PREFIX_PATH   every prefix the caller has SOURCED, in ament's order
4. $ROS_DISTRO         under /opt/ros
5. /opt/ros/*          newest distro name last
```

Verified with the fallbacks disabled, which is the case that was broken:

```
$ source /opt/ros/humble/setup.bash
$ unset ROS_DISTRO; NROS_ZENOHD_OPT_ROS=/nonexistent
$ nros_zenohd_bin
/opt/ros/humble/lib/rmw_zenoh_cpp/rmw_zenohd
```

### `rmw_zenohd` is not on `PATH`, and that is not a bug

Worth stating because it is the reasonable expectation and it is wrong: sourcing
`setup.bash` does **not** make `command -v rmw_zenohd` work. The binary installs
into `<prefix>/lib/rmw_zenoh_cpp/`, and the setup script exports only `bin/`;
ROS's own route is `ros2 run rmw_zenoh_cpp rmw_zenohd`. Step 2 above honours a
`PATH` the user has arranged rather than depending on one, and the book now says
a silent `command -v` is normal so nobody reads it as a broken install.

### Two resolvers, one table

`zenohd.sh` has carried the sentence *"Mirrors `ros_zenohd_path`; the two must
agree"* since phase-362, with nothing checking it — and they drifted anyway, in
the same direction, which is what this issue is. Two implementations are
unavoidable (one is invoked from a justfile, one from the harness, neither can
call the other), so the shared thing is the EXPECTATIONS:

* `scripts/dev/zenohd-resolution-cases.tsv` — nine rows, each decided by a
  different step of the order, each answered wrongly by some plausible
  alternative order.
* `scripts/check-zenohd-resolution-parity.sh` runs the SHELL over it (in
  `just check`).
* `zenohd_resolution_matches_the_shared_table` runs the RUST over it.

Behaviour on both sides, not a diff of two languages. Mutation-checked by
deleting the `AMENT_PREFIX_PATH` step: 4 of 9 cases fail, naming the rows.

### Two defects the gate found in itself before it found any in the resolver

Both would have made it pass on anything, and both are recorded because a gate
that cannot fail is the failure mode this whole class is about:

* `IFS=$'\t' read` **collapses runs of tabs** — tab is IFS whitespace — so every
  empty column shifted the fields, and each row compared an empty expectation
  against an empty result. Now split on `\037`, with a guard asserting the row
  name is a slug and that at least six rows expect a router.
* A per-case `PATH=... bash -c` prefix assignment applies to the **command
  lookup too**, so `bash` itself was not found. `bash` is now resolved
  absolutely.

### And one in the resolver's own dependencies

Driving the shell over a synthetic `PATH` showed `nros_zenohd_bin` shelling out
to `tr`, `ls`, `sort` and `tail` — so its answer depended on `PATH`, which is one
of the things it resolves over. It is builtins-only now (array split for step 3,
a glob under `LC_ALL=C` for step 5 — the locale being issue 0485's class).

### Not changed

`NROS_ZENOHD_OPT_ROS` exists solely so the gate can drive steps 4 and 5 over a
synthetic tree; no non-test caller sets it. The alternative was a gate that
checks the two new steps and leaves the two legacy ones unwatched on both sides.
