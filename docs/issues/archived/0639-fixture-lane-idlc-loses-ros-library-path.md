---
id: 639
title: "`activate.sh` sourced ROS's bash-only `setup.bash`, so under zsh it set
  NOTHING — and said nothing
status: resolved
type: bug
area: build
related: [0601, 0633]
---

## What is actually known

Issue 0633 fixed why a broken `idlc` was cached and reused. It left one thing
unexplained, and this issue exists so that gap is recorded rather than implied
away.

`/opt/ros/humble/bin/idlc` on this host links `libiceoryx_binding_c.so`, which
lives in `/opt/ros/humble/lib/x86_64-linux-gnu`. Whether it runs depends
entirely on the loader path:

```
no LD_LIBRARY_PATH                                   -> 127
LD_LIBRARY_PATH=/opt/ros/humble/lib                  -> 127   (wrong dir)
under `source ./activate.sh`                         ->   0
```

`activate.sh` sets

```
LD_LIBRARY_PATH=/opt/ros/humble/opt/zmqpp_vendor/lib:/opt/ros/humble/opt/rviz_ogre_vendor/lib:/opt/ros/humble/lib/x86_64-linux-gnu:/opt/ros/humble/lib
```

so the tool is not intrinsically broken here.

## RESOLVED 2026-08-17 — it was the SHELL, and none of the candidates below

`activate.sh` sourced `/opt/ros/humble/setup.bash` unconditionally.
`setup.bash` is a bash script: it reads `${BASH_SOURCE[0]}`, which zsh does not
define, so under zsh it sets **nothing** — and it fails silently, so
`activate.sh` reported success while no ROS environment existed at all.

Everything downstream was innocent. No make driver, `cmake -E env`, ninja rule
or `just` boundary dropped the variable; it was never set in the first place.
Measured on this host:

| shell | sourced | result |
| --- | --- | --- |
| bash | `activate.sh` (→ `setup.bash`) | `ROS_DISTRO=humble`, `LD_LIBRARY_PATH` set |
| **zsh** | `activate.sh` (→ `setup.bash`) | **both UNSET** |
| zsh | ROS's `setup.zsh` | `ROS_DISTRO=humble`, `LD_LIBRARY_PATH` set |
| zsh | ROS's `setup.sh` (POSIX) | `ROS_DISTRO=humble`, `LD_LIBRARY_PATH` set |
| bash | ROS's `setup.sh` | `ROS_DISTRO=humble`, `LD_LIBRARY_PATH` set |

That is the whole of `code=127`: a build launched from zsh invoked
`/opt/ros/humble/bin/idlc` with no loader path, and the failure surfaced far
away as "command not found" on the first `.idl`.

### The fix

`activate.sh` now picks a file the CURRENT shell can read — `setup.bash` under
bash (so a working setup sees no change, and keeps bash completions),
`setup.sh` otherwise — and then CHECKS that it worked, warning when
`ROS_DISTRO` is still unset afterwards. Sourcing something that quietly sets
nothing is the failure this issue is made of, so the file no longer assumes its
own success.

`activate.fish` already had the right shape and is untouched: it looks for
`setup.fish`, and when there is none it says so and names the remedy
(`bass source …`). This file was the one that assumed its own shell.

### Proofs

* Both shells now load the environment: `bash: ROS_DISTRO=humble LD=set`,
  `zsh: ROS_DISTRO=humble LD=set`.
* The new guard was sabotage-tested with a setup file that sources cleanly and
  sets nothing — the shape the zsh/`setup.bash` pairing had. It fires and names
  the shell correctly (`shell: zsh`, `shell: bash`), and stays silent on a
  healthy load (0 occurrences in both).
* End to end: with a probe shim standing in for `/opt/ros/humble/bin/idlc`, the
  real ninja rule was rebuilt from **zsh**. The rule now sees the full
  `LD_LIBRARY_PATH=/opt/ros/humble/opt/zmqpp_vendor/lib:…:/opt/ros/humble/lib/x86_64-linux-gnu`
  and the build returns 0 — the same path that produced `code=127` before.

### One correction to the message this issue shipped with

The guard's first draft printed `shell: ${0##*/}`, which under a SOURCED file is
the file — it reported `shell: activate-sabotage.sh`, pointing the reader at
`activate.sh` when the shell is the thing that matters. It now derives the name
from `ZSH_VERSION` / `BASH_VERSION`.

## The unexplained part (superseded — kept for the record)

`just build-test-fixtures lane=native` was launched from a shell that HAD
sourced `activate.sh`, and the cyclone leaves still failed with

```
/opt/ros/humble/bin/idlc: error while loading shared libraries:
libiceoryx_binding_c.so: cannot open shared object file: No such file or directory
FAILED: [code=127] …
```

Something between that shell and the `idlc` invocation does not carry
`LD_LIBRARY_PATH`. Candidates not yet distinguished:

* the generated make driver, or the `make -j` leaves it spawns;
* `cmake -E env` launchers, which replace rather than extend the environment
  when handed a full assignment;
* ninja's own invocation of a raw baked command;
* a `just` recipe boundary.

None of these was the cause. All four assumed the variable existed and was
lost in transit; it never existed. The instruction to "find the boundary first
— printing `LD_LIBRARY_PATH` from inside the failing `idlc` rule is the
cheapest probe" was the right instruction, and it is what found the answer: the
probe showed the rule's environment, and working backwards showed the shell had
never had it either.

