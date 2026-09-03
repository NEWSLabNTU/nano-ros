---
id: 741
title: "`test_xrce_service_ros2_client` fails on main — Fast-DDS refuses the
  28-byte reply into a 15-byte history payload"
status: resolved
type: bug
area: rmw, testing
related: [issue-0736, issue-0776]
---

## Symptom

```
cargo nextest run -p nros-tests --test xrce_ros2_interop \
    -E 'test(test_xrce_service_ros2_client)' --retries 0

ROS 2 service client did not get sum=8 from the nano-ros XRCE service server
— XRCE-DDS service interop regression (233.6).
--- server startup ---
[INFO] nros: session open
[INFO] Waiting for service requests

--- ros2 client output ---
requester: making request: example_interfaces.srv.AddTwoInts_Request(a=5, b=3)
[RTPS_READER_HISTORY Error] Change payload size of '28' bytes is larger than
the history payload size of '15' bytes and cannot be resized.
    -> Function can_change_be_added_nts
```

Deterministic: 3/3 in-sweep retries and 1/1 solo.

## What the error says, and what it does not

The server came up and listened — its startup output is in the assert for
exactly this reason, and it rules out "no reply because the server never
started". The failure is on the **ROS 2 client's reader for the reply topic**:
Fast-DDS sized that reader's history from a max-serialized-size it learned at
discovery, got 15, and then refused a 28-byte sample rather than resizing.

15 bytes is not a plausible `AddTwoInts_Response` (8-byte `sum` + a 4-byte CDR
header is 12; a request is 16 + 4). So the reader was created against a type
whose advertised max size is wrong — a type-registration/discovery defect on
the XRCE side, not a serialization one. The reply is almost certainly being
built correctly and then dropped by the peer's history.

Note the direction: the sibling `test_xrce_action_ros2_client` and
`test_xrce_to_ros2_pubsub` both PASS in the same run, so whatever is wrong is
specific to the service reply type registration, not to the XRCE transport or
the agent as a whole.

## Not caused by phase-359 W10

Checked the way #0736 and #0737 were: `git checkout origin/main --
packages/core packages/api scripts/check-std-census.py`, `just setup-cli`,
rebuild native fixtures, run — fails identically. Upstream `main` is red here.

## Second host, 2026-08-21 — does not reproduce

Ran the issue's own sequence on the other host, after `just build-test-fixtures
lane=native` (RC=0) and `just setup-cli`:

| arm | result |
| --- | --- |
| `-E 'test(test_xrce_service_ros2_client)' --retries 0`, five separate runs | 5 pass / 0 fail |
| the whole `xrce_ros2_interop` binary, in-sweep, `--retries 0` | **9 tests run: 9 passed, 0 skipped** |

The in-sweep arm matters because this issue reports the failure as deterministic
in-sweep (3/3) as well as solo; the siblings that pass for you
(`test_xrce_action_ros2_client`, `test_xrce_to_ros2_pubsub`) pass here too, in
the same run as the one that fails for you.

Host: `ROS_DOMAIN_ID` / `CYCLONEDDS_URI` / `RMW_IMPLEMENTATION` unset, ROS 2
Humble at `/opt/ros/humble`.

**A hypothesis worth killing before anyone spends time on it.** Issue 0740 (the
config-header mirror invisible to Makefiles) lands consumer TUs against the
in-tree STUB header, and issue 0268 records that exact stale-sizes path
producing wrong `*_OPAQUE_U64S` and "memory corruption that surfaces as
unrelated runtime failures". A 15-byte history for a type that cannot be 15
bytes looks exactly like that. It is NOT that here: this fixture
(`build/cargo-fixtures/linux-*/service-server`) is a CARGO binary, and 0740 is a
cmake-generator defect that never touches it.

So the difference is the host, not the tree — which is the same split #0737 hit,
and worth stating so the two are not investigated as one thing.

## Fourth environment, 2026-08-26 — green, and the resolver now SAYS which agent ran

Ran the issue's command on a fourth host/tree (`2eb276e46`) after `just
setup-cli` + `build-test-fixtures lane=native`: `test_xrce_service_ros2_client`
passes and the whole binary is **8/8**. Fourth green environment against one red.

**The skew is present here and is not sufficient.** This host's store agent is
the same Jazzy-era pairing the "Version skew, measured" section above documents
(bundled Fast-DDS **2.14.6**; ROS peer **2.6.11**). Since
`xrce_agent_binary_path()` prefers `build/xrce-agent/` over the store, both are
swappable on one tree, so this was measured directly rather than argued — same
host, same fresh fixtures, whole binary each time:

    ROS-paired agent (2.6.11)    -> 8/8 pass
    SDK store agent (2.14.6 skew) -> 8/8 pass

That does not contradict "Proven (2026-08-21)" — an agent with zero skew works
on the RED host. It adds the other half: **skew alone does not cause the
failure**, because a skewed agent passes everywhere else. Whatever the red host
has is something the skew interacts WITH, not the skew by itself.

**What I did not reproduce, and the warning it repeats.** My first run here did
fail — with `Test fixture is STALE … newer: packages/api/nros/src/node_metadata.rs`,
which is not this issue's symptom: no `RTPS_READER_HISTORY` error, no 15-byte
history, the fixture never started. I nearly logged it as a reproduction. The
one-line discriminator: a real instance shows the Fast-DDS error from the ROS
CLIENT; a stale tree shows a resolver error before anything runs.

### Change made: the agent announces its provenance

Every axis in this issue was compared by hand across hosts, and each comparison
began with someone working out which agent their machine had picked — a
question the run itself never answered. It does now:

```
xrce agent: /…/build/xrce-agent/MicroXRCEAgent — built against the sourced ROS (no Fast-DDS skew)
xrce agent: /…/.nros/sdk/xrce-agent/2.4.3-nros1/bin/MicroXRCEAgent — the `nros setup` SDK pin,
            which BUNDLES its own Fast-DDS (a version skew against the ROS peer is possible — issue 0741)
```

`XrceAgentProvenance` distinguishes the ROS-paired build, the SDK store, and a
bare `PATH` agent, and `xrce_agent_binary_path()` is derived from it so the
resolution order keeps one spelling. Note what this corrects: `ca224e271` is
titled "the harness agent is always Fast-DDS-paired with the sourced ROS", and
that holds only for hosts that ran `just xrce setup` — `nros setup --tool
xrce-agent` installs the bundled pin, and nothing said so. Same shape as issue
0774 one component over: finding A binary is not finding the RIGHT one, and the
failure lands layers away.

If the red host is still red, its log now names the agent and the pairing in one
line, which is the first thing to paste next time.

## Reproduce

```
just build-test-fixtures lane=native
cargo nextest run -p nros-tests --test xrce_ros2_interop \
    -E 'test(test_xrce_service_ros2_client)' --retries 0
```

## Counter-measurement (2026-08-21) — GREEN in the ROS distrobox, so this is an environment axis

The issue's own command, on current `main`, in the ROS distrobox:

```
cargo nextest run -p nros-tests --test xrce_ros2_interop \
    -E 'test(test_xrce_service_ros2_client)' --retries 0

