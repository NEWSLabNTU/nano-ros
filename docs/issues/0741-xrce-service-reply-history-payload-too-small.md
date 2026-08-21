---
id: 741
title: "`test_xrce_service_ros2_client` fails on main — Fast-DDS refuses the
  28-byte reply into a 15-byte history payload"
status: open
type: bug
area: rmw, testing
related: [issue-0736]
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