Worth keeping as a method note: the four candidates were all plausible, all
downstream, and all wrong, because the question "what drops it?" smuggled in
the premise that something had it. The probe that settled it was the one that
made no such assumption.

## Why it is not urgent, and why it should not simply be closed

It is currently masked. After 0633, `idlc` resolves to the SDK-provisioned
`~/.nros/sdk/cyclonedds/<ver>/bin/idlc`, which has no such loader dependency,
so no build on this host reaches ROS's copy. The lane is green.

That masking is the reason to file rather than drop it:

* the environment gap is real and is not specific to `idlc` — any host tool a
  build reaches through the environment is exposed to the same boundary;
* `_nros_idlc_runs` derives `<prefix>/lib/${CMAKE_LIBRARY_ARCHITECTURE}` and
  bakes an `LD_LIBRARY_PATH` launcher when the bare invocation fails, so a host
  WITHOUT an SDK idlc will depend on that path working — and it was never
  exercised, because 0633's cache meant the probe never ran;
* the probe validates in the CONFIGURE environment and, when the tool runs
  there, bakes no launcher at all. A build later executed in a thinner
  environment then fails at `code=127` with nothing recording that the
  configure had made an assumption. That shape is worth checking on its own.

## Which `idlc` SHOULD be used — measured 2026-08-17

The question behind this issue is whether to keep preferring the
SDK-provisioned `idlc` or simply use the one ROS ships. Two sub-questions were
asked, and both were measured rather than reasoned about.

### Is the Cyclone DDS version guaranteed per ROS edition? No.

From the ROS apt indexes (`packages.ros.org/ros2/ubuntu/dists/<codename>/main/binary-amd64/Packages.gz`):

| edition | `ros-<distro>-cyclonedds` |
| --- | --- |
| humble | `0.10.5-2jammy.20260226` |
| iron | `0.10.5-1jammy.20241108` |
| jazzy | `0.10.5-1noble.20260225` |
| kilted | `0.10.5-2noble.20260410` |
| **rolling** | **`11.0.1-1noble.20260424`** |

Every RELEASED edition currently ships upstream 0.10.5 — the version this tree
pins as `0.10.5-nros1` — so the pin has been trouble-free. That convergence is
a coincidence of timing, not a contract:

* the debian revisions and build dates differ (`-1` vs `-2`, 2024-11 through
  2026-04), so each distro re-releases on its own schedule and the upstream
  version a distro carries can move during its life;
* **rolling has already moved to 11.0.1**, a major bump. The next distro cut
  from rolling will not be 0.10.5, and on that day "the version ROS used" stops
  being a single answer.

### Does idlc behaviour differ per edition? Not per EDITION — per VERSION.

Because all four released editions ship the same upstream 0.10.5, they ship the
same `idlc`. Verified rather than assumed: 8 real generated IDLs from
`cyclonedds-ts/_idlroot` (messages plus the Fibonacci action) compiled with
`-t -l c` by both `~/.nros/sdk/cyclonedds/0.10.5-nros1/bin/idlc` and
`/opt/ros/humble/bin/idlc` produce 16 generated files each, **byte-identical**.
Run WITHOUT `-t`, both emit the same `@verbatim … not supported` warnings, so
the XTypes limitation the `-t` flag works around is upstream 0.10.5's and not
something this fork introduced.

That is consistent with what the fork actually contains: its commits are all
`ddsrt` / `ddsi` — RTOS sync ports, multicast/socket fixes, addrset lock
striping. It does not touch the IDL compiler.

### So: keep using ours

Not because it compiles better — today the two are the same compiler. Because
the correctness constraint is that idlc's output must match **the `ddsc` the
image LINKS**, and the native cyclone examples build CycloneDDS FROM SOURCE
in-tree (`…/nros-rmw-cyclonedds/_cyclonedds`), so the runtime is
`0.10.5-nros1`. ROS's idlc satisfies that only while the versions coincide.

Taking ROS's copy would make descriptor generation a function of the host's ROS
edition — precisely the axis that is already diverging. On a rolling-derived
host an 11.x compiler would emit descriptors into a 0.10.5 runtime, silently:
the museum-compiler / `find_descriptor() -> nullptr` class issue 0325 already
describes. Preferring ours also sidesteps this issue entirely, since the SDK
binary has no ROS loader dependency.

The preference order 0633 restored (SDK store first, `-h`-probed, then PATH) is
therefore the right one, and this issue's `LD_LIBRARY_PATH` question stays a
question about the FALLBACK path rather than about the normal one.

### What this does not establish

* Only humble's `idlc` binary exists on this host. iron / jazzy / kilted are
  inferred from identical upstream version strings, not executed.
* The comparison covered 8 message/action IDLs — not unions, `@key`, or deeper
  module nesting.
* It says nothing about the boundary that drops `LD_LIBRARY_PATH`, which is
  still the open part of this issue.

## Provenance

Split out of issue 0633 on 2026-08-16. 0633's first draft asserted that
propagating ROS's library path "would not have worked here either", on the
strength of the `LD_LIBRARY_PATH=/opt/ros/humble/lib -> 127` line above. That
was the wrong directory, and the conclusion drawn from it was wrong; the
correction is recorded in 0633 and the real question is this one.