PASS [ 3.343s] (1/1) nros-tests::xrce_ros2_interop test_xrce_service_ros2_client
```

and in a full `just ci` sweep in the same environment: 1490 cases, **0 real
failures**, with all eleven `xrce_ros2_interop` / xrce-service cases passing.

So "upstream `main` is red here" is true of that host and not of this one. That
matters for the diagnosis: the issue reasons from the 15-byte history to "a
type-registration/discovery defect on the XRCE side", and a defect in the
registration itself would not care which machine ran it. Something in the
environment is choosing the advertised max-serialized-size.

Exact versions where it passes, so the difference is checkable rather than
guessed:

| component | version |
| --- | --- |
| ROS | Humble (Ubuntu 22.04 distrobox) |
| Fast-DDS | 2.6.12-1jammy |
| rmw_fastrtps_cpp | 6.2.10-1jammy |
| XRCE agent | 2.4.3-nros1 (the SDK-index pin, `~/.nros/sdk/xrce-agent/`) |

The agent is the component that registers the DDS type the ROS reader sizes
itself from, so it is the first thing to compare — but it is PINNED by the
index, which makes Fast-DDS / rmw_fastrtps the more likely axis if the failing
host also has 2.4.3-nros1.

**Worth ruling out before the type-registration hypothesis:** fixture
freshness. A museum XRCE binary is the most common cause of a deterministic
interop red in this tree, and the run above was on a fixture set rebuilt from
scratch minutes earlier (CLI first, then `build-test-fixtures lane=native`).
If the failing host's fixtures predate a `nros` CLI rebuild, they are not
testing the code the diagnosis is about.

Not closing: it fails there, and that is real. What this changes is the
question — from "what is wrong with the service reply type registration" to
"which of agent / Fast-DDS / fixture-freshness differs between the two hosts".

## Armed (2026-08-21) — the failing run now describes its own environment

Three hosts have now run the same command on the same tree: one fails
deterministically, two pass. That is two "does not reproduce" reports, and a
third would not settle it either — nothing in the failure output describes the
stack underneath the two processes, so the one host that CAN answer the question
is the one nobody can inspect.

So the assert carries a fingerprint now
(`nros_tests::ros2::interop_environment_fingerprint`). Sample, from a host where
it passes:

```
--- interop environment ---
  ROS_DISTRO=humble
  RMW_IMPLEMENTATION=<unset>
  ROS_DOMAIN_ID=<unset>
  xrce agent: ~/.nros/sdk/xrce-agent/2.4.3-nros1/bin/MicroXRCEAgent
  fastrtps: 2.6.12
  rmw_fastrtps_cpp: 6.2.10
```

Those are the fields the diagnosis turns on. The agent registers the DDS type a
ROS reader sizes its history from, so its version is the first comparison; Fast-DDS
is the second, since a reader that refuses to resize is enforcing a rule some
versions enforce differently.

Two details worth keeping, because each was a bug in the first draft:

* **Every AMENT prefix, both packages.** Stopping at the first prefix that
  yielded anything reported `rmw_fastrtps_cpp` and left `fastrtps` permanently
  "not found" — omitting the field the issue most needs. More than one version
  of a package on the path is printed rather than collapsed: two of them is
  itself an answer.
* **Two version SHAPES.** `fastrtps` is not an ament package and ships no
  `package.xml`; its version lives in
  `share/fastrtps/cmake/fastrtps-config-version.cmake` as
  `set(PACKAGE_VERSION "…")`. Probing only the ROS shape is what produced the
  false "not found" above.

The probe is total — every lookup degrades to a note rather than an error,
because it runs on a path that is already failing and must not add a second
failure mode. A unit test asserts that and prints the result under
`--nocapture`.

**Next step is a measurement, not a patch:** re-run the failing command on the
host where it fails and compare those five lines against the sample above.

## Version skew, measured (2026-08-21) — the agent is a JAZZY-era Fast-DDS on a Humble bus

The fingerprint above reports the SYSTEM Fast-DDS. The other half of the wire is
the one bundled inside the XRCE Agent, and that is where the skew is.

**Measured on disk** (`~/.nros/sdk/xrce-agent/2.4.3-nros1/lib/`):

```
libfastrtps.so.2.14.6
libfastcdr.so.2.2.7
```

**Measured from the installed ROS** (Humble, the tree's `DEFAULT_ROS_DISTRO`):

```
fastrtps 2.6.12    rmw_fastrtps_cpp 6.2.10
```

So one DDS bus carries Fast-DDS **2.14.6** talking to **2.6.12**, and — the
sharper difference — Fast-CDR **2.2.7** against Humble's Fast-CDR **1.x**. Fast-CDR
2 is a different major version of the library that computes serialized sizes,
which is precisely the quantity a reader sizes its history from. A 15-byte
history for a type that cannot be 15 bytes is the shape that produces.

### Where 2.14 comes from, and the agent-to-Fast-DDS map

Not a nano-ros choice: `[tool.xrce-agent]` pins upstream `v2.4.3`, and that tag's
`CMakeLists.txt` superbuilds its own Fast-DDS. Read from the upstream tags:

| Micro-XRCE-DDS-Agent | Fast-DDS | Fast-CDR |
| --- | --- | --- |
| v2.2.0 | 2.4.1 | 1.0.22 |
| v2.3.0 | 2.9.x | 1.0.26 |
| v2.4.0 | 2.10.x | 1.0.27 |
| v2.4.2 | 2.12.x | 1.1.1 |
| **v2.4.3 (ours)** | **2.14.x** | **2.2.x** |

Against the ROS editions (Humble measured here; Iron/Jazzy are the widely-known
pairings and are NOT verified on this machine — verify before acting on them):

| edition | Fast-DDS | nearest agent |
| --- | --- | --- |
| Humble | 2.6.x *(measured 2.6.12)* | none — no agent release bundles 2.6.x |
| Iron | 2.10.x *(unverified)* | v2.4.0 |
| Jazzy | 2.14.x *(unverified)* | **v2.4.3 — what we ship** |

So the agent we pin lines up with **Jazzy**, while the tree defaults to
**Humble**. Nobody chose that: the pin tracks an upstream agent release, and the
Fast-DDS rides along inside it.

### Why this is not obviously "just bump/downgrade the pin"

* **No agent release matches Humble.** The closest below is v2.3.0 (2.9.x), which
  at least keeps Fast-CDR on the **1.x** line — the boundary that matters more
  than the Fast-DDS minor.
* **The agent supports using the system libraries** —
  `UAGENT_USE_SYSTEM_FASTDDS` / `UAGENT_USE_SYSTEM_FASTCDR`, which our
  `[tool.xrce-agent.source].configure` does not set. Building against the target
  edition's own Fast-DDS removes the skew BY CONSTRUCTION rather than by picking
  a number. But it also destroys the property the bundle exists for: a
  relocatable prefix that works on a host with NO ROS installed. That would have
  to become a second variant, not a replacement.
* **It is a pin with interop consequences**, the same class as the cyclonedds
  pointer tracking the version ROS ships (CLAUDE.md now records that rule). Not
  an agent's call to move.

### What this does NOT explain

Why it fails on one host and passes on two others, all of which run the same
pinned agent. If the skew alone were sufficient, every host would fail. So the
skew is the mechanism's setting, not its trigger — something else on the failing
host (a different system Fast-DDS, a second agent on PATH, a stale fixture)
selects it. That is what the fingerprint is there to catch.


## Third host (the original red one), 2026-08-21 — the fingerprint does NOT separate red from green

Re-ran the issue's recipe here at head (`12a3adcca`), fixtures rebuilt from
scratch twice (CLI first, then `lane=native`, FX=0 both times — museum binary
ruled out). Deterministic FAIL, 7.3–7.9 s real runs.

The e449b0b63 fingerprint on the FAILING run:

| axis | failing host | passing distrobox |
| --- | --- | --- |
| ROS_DISTRO | humble | humble |
| fastrtps | 2.6.12 | 2.6.12-1jammy |
| rmw_fastrtps_cpp | 6.2.10 | 6.2.10-1jammy |
| agent | `build/xrce-agent/MicroXRCEAgent` (85-byte shim → `~/.nros/sdk/xrce-agent/2.4.3-nros1`) | 2.4.3-nros1 |
| RMW_IMPLEMENTATION / ROS_DOMAIN_ID / profiles XML | unset / test-assigned / none | unset |

**Every fingerprinted axis is identical, and the verdict differs.** So the
discriminator is none of agent / Fast-DDS-version / fixture-freshness — the
fingerprint needs a deeper axis. (The 85-byte agent path looked like the smoking
gun; it is a shim `exec`ing the same pinned 2.4.3-nros1.)

Two observations that narrow where to look next:

* `…cannot be resized` means the failing reply reader runs a strict
  PREALLOCATED history policy — `rmw_fastrtps` defaults to
  PREALLOCATED_WITH_REALLOC, which resizes. No `FASTRTPS_DEFAULT_PROFILES_FILE`
  / `RMW_FASTRTPS_USE_QOS_FROM_XML` is set on this host, so the policy
  difference is baked into a BINARY, not chosen by config.
* 28 bytes is the CORRECT reply wire size (4 encaps + 16 request/reply header +
  8 `sum`); 15 bytes is not any AddTwoInts serialization. Consistent with the
  Jazzy-era-bundle mechanism: the reader sized itself from a max-serialized-size
  computed by one Fast-DDS generation and received a change framed by another.

Next discriminator worth fingerprinting: the exact BUILD (snapshot date /
sha1sum) of `libfastrtps.so` and `librmw_fastrtps_cpp.so` on both hosts — same
version string, different rebuild dates is exactly what apt's rolling ROS
snapshots produce, and a behavior default can move between rebuilds.
sha1 on the failing host:

```
8e4aea5ce69605ebf24e087fda36ee52a9e80758  libfastrtps.so.2.6.12
897971f5829ad150bf1fd0fa31edbb8df61aea24  librmw_fastrtps_cpp.so
```

Compare on a green host; a mismatch at equal version strings is the axis.

## Proven (2026-08-21) — an agent built against the edition's OWN Fast-DDS works

The skew above is removable, and the removal is cheap. Built upstream agent
**v2.4.2** against Humble's installed Fast-DDS:

```
cmake -S src -B build -DCMAKE_BUILD_TYPE=Release -DUAGENT_BUILD_EXECUTABLE=ON \
  -DUAGENT_USE_SYSTEM_FASTDDS=ON -DUAGENT_USE_SYSTEM_FASTCDR=ON \
  -DUAGENT_P2P_PROFILE=OFF -DUAGENT_LOGGER_PROFILE=OFF -DUAGENT_SOCKETCAN_PROFILE=OFF

ldd build/MicroXRCEAgent
  libfastrtps.so.2.6 => /opt/ros/humble/lib/libfastrtps.so.2.6
  libfastcdr.so.1    => /opt/ros/humble/lib/libfastcdr.so.1
```

Zero skew: the agent and the ROS peer are now the same libraries. Dropped it at
`build/xrce-agent/MicroXRCEAgent` (which `xrce_agent_binary_path()` prefers over
the SDK store, so no code change was needed to test it) and ran the whole suite:

```
9 tests run: 9 passed, 0 skipped
```

including `test_xrce_service_ros2_client`, the one this issue is about.

### Why v2.4.2 and not our pinned v2.4.3

The agent tag is not free to choose: its SYSTEM branch expects a particular
Fast-CDR MAJOR.

| agent | system-branch Fast-CDR | usable against |
| --- | --- | --- |
| v2.4.2 and earlier | 1.x | Humble (`libfastcdr.so.1`), Iron |
| v2.4.3 (our pin) | 2.x | Jazzy |

Humble ships `libfastcdr.so.1`, so **v2.4.3 cannot be built against Humble at
all** — which is the same fact that makes its bundled 2.14.6/2.2.7 a Jazzy build.
The edition does not merely select a Fast-DDS version, it selects an agent tag.

### The shape this suggests

`[system].ros_edition` already exists (humble | iron | jazzy, default humble)
and already drives the `ros-<edition>` cargo features. The agent is the one
host tool whose correct build depends on it, and today it does not know.

* **When a ROS install is present** — the only case where interop matters — build
  the agent from source with `UAGENT_USE_SYSTEM_FASTDDS/FASTCDR=ON` against
  THAT install, picking the agent tag from the edition's Fast-CDR major. No
  per-edition prebuilt tarballs to publish: the correct Fast-DDS is the one
  already on the machine.
* **When there is no ROS** — an embedded-only user — the current bundled
  prebuilt stays exactly right. There is nothing to skew against.

That keeps the relocatable no-ROS bundle the pin exists for, and makes the
edition axis reach the one tool that silently ignored it. What it needs is an
edition→agent-tag table and a "ROS present?" branch in `nros setup`; the
`[tool.xrce-agent.source]` recipe already exists and only needs the two flags
and a tag that varies.

### Still not the trigger

This does not explain why one host fails and two pass on the SAME pinned agent —
see above. It explains why the failure is POSSIBLE at all, and removes the
setting that makes it possible.

## The sha1 discriminator is ruled out too (2026-08-21) — byte-identical libraries

Answering the question the section above asks, from the green distrobox:

```
8e4aea5ce69605ebf24e087fda36ee52a9e80758  /opt/ros/humble/lib/libfastrtps.so.2.6.12
897971f5829ad150bf1fd0fa31edbb8df61aea24  /opt/ros/humble/lib/librmw_fastrtps_cpp.so
```

**Identical to the failing host, byte for byte.** Same apt rebuild, not merely
the same version string. So the "different snapshot at equal version" axis is
out.

Standing tally of everything now excluded: agent build (same pinned
2.4.3-nros1), Fast-DDS and rmw versions, the library BINARIES themselves,
fixture freshness (rebuilt twice on the failing host), profiles XML, and the
domain/RMW env. The DDS stack on the two machines is the same software.

### Where that points, and it is not the stack

If the code is identical on both sides, the difference is what else is ON THE
BUS. That is worth stating because this issue's own reasoning — a reader sized
from a max-serialized-size learned AT DISCOVERY — does not require the reply
type to be the one it matched. A reader that discovered a DIFFERENT endpoint
would size itself from THAT type, and 15 bytes is not any `AddTwoInts`
serialization (the section above already establishes 28 is the correct one),
which is exactly what matching something else looks like.

Two candidates, both about a foreign participant rather than a version:

* **Issue 0707's hazard, which this issue's own filing predicted.** 0707 was
  filed FROM this failure — "an orphan from the last run joins the next one" —
  and its fix makes a filtered/solo run step off an occupied domain. If the
  failing host carries a stale participant (issue 0659's class: this host has
  had days-old `zenohd`/agent processes before), the ROS client can discover it.
  Note the failing runs above predate that fix landing, and 0707 explicitly did
  NOT claim to fix this failure.
* **A second XRCE agent or ROS node already running**, holding the reply topic
  with a differently-registered type.

### The check that would settle it

On the failing host, WHILE the test runs, list who else is on the bus rather
than inspecting versions again:

```
ros2 node list        # during the run, in the test's ROS_DOMAIN_ID
ros2 topic info -v /<service reply topic>
ss -unap | grep -E ':(7[0-9]{3}|[0-9]{5})'   # RTPS ports, stray participants
```

A second endpoint on the reply topic is the finding; none, and the foreign-peer
hypothesis dies with it. Cheaper than another version comparison, and it is the
first check that has not already been answered identically on both machines.

## Armed again (2026-08-21) — the failing run now captures the bus, not just the versions

Five axes have now been compared across the hosts and matched exactly, ending
with the library binaries themselves. Asking someone to run three `ros2`
commands at the right moment during a failing run is the remaining step, and
that has a poor success rate across sessions — so the test does it.

On failure, and only on failure, the assert now carries a bus snapshot taken
**while the server and agent are still alive**:

```
--- DDS bus, domain 1, peers still alive ---
  [nodes]
    <empty>
  [services]
    /add_two_ints [example_interfaces/srv/AddTwoInts]
  [topics]
    /parameter_events [rcl_interfaces/msg/ParameterEvent]
    /rosout [rcl_interfaces/msg/Log]
```

That is the GREEN reading, captured by forcing the failure path on a passing
host — one `/add_two_ints`, nothing foreign. On the failing host, a second
`/add_two_ints`, or a reply topic nobody in the test created, is the finding;
the same shape as this one kills the foreign-peer hypothesis and sends the next
reader somewhere else.

Three details, each of which would have made it useless:

* **Ordering.** `server.kill()` and `drop(agent)` ran BEFORE the assert, so a
  snapshot taken there would show an empty bus and prove nothing. The capture
  moved ahead of teardown rather than into the assert with the other
  fingerprint.
* **Hidden topics included.** A service's request/reply pair is hidden, and
  hiding them is exactly what would keep a foreign endpoint out of the listing.
* **An empty `[nodes]` is the normal reading, not a failed probe** — the agent
  creates DDS participants on behalf of its clients and those are not ROS nodes.
  Said in the helper, because a reader who mistakes it for a broken probe stops
  there.

Costs nothing when green: the snapshot is skipped entirely on success, and the
suite is still 9/9 in 15 s.

The domain is printed in the header because the test passes it per-invocation
rather than exporting it — so the fingerprint's `ROS_DOMAIN_ID=<unset>` is
honest about the process and says nothing about the bus. Worth reading together:
this run was on **domain 1**, which is issue 0707's default for a filtered run.


## Third green host (2026-08-22) — and the "fixture freshness" exclusion is not safe

Ran the suite on the second green host again, current tree: **8/8, 15.8 s**,
`test_xrce_service_ros2_client` among them. Another green adds little on its
own; what getting there cost is the part worth recording.

**The documented repro does not necessarily rebuild the fixture it names.**
`just build-test-fixtures lane=native` exited 0, and immediately afterwards
every XRCE-feature native fixture read STALE:

```
x2  0m  build/cargo-fixtures/linux-3000917972/nros-relwithdebinfo/service-server
x1  0m  .../talker   .../service-client   .../listener
x1  0m  .../action-server   .../action-server-concurrent   .../action-client
```

```
binary:          20:12:48
generated/action_msgs/src/msg/goal_info.rs:  20:45:09
```

The lane runs `nros sync` first, which REWRITES the leaf's `generated/` tree.
The regenerated files are byte-identical, so cargo's content fingerprint says
"nothing to do" and never relinks — while the staleness probe is mtime-based
and sees a source newer than the binary. Green build, stale fixture, and the
build says nothing. Deleting the seven binaries and rebuilding produced a real
link (`16:17:40` against `16:16:53`) and the tests then ran.

Why that matters HERE: the standing tally above excludes **fixture freshness**
on the grounds that the failing host "rebuilt twice". A rebuild that reports
success can leave the previous binary in place, exactly as observed on this
host — so "rebuilt twice" is evidence about the COMMAND, not about the binary.
That axis should be re-closed with the mtimes, not with the exit code:

```
ls -la --time-style=+%H:%M:%S <fixture-binary> <leaf>/generated/**/*.rs | sort -k6
```

If the binary is older than anything under `generated/`, the failing runs were
made by a museum binary and every conclusion drawn from them is about an older
build. This does not predict which way it resolves — it says the axis was
closed by an argument that does not hold.

Note the shape is NOT the same as issue 0445's absorbing STALE verdict: here
the probe was right and the BUILD was wrong. `just fixture-staleness` cleared
to zero for this group once a genuine link happened, so nothing needs
suspecting in the probe.

Unrelated but visible in the same listing, and quiet for days:

```
x6  5d  .../qemu-arm-baremetal/thumbv7m-none-eabi/nros-relwithdebinfo/qemu-bsp-large-msg-test
x3  6d  .../qemu-arm-baremetal/thumbv7m-none-eabi/nros-relwithdebinfo/qemu-baremetal-main-e2e
```

Two coordinates that have produced no runtime result in 5–6 days — the 0445
shape, in a lane nobody in this issue is watching.

## Reproduced on the second host (2026-08-23)

The environment-specific theory is now much weaker: this host reproduces the
reported symptom exactly.

```
cargo nextest run -p nros-tests --test xrce_ros2_interop \
    -E 'test(test_xrce_service_ros2_client)' --retries 0

FAIL [11.698s] nros-tests::xrce_ros2_interop test_xrce_service_ros2_client
[RTPS_READER_HISTORY Error] Change payload size of '28' bytes is larger than
the history payload size of '15' bytes and cannot be resized.
ROS 2 service client did not get sum=8 from the nano-ros XRCE service server
```

An 11.7 s runtime, not a sub-second bail — the fixture built, the server ran and
the ROS 2 client ran. Four earlier attempts on this host never reached the test:
three died on STALE fixtures (a `git pull` between building them and running,
which re-stamps every source) and one on unrelated transient gate reds that other
sessions have since fixed. None of those were evidence about the bug, and the
"does not reproduce" reading nearly drawn from them would have been wrong.

So the tally is **two hosts reproduce, two environments do not** (the other host
and the ROS distrobox). Whatever explains the split has to account for both
sides; a fix should not be accepted until it does.

## Mitigation landed (2026-08-24) — the harness always runs a zero-skew agent

`scripts/xrce-agent/build.sh` (the single publish point behind
`xrce_agent_binary_path()`'s highest-priority slot) now builds the agent
against the PRESENT ROS install's own Fast-DDS whenever a sourced
environment provides one: prefix taken from `AMENT_PREFIX_PATH` (the
RFC-0075 only-what-the-user-named rule), agent tag derived from the
installed Fast-CDR MAJOR itself (`libfastcdr.so.1` → v2.4.2, `.so.2` →
v2.4.3 — a library fact, not a distro-name table), built with
`UAGENT_USE_SYSTEM_FASTDDS/FASTCDR=ON`, published as a wrapper that
pins `LD_LIBRARY_PATH` at the paired prefix. Stamped idempotent;
offline clone failure falls back to the bundled prebuilt; the no-ROS
path is untouched (nothing to skew against).

Verified on THIS host — the originally red one: `ldd` shows both
libraries resolving from `/opt/ros/humble`, and
`cargo nextest run --test xrce_ros2_interop --retries 0` is **8/8,
twice** (plus the issue's own test solo, 2.7 s). The deterministic
`28-into-15` refusal does not occur under zero skew.

Still open per this issue's own bar: the two-hosts-red / two-envs-green
split under the PINNED agent remains unexplained, and the bus capture
armed above is still the instrument for it. The mitigation removes the
skew that makes the failure possible; it does not yet name the trigger.
The `nros setup` integration (edition-aware provisioning in the CLI
proper, per "The shape this suggests") is follow-up work — the harness
funnel was the smallest surface that closes the lane.

## Investigation state — NOT fixed

Ruled out so far, by reading:

* **The type names are correct.** `xrce_dds_reply_type()` produces
  `<Service>_Response_` and `xrce_dds_request_type()` produces
  `<Service>_Request_` (`session.c`), which is what ROS 2 expects. The topic
  keeps the `Reply`/`Request` suffix while the type uses `Response`; both
  spellings are handled.
* **We never advertise a size.** `create_service` declares the replier with
  `uxr_buffer_create_replier_bin(...)` passing type NAMES only — no type
  description and no max-serialized-size. So the 15 is not a number nano-ros
  computes and gets wrong; it is inferred downstream, by the Agent or by
  Fast-DDS.

That is where this stops. The next question is where the reply reader's history
size actually comes from — the Agent's dynamic type registration under a bin
profile, or the ROS 2 client sizing its reader from a discovered writer's
advertised max size — and answering it needs Agent-side inspection rather than
more reading of this repo.

Worth keeping in view: the sibling `test_xrce_action_ros2_client` and
`test_xrce_to_ros2_pubsub` PASS in the same run. Whatever is wrong is specific to
the service REPLY path, which is a strong constraint on any proposed cause.

## Agent-side investigation (2026-08-24) — the 15 is NOT the Agent's

Vendored Agent is **v2.4.3** at `third-party/xrce/agent`. Traced the bin/replier
path:

* `FastDDSMiddleware::create_replier_by_bin` fills `ReplierAttributes` from the
  binary profile and sets `subscriber.topic.topicDataType = request_type()` and
  `publisher.topic.topicDataType = reply_type()` — type NAMES only, matching what
  nano-ros sends.
* `FastDDSReplier::create_by_attributes` then creates a datawriter on the reply
  topic and a datareader on the request topic.
* The Agent's generic type support is `TopicPubSubType`, and its size is a
  constant:

```cpp
TopicPubSubType::TopicPubSubType(bool with_key) {
    m_typeSize = 1024 + 4 /*encapsulation*/;
```

**So the Agent advertises 1028 bytes, not 15.** Whatever sizes that reply
reader's history to 15, it is not the Agent's type registration — which is the
hypothesis this issue was pointing at, and it is now eliminated.

That leaves the ROS 2 client side. `rmw_fastrtps` creates the reply reader from
its OWN `AddTwoInts_Response` type support, so 15 has to come from either that
type's computed max serialized size or from a discovered-endpoint negotiation.
Note 15 is not a plausible size for that type either (8-byte `sum` + 4-byte
encapsulation = 12), so something is computing it from the wrong type
descriptor.

### Still not fixed, and the next step is not in this repo

Three of the four layers are now excluded by direct inspection: nano-ros never
advertises a size, the type names are correct, and the Agent advertises 1028.
The remaining layer is `rmw_fastrtps`/Fast-DDS on the ROS 2 client, which needs
Fast-DDS-side inspection (or a Fast-DDS log at discovery showing what max size
the reply reader negotiated).

The host split is the strongest remaining clue and should be used: two hosts
reproduce and two environments do not, so comparing their Fast-DDS and
`ros-humble-rmw-fastrtps-cpp` versions is likely faster than reading further —
whatever changed between those versions is a candidate for whether the reader
resizes or refuses.

## Fast-DDS version RETRACTED as the axis (2026-08-24)

The version hypothesis was mine, it was confident, and it is wrong.

This host ran `fastrtps 2.6.11` while the passing environment recorded
`2.6.12` — a single patch release, on the one component neither the agent pin
nor `rmw_fastrtps_cpp` could explain, and the symptom (`RTPS_READER_HISTORY …
cannot be resized`) is reader-history sizing, which lives in Fast-DDS. It fit.

Upgraded and re-tested with fixtures rebuilt in the same window:

| | before | after |
| --- | --- | --- |
| `ros-humble-fastrtps` | 2.6.11 | **2.6.12** |
| `ros-humble-rmw-fastrtps-cpp` | 6.2.10 | 6.2.10 |

```
Summary [11.872s] 1 test run: 0 passed, 1 failed
[RTPS_READER_HISTORY Error] Change payload size of '28' bytes is larger than
the history payload size of '15' bytes and cannot be resized.
```

11.9 s, so the fixture built and both peers ran — not the sub-second STALE bail
that has produced four uninterpretable results in this issue already. The
versions now match the passing environment exactly and the failure is
byte-identical. **Fast-DDS version is not the differentiator.**

## Four layers now excluded by direct evidence

1. nano-ros never advertises a size — the replier is declared with
   `uxr_buffer_create_replier_bin`, type NAMES only.
2. The type names are correct (`<Service>_Response_` / `_Request_`).
3. The Agent advertises **1028** (`TopicPubSubType::m_typeSize = 1024 + 4`), not 15.
4. Fast-DDS / rmw_fastrtps versions match a passing environment and it still fails.

And yet two hosts reproduce while two environments do not. Something
environmental still differs and it is none of the components anyone has checked.

## Next, cheapest first

* **Which agent binary actually runs.** This host resolves the SDK-store pin
  `2.4.3-nros1` (`~/.nros/sdk/xrce-agent/`), while `third-party/xrce/agent` is
  plain v2.4.3. If the two environments run different builds of the agent, that
  is a difference nobody has compared — and the agent is where the reply topic's
  type gets registered.
* **`fastcdr`** — 1.0.29 here, never recorded for the passing host.
* **A Fast-DDS log at reader creation.** This is the only step that stops
  guessing: it shows directly what max serialized size the reply reader
  negotiated and from which discovered endpoint. Every hypothesis so far has
  been an inference from a number nobody has watched being computed.

Recommend the third. Four eliminations in, inference has a poor record on this
issue.

## The advertising path, measured in the API (2026-08-26)

Attempted the "advertise the computed bound" fix and stopped before writing it,
because the shape of the work and the evidence disagree. Recording the API facts
so nobody repeats the search.

**The BIN profile cannot carry a size, at any level.** All three checked in the
vendored client headers, not inferred:

| call | carries |
| --- | --- |
| `uxr_buffer_create_topic_bin` | topic name, type name. No size. |
| `uxr_buffer_create_replier_bin` | service name, request/reply type + topic names, `uxrQoS_t`. No size. |
| `uxrQoS_t` | `durability`, `reliability`, `history`, `depth`. No size. |

`subscriber.c`, `publisher.c` and `service.c` all use those BIN calls. So there
is no field anywhere in the current registration path to put a bound in, and the
Agent supplies its own default — which is where the 15 comes from.

**What advertising would actually cost.** Switch entity creation to the XML
profile (`uxr_buffer_create_topic_xml` / `..._replier_xml`, both present in the
headers), generate DDS topic XML stating the type's max serialized size, and
plumb that size across our C ABI. Subscriptions have somewhere to put it —
`rmw_subscription_options_t::rx_buffer_hint`, which today carries the LOCAL
buffer size and which the XRCE backend ignores entirely. Services have no
equivalent field at all. Also unconfirmed: whether
`UCLIENT_PROFILE_CREATE_ENTITIES_XML` is compiled into our client build.

**Why it was not written.** This issue reproduces 1 run in 13 on the failing
host and 0 in many on three other environments. A missing advertisement is
IDENTICAL on every host, so it cannot by itself explain a split — the same
objection that retired the Fast-DDS version and agent-pairing hypotheses. A
backend change of this size, justified by a hypothesis the host evidence
disfavours and unfalsifiable on a machine that cannot reproduce the failure, is
the wrong trade.

**What would justify it.** Either (a) the failing host shows the Agent
advertising a size our registration did not supply — the bus snapshot this issue
already arms is the place to look — or (b) someone reproduces on demand and can
A/B an XML-profile build against it. Absent one of those, advertising is a real
improvement to make on its own merits and should be filed as such rather than as
this issue's fix.

## 0776 is CLOSED, and it moves this issue's next step (2026-08-26)

The cross-link below said the investigation kept hitting a hole: nothing in this
tree computes a serialized size bound. Phase 380 filled it —
`nros_serdes::size` computes one from the schema, and `schema::Message` carries
`MAX_SERIALIZED_SIZE_XCDR1` / `_XCDR2` per type.

**That does not fix this issue, and it sharpens why.** The cross-link's
conclusion — "nothing computes anything, so the 15 comes from a peer's default"
— survives intact, because computing a number and ADVERTISING one are different
things. Verified in the API rather than inferred:

```c
uint16_t uxr_buffer_create_topic_bin(
        uxrSession* session, uxrStreamId stream_id, uxrObjectId object_id,
        uxrObjectId participant_id,
        const char* topic_name,
        const char* type_name,          /* <- names only */
        uint8_t flags);
```

`subscriber.c` and `publisher.c` both create topics through that BIN profile,
which carries a topic name and a type name and no size at all. So even now that
we can compute the bound, there is nowhere in the current registration path to
put it, and the Agent keeps filling in whatever default it filled in before.

**The actionable next step this makes possible.** Carrying a size means creating
topics through the XML profile (`uxr_buffer_create_topic_xml`) with a DDS topic
XML that states the type's max serialized size, now that
`M::MAX_SERIALIZED_SIZE_XCDR2` exists to state. That is a real change to the
XRCE backend's entity creation, not a log-line tweak, and it should be measured
against the failing host rather than assumed: this issue reproduces 1 run in 13
here and not at all on three other environments, whereas a missing advertisement
is identical everywhere — so advertising may well be correct AND not be the
whole story.

## Cross-link: issue 0776 is the missing piece this investigation kept hitting

Filed independently, and it names exactly the hole every hypothesis here ran
into: **nothing in this tree computes a message's serialized size bound.**
Upstream has `rmw_get_serialized_message_size(typesupport, bounds, size_t *out)`;
we have no equivalent, no generated `MAX_SERIALIZED` constant, and buffers are
sized by integrator-guessed env knobs rather than a bound derived from the type.

That reframes the four eliminations recorded above. Each one asked "who
advertises 15?" and answered "not this layer" — nano-ros does not advertise a
size at all (bin profile, names only), the type names are correct, the Agent
advertises a flat 1028, and the Fast-DDS versions match a passing environment.
The reason the question kept coming back unanswered is that **no layer here
computes the number**, so there is nothing to be wrong in the way the
investigation assumed.

It does not immediately explain the host split — two hosts reproduce, two do not,
and a missing capability is the same on all four. But anyone resuming this should
read 0776 first: it is the difference between "find the bug that computes 15" and
"nothing computes anything, so the 15 comes from a peer's default".

## Re-measured 2026-08-26 — 1 failure in 13, and three hypotheses eliminated

This host reproduced the symptom exactly once and has not since. The number
matters because this issue has been reasoning from "deterministic on reproducing
hosts", which is not what it does here.

```
[RTPS_READER_HISTORY Error] Change payload size of '28' bytes is larger than
the history payload size of '15' bytes and cannot be resized.
  -> Function can_change_be_added_nts
```

| run | result |
| --- | --- |
| first run after a fresh `build-test-fixtures lane=native` | **FAIL, 15.9 s** |
| 6 consecutive, no changes | 6 pass, ~2.8 s each |
| 5 consecutive under CPU load (6 spinners) | 5 pass |
| first run after ANOTHER full fixture build | pass, 5.1 s |

**1 / 13.**

### Eliminated by controlled runs

* **CPU load / slow scheduling** — 5/5 pass under six spinning load generators.
  The failing run took 15.9 s against a 2.8 s steady state, so slowness looked
  causal; induced slowness does not reproduce it.
* **First-run-after-rebuild** — the one failure was the first run after a
  rebuild, which is a tempting story. Tested directly: a second full fixture
  build followed immediately by the test PASSED. Refuted.
* **Stale Fast-DDS shared memory** — `/dev/shm` holds 99 `fastrtps_*` segments,
  all dated five days earlier. They were present during the failure AND during
  all twelve passes, so they separate nothing.

### The contamination this issue should know about

Before the rebuild, this test had been answering with an ABSORBED STALE verdict
for three days — `NOT RUN: 7th consecutive stale verdict for this fixture, first
3d ago` (issue 0445). A STALE verdict replaces whatever the fixture would have
done at runtime.

So any "does not reproduce" recorded on a host whose fixture was stale is not
evidence of non-reproduction — the test never ran. Several of this issue's
green measurements were taken across hosts and dates without recording fixture
freshness, and at least the ones from such a window cannot be read as greens.
That may be part of why "two hosts reproduce, two do not" never resolved into an
axis: some of the greens may be absences rather than passes.

This bears on the open question the 0776 cross-link above leaves — "it does not
immediately explain the host split". If the split is partly an artifact of
unrecorded fixture staleness plus a 1-in-13 rate under `retries = 2`, there may
be no host axis to explain: four environments sampling a rare intermittent
failure, some of them not actually running the test, would produce exactly the
two-and-two pattern. That is a hypothesis, not a finding — but it is cheaper to
test than another layer sweep, and it costs only that future measurements record
fixture freshness, retry count, and the number of runs.

Issue 0764 (fixed 2026-08-25) removed one large source of false STALE, so
measurements taken from now on are cleaner than the ones above it in this file.

### And sweeps mask it

`.config/nextest.toml` gives `binary(xrce_ros2_interop)` **`retries = 2`**. At a
1-in-13 rate a sweep will almost always show FLAKY-then-pass rather than a
failure, which is consistent with the tier-1 run that logged this test as
`FLAKY 3/3`. Anyone hunting this must run with `--retries 0`, as the Reproduce
section already says — and should not read a green sweep as evidence.

### What this does not do

It does not find the 15. The next step this issue names — a Fast-DDS log at
reader creation — still needs a failing run to log, and 1-in-13 makes that
expensive to catch. Raising `FASTDDS_LOG_LEVEL=Info` produced no additional
Fast-DDS output on a passing run, so the log capture will need to be armed
inside the harness and left on, rather than run by hand until it trips.
## 54 orphaned `add_two_ints_server` processes were live on this host (2026-08-26)

Housekeeping that bears directly on this issue's open hypothesis.

`ps` on the green host found **54 `add_two_ints_server` processes**, all
reparented to init, the youngest 56 minutes old and the oldest running **five
days**. Every one of them is a DDS participant that has been sitting on the bus
across an unknown number of test runs.

That is precisely the foreign-peer hypothesis this issue arrived at — "a second
XRCE agent or ROS node already running, holding the reply topic with a
differently-registered type" — and issue 0707's class ("an orphan from the last
run joins the next one"), which was filed FROM this failure.

Two things follow, and they point in opposite directions:

* **It does not explain the failure here**, because this host is the GREEN one:
  8/8 with all 54 orphans live. If a stale participant were sufficient to cause
  the 15-byte history, this host should have been red for days.
* **It does mean every measurement taken on this host — green or red — was
  taken with 54 unaccounted participants on the bus.** The bus-snapshot arming
  added on 2026-08-21 prints what the test can see at failure time; nothing has
  ever recorded what was on the bus during a PASS.

The orphans are now killed. Anyone re-measuring on either host should check
`pgrep -c -f add_two_ints_server` first — a clean bus is a precondition this
issue never stated and never had.

## Fifth environment, 2026-08-26 — green, plus a wire baseline and a defect in
## this issue's own instrumentation

Ran the issue's command on a fifth host/tree after `just setup-cli` +
`build-test-fixtures-leaves native`: `test_xrce_service_ros2_client` passes and
the whole binary is **8/8**, solo AND in-sweep. Five green environments against
one red.

(My first attempt "reproduced" nothing — it hit `Test fixture is STALE`, which
is the discriminator this issue already records. Worth noting it caught me too.)

### The number this issue has never had from a green host

`tshark -i lo -f udp` during a passing run, then reading the SEDP announcements:

| topic | `PID_TYPE_MAX_SIZE_SERIALIZED` |
| --- | ---: |
| `rq/add_two_intsRequest` | **20** |
| `rr/add_two_intsReply` | **1028** |

20 is exactly the request the issue predicts (16-byte request header + 4-byte
encapsulation). **1028 is 1024 + 4 — a fixed buffer bound, not a computed type
size.** The red host refused a 28-byte sample into a 15-byte history; a green
host advertises 1028, so 28 fits with room to spare.

**Where the bound comes from matters.** `uxr_buffer_create_topic_bin` sends only
a topic name and a type name — no size crosses the XRCE wire at all. So neither
1028 nor 15 originates in nano-ros: **the AGENT chooses it** when it creates the
DDS type. That makes the agent's type registration the variable, and it is
consistent with 15 being a bound some other agent build computes.

### The provenance line can lie, and it lied here

`scripts/xrce-agent/build.sh` publishes to `build/xrce-agent/MicroXRCEAgent` two
different ways: a genuine ROS-paired build, **or** an 85-byte forwarding wrapper

    #!/bin/sh
    exec "/…/.nros/sdk/xrce-agent/2.4.3-nros1/bin/MicroXRCEAgent" "$@"

around the SDK store agent, which bundles its own Fast-DDS.
`xrce_agent_binary_with_provenance()` classified by PATH alone, so on this host
the run printed

    xrce agent: …/build/xrce-agent/MicroXRCEAgent — built against the sourced ROS
                (no Fast-DDS skew)

while executing the bundled-Fast-DDS agent — the exact skew it claims to
exclude.

**This undermines the measurement in the section above.** "ROS-paired agent
(2.6.11) -> 8/8 pass / SDK store agent (2.14.6 skew) -> 8/8 pass" was offered as
proof that skew alone does not cause the failure. If that host's
`build/xrce-agent/` entry was also a wrapper, both arms ran the SAME binary and
the comparison measured nothing. It needs re-running with the corrected
provenance before its conclusion can stand.

Fixed here: provenance now reads the file's CONTENT and resolves the wrapper to
its target, so a wrapper reports `SdkStore` with the skew warning. The matcher is
deliberately narrow (the exact two-line shape `build.sh` emits) so an unrelated
shell script is not silently reinterpreted.

### What the red host should measure next

1. `ls -la build/xrce-agent/MicroXRCEAgent` — 85 bytes means the wrapper, and
   every "ROS-paired" arm recorded on that host is suspect.
2. tshark the failing run and read `rtps.param.type_max_size_serialized` for
   `rr/add_two_intsReply`. If it is 15 rather than 1028, the defect is the
   agent's type registration and the client is an innocent bystander.
3. Compare the agent binaries by content, not by path.


## phase-414 W5 (2026-09-03): the routing verdict, and the title is inverted

**Stays in phase-414. NOT phase-303.** The phase said to move it if the cause
turned out to be encoding. It is not: this is wire FRAMING of the request/reply
mapping, outside our serializer — not XCDR2/extensibility (phase-303's class,
which is parked and concerns our own message types' DHEADER).

**This issue's central premise is refuted.** It reads:

> 15 bytes is not a plausible `AddTwoInts_Response` … so the reader was created
> against a type whose advertised max size is wrong

15 is exactly right. `rmw_fastrtps` computes `m_typeSize = align4(4 + data_size)`
= 12, and Fast-DDS sizes reader history as `payloadMaxSize = m_typeSize + 3` =
**15**. Confirmed independently by five environments whose clients accept the
reply and print `sum=8`, which is only possible if the correct reply is <= 15
bytes. **Nothing on the receiving side is undersized — the sample is oversized
by 16 bytes.** The title says "payload too small"; it should say the reply is
too big.

### Where the 16 bytes come from — MEASURED up to the agent, INFERRED across it

nano-ros cannot produce a 28-byte reply. The chain, all read:

* `service-server/src/lib.rs:52` -> `ctx.reply::<AddTwoIntsResponse, 64>()`
* `nros/src/node.rs:1956` — serialize with a 4-byte CDR header, len = **12**
* `nros-node/src/executor/arena.rs:2561` — `send_response(seq, &buf[..12])`
* `xrce/src/service.c:376` — strip the 4-byte header, body = **8**
* `micro-xrce-dds-client .../write_access.c:50` — prefix a **24-byte
  SampleIdentity**, so the XRCE wire payload is 24 + 8 = **32**

The Agent must strip that 24-byte prefix into `related_sample_identity` and
publish 8 bytes; its `TopicPubSubType` adds the 4-byte encapsulation. Correct
output = **12**. Observed = **28**, and `28 = 4 + 24`: the Agent forwarded 24
bytes of body where it should have forwarded 8, consuming only 8 of the
SampleIdentity and leaking the other 16 into the DDS payload. (This issue's own
`4 + 16 + 8` decomposition is the same 16 bytes.)

That is INFERENCE across the agent boundary — `third-party/xrce/agent` is
uninitialised here, so `FastDDSReplier::write` was not read. It fits the one
intervention that ever flipped a red host green: rebuilding the agent against
the sourced ROS's Fast-CDR.

**Not fixed by 0819**, though every reproduction here predates it. 0819
(`446ba0643` / `8a220ec76`, both 2026-08-27) is a receive-side fragment-boundary
defect with a cliff at the 4096-byte MTU; this exchange is 12-36 bytes, one
fragment. Different class. Still worth re-measuring on current main — the last
data is over a week old.

### FIXED here: this issue's own mitigation was unreachable

`ca224e271` (2026-08-24) added the fix — build the agent against the sourced
ROS's Fast-CDR — and `scripts/xrce-agent/build.sh` keys its stamp on
`"$agent_ref $ros_prefix"` correctly. But `just xrce setup` short-circuited on
`[ -x "$adir/MicroXRCEAgent" ]` and never called it, so **any host that had ever
published an agent kept the skewed one**. A file-existence test cannot see a ROS
prefix change.

MEASURED on this host: `build/xrce-agent/MicroXRCEAgent` is the 85-byte
pre-mitigation wrapper dated Aug 23, forwarding to a store agent with Fast-DDS
2.14.6 / Fast-CDR 2.2.7, against `/opt/ros/humble`'s 2.6.12 / 1.0.29. The exact
skew the mitigation exists to remove, still live, nine days after it landed.

The recipe now always delegates. Same shape as the rest of this session: a
mechanism that was correct and unreachable from the path a real user takes.

### Still open

The **1-in-13 intermittency** is unexplained by either candidate cause, exactly
as this issue keeps concluding. A second candidate — a foreign CycloneDDS-mapped
`/add_two_ints` server on the same domain, which would also produce `4 + 16 + 8`
— is rated lower: a foreign writer's refused sample would not stop our own valid
12-byte reply from being accepted.

**Next, and it is one packet capture** — the question this issue has never
asked. Pre-checks first (seconds): confirm the agent is no longer the 85-byte
wrapper, and that no orphaned `add_two_ints_server` is on the bus. Then capture
a failing run: if our XRCE `WRITE_DATA` payload is 32 bytes and the DDS reply
sample is 28, the Agent framed it wrong and the surplus 16 will match the tail
of the SampleIdentity we sent; if the 28-byte sample's writer GUID is not the
Agent's participant, it is a foreign peer and the fix is domain isolation.

## 2026-09-03 re-measured on a ZERO-SKEW agent: the failure SURVIVES

**0741 cannot be closed as "the mitigation was never applied."** It was not
applied — and applying it does not fix this.

### The pairing, verified rather than assumed

| | before (85-byte wrapper -> SDK store) | after `just xrce setup` | ROS peer |
| --- | --- | --- | --- |
| Fast-DDS | 2.14.6 | **2.6.12** | 2.6.12 |
| Fast-CDR | 2.2.7 | **1.0.29** | 1.0.29 |

`ldd` on both the executable and `libmicroxrcedds_agent.so`: same library FILES
as `/opt/ros/humble`, same major, same build. Zero skew. Bus clean before and
after all 66 runs — no orphans.

### Counts, `--retries 0`

| batch | runs | pass | fail |
| --- | ---: | ---: | ---: |
| A (paired, as shipped) | 15 | 14 | **1** |
| B (paired + logger, stopped at first fail) | 6 | 5 | **1** |
| C (paired + logger) | 45 | 45 | 0 |
| **total** | **66** | **64** | **2** |

Batch A alone clears the >=13 bar: 14 of 15. Observed ~1 in 33, the same order
as the historical ~1 in 13. Failing runs are byte-identical to the historical
symptom, and the fingerprint line now truthfully reads `built against the
sourced ROS (no Fast-DDS skew)`.

### MY OWN INFERENCE IS REFUTED

The W5 section above inferred that the Agent consumed only 8 of the 24-byte
SampleIdentity and leaked 16 into the DDS payload. **It did not. In the failing
run the Agent never received the DDS request and never wrote a DDS reply at
all.**

Failing trace (37 lines): session opened, participant created, replier created,
client `READ_DATA` sent — then NOTHING for ~8 s. `read_fn=0`, `write=0`. The
client's 28-into-15 error is timestamped ~1.43 s AFTER the agent's last line.

Passing trace (52 lines), for contrast:
`Replier.cpp read_fn [==>> DDS <<==] len: 40` (24 identity + 16 request) ->
forwarded 52 XRCE -> client replies 44 XRCE (12 + 24 + 8) ->
`Replier.cpp write [** <<DDS>> **] len: 32`, split 24/8 correctly -> 4+8=12,
under the 15-byte limit.

Truncation excluded: SIGKILLing the agent after two lines still leaves all bytes
on disk (the sink flushes per message) and neither log is a multiple of 4096.
The failing trace is COMPLETE, not cut off.

So the `28 = 4 + 24` arithmetic still fits the story — but **this Agent did not
perform it**. The remaining question changes shape: the client's request never
reached the Agent's request reader, AND something wrote a 28-byte sample on the
reply topic that the Agent did not write. That reads as an
**endpoint-matching/discovery anomaly, not a serialization one**.

Who wrote the sample is UNKNOWN. The bus snapshot shows only
`/add_two_ints_server`, but a bare Agent participant is not a ROS node and would
not appear there, so that snapshot cannot exclude a foreign DDS participant.

### FIXED here: the only non-root instrument was compiled out

`scripts/xrce-agent/build.sh` passed `-DUAGENT_LOGGER_PROFILE=OFF` at BOTH build
sites, so the agent it publishes emits nothing for `-v6`.
`NROS_XRCE_AGENT_VERBOSE` and `NROS_TEST_LOGS` were **silently inert against the
very agent the mitigation ships** — the harness passed the flag, the agent said
nothing, and an empty log read as "nothing to report" rather than "this binary
cannot report". `config.hpp` line 31 read `/* #undef UAGENT_LOGGER_PROFILE */`.

The traces quoted above only exist because a logger-enabled agent was built by
hand for the experiment. That should not have been necessary.

Now: `NROS_XRCE_AGENT_LOGGER=1` selects it, the value is in the STAMP so
flipping it REBUILDS rather than silently reusing the other flavour, and the
derivation sits at file scope so BOTH build paths see it — deriving it inside
the paired branch left the fallback expanding to an EMPTY `-D`, which is the
unresolved-knob shape CLAUDE.md warns about. Verified: `config.hpp` flips
between `#define` and `#undef`, and the default OFF build is idempotent.

The test side no longer lies either: requesting `-v6` now prints what an empty
log would mean and names the rebuild.

### Also noted, not fixed

`forwarding_wrapper_target` (`xrce_agent.rs`) is coupled to the paired wrapper's
TEXT — it returns `None` today only because the exec line starts with
`LD_LIBRARY_PATH=`. If `build.sh` ever emits a bare `exec "..."`, provenance
flips to `SdkStore` and the resolver hands back the inner binary directly,
bypassing the `LD_LIBRARY_PATH` the wrapper exists to set. That is issue 0774's
class, one seam over.

### Next

A DDS capture on a failing run answers it in one field: **the writer GUID of the
28-byte sample.** Agent's participant -> the Agent framed it wrong despite
logging nothing; anything else -> foreign peer, and the fix is domain isolation.
`tshark`/`dumpcap` are absent here and `tcpdump` has no capabilities, so this
needs root and was not attempted.

Non-root alternative worth trying first: force the client's reply reader to
`PREALLOCATED_WITH_REALLOC` via `FASTRTPS_DEFAULT_PROFILES_FILE` +
`RMW_FASTRTPS_USE_QOS_FROM_XML=1`, so the sample is accepted and its CONTENT
becomes visible. Caveat: it changes the client's QoS and may mask the failure.

## RESOLVED 2026-09-03 — the 28-byte sample is a FOREIGN peer's, on another host

**It was never our defect.** Not the agent pin, not Fast-CDR skew, not type
registration, not the missing size advertisement, not the reply framing. The
15-byte reader history is correct, our reply is 12 bytes, and the 28-byte
sample comes from a machine this repo does not run on.

### The repro this issue never had, and it uses none of our code

    $ ROS_DOMAIN_ID=1 ros2 service call /add_two_ints \
        example_interfaces/srv/AddTwoInts "{a: 5, b: 3}"
    [RTPS_READER_HISTORY Error] Change payload size of '28' bytes is larger than
    the history payload size of '15' bytes and cannot be resized.

3 of 3 on re-verification here, 5 of 5 in the original measurement. **No
nano-ros process, no XRCE agent, nothing of ours running.** Domain 9 is empty
and is the control.

### The writer, identified

A CycloneDDS `add_two_ints_server` on **`arm-a100` = 10.2.15.142** (this host is
10.2.15.118), squatting **35 ROS domains** including 1-5. Writer GUID
`0110ba9bee88c773100c4688.00001503` — `0110` is the Cyclone vendor id — peer
`10.2.15.142:52724`, topic `rr/add_two_intsReply`, payload 28 B:

    00010000 | 0500000000000000 0300000000000000 0000000000000000

That is the **rmw_cyclonedds** request/reply mapping `[client GUID 8][seq 8]
[response 8]`. The Cyclone server read our Fast-DDS request — which carries only
`a=5, b=3` in the payload, with the identity in inline QoS `PID 0x800f
RELATED_SAMPLE_IDENTITY` — as its own header, hence `guid=5, seq=3, sum=0`. The
two service mappings are not interoperable. Bytes measured; that reading
inferred.

Captured without root: `tshark`/`dumpcap` are absent and `tcpdump` has no
capabilities, so an `LD_PRELOAD` interposer on `sendto`/`recvfrom` logged the
RTPS datagrams instead, plus a passive SPDP scanner that binds the discovery
multicast ports and sends nothing.

### It also explains the intermittency — which was never randomness

Request-side, from the wire: a PASSING run writes the request to BOTH
`127.0.0.1:7661` (our Agent) and `10.2.15.142:58782`. A FAILING run writes it
**only** to the foreign peer. That is the `read_fn=0 / write=0` agent trace from
the section above, confirmed from outside the agent.

`wait_for_service` is satisfied by whichever server is seen first; when our
Agent's replier is not yet matched at request-write time (RELIABLE + VOLATILE),
only the foreign reader gets it.

And the "1 fail then 45 green" shape has a mechanism: `dds_bus_snapshot` ran
`ros2 node list` etc. WITHOUT `--no-daemon`, so every failing run left a ros2
daemon on that domain, `domain_discovery_port_busy` read the port as busy, and
the next run stepped to the NEXT domain — walking the test off the poisoned
domains onto domain 9, which has no foreign peer and can never fail. A
reproduction of that walk went 1 -> 5 -> 9 exactly.

**So every measurement in this issue, mine included, was taken on a bus whose
occupants nobody had enumerated.**

### What is being FIXED, and what is not

Fixed here: `dds_bus_snapshot` now passes `--no-daemon` like its three siblings,
so a failing run stops walking itself off the evidence. Necessary, not
sufficient — that probe could not have caught this anyway, because `ros2 service
list` collapses a service to one NAME however many servers offer it, so the
"SECOND `/add_two_ints`" its own comment hopes for never appears as a row.

Filed as **issue 1009**: our interop bus is not isolated from the LAN, and
`ROS_LOCALHOST_ONLY=1` alone is measured NOT to fix it — 0 of 15, because the
XRCE Agent is a bare Fast-DDS application and ignores the variable, so the two
sides stop discovering each other. A `FASTRTPS_DEFAULT_PROFILES_FILE` with an
`interfaceWhiteList` of `127.0.0.1`, exported for BOTH processes, is measured at
15 of 15 on a poisoned domain.

Not covered: the 35 orphans on `arm-a100`, which need access to that host.

### For the record, the wrong diagnoses this issue accumulated

Five, including one of mine. The reply-framing story (`28 = 4 + 24`, the Agent
mis-slicing a SampleIdentity) was mine, and it was refuted twice — first by the
agent trace showing it wrote nothing, then by this, showing it was never asked
to. Every one of the five was reached by reading code and return codes. The
thing that settled it was asking who else was on the wire.
