# Phase 394 — CAN unicast for zenoh-pico: nano-ros talks ROS 2 services over CAN

**Status (2026-08-27). COMPLETE — all eight waves done, on `vcan0`.**

Implements [RFC-0083](../design/0083-can-unicast-over-isotp.md), the zenoh-pico
half. A nano-ros node and a ROS 2 node exchange **services, actions, action
cancellation and parameters** across a CAN bus, in both roles.

Services, actions and cancellation run with **no router and no TCP endpoint
anywhere in the path**. Parameters need a persistent peer on the CAN side and so
use a router there — the nano-ros node's only link is still ISO-TP, and §7
records why (`ros2 param` is a series of short-lived processes, and ISO-TP
addresses a peer by a directed pair that exactly one peer may own).

| | over ISO-TP, nano-ros ↔ ROS 2 | bus |
| --- | --- | --- |
| service, nano-ros serves | `sum=42` | 140 frames |
| service, nano-ros calls | `2 + 3 = 5` | 135 frames |
| action, nano-ros serves | `SUCCEEDED`, `[0,1,1,2,3,5]` | 534 frames |
| action, nano-ros calls | `[0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]` | 536 frames |
| action **cancel** | both ends agree | 526 frames |
| parameters (via a router on the bus) | `list` + `get 120` + `set 250` + `get 250` | 834 frames |

**The claim this phase set out to test.** "ROS services do not work over CAN"
was never true of CAN. It is a property of zenoh's **multicast** transport:
queries route to unicast faces only, so the RFC-0080 link carries topics and
nothing built on a query. The same failure reproduces on stock ROS over UDP
multicast. Give CAN a real unicast face — ISO 15765-2 — and services, actions,
cancellation, parameters and graph introspection all come back.

**What is NOT tested: timing.** Everything here ran on `vcan`, which has no bit
rate and no arbitration. See §5 — this is the one open risk and it is now
supported by two independent sources.

**Depends on** phase-393, the zenoh-rs half, which is complete.

---

## 1. Shape

ISO-TP comes from the platform, never from zenoh-pico. That is the whole design.

```
zenoh-pico:  third-party/isotp-c/                     vendored, MIT, pinned
             include/zenoh-pico/system/link/isotp.h   the platform contract
             src/link/config/isotp.c                  endpoint parsing
             src/link/unicast/isotp.c                 capabilities, lifecycle
             src/system/unix/network.c                CAN_ISOTP socket
             src/system/zephyr/network.c              native isotp_bind/send/recv
             src/system/<other>/network.c             the three vendored hooks
             CMakeLists.txt                           Z_FEATURE_LINK_ISOTP, default 0
```

## 2. Waves

Ordered so the vendored library is proven on a platform with a reference
implementation to disagree with, before it is trusted on one without.

| | What | Proves | State |
| --- | --- | --- | --- |
| **W0** | Vendor `SimonCahill/isotp-c` at a pinned commit, with its MIT licence; wire into the build | the dependency is present, attributed and reproducible | **done** |
| **W1** | `unix` platform: implement the link over the **kernel** `CAN_ISOTP` socket | the pico link works where the kernel is the reference implementation | **done** |
| **W2** | `src/link/unicast/isotp.c` + config; register in `_z_open_link` **only** | pico connects out and never touches the accept path | **done** |
| **W3** | zenoh-pico ↔ zenoh-rs over `vcan0`: session, pub/sub, and a **query** | the two implementations agree on the wire | **done** |
| **W4** | `unix` platform on the **vendored library** instead of the kernel socket | the vendored ISO-TP is conformant against the kernel as reference | **done** |
| **W5** | Zephyr platform, using Zephyr's native `isotp_bind`/`isotp_send`/`isotp_recv` | the island's real platform | **done** |
| **W6** | **nano-ros node ↔ ROS 2 node: a service call over CAN** | the reason this phase exists | **done** |
| **W7** | Extend the demo container to show it | the artifact reviewers can run | **done** |

**W4 is the interesting one.** Implementing `unix` twice — once on the kernel,
once on the vendored library — makes the kernel the oracle for the library on
the one platform where both exist. Any divergence is a library bug found on a
laptop instead of on a board.

## 3. Acceptance criteria

**W0.**
* The vendored tree records upstream URL, commit and licence, and the MIT
  `LICENSE` file is present verbatim.
* No GPL or AGPL code enters the tree. `devcoons/iso15765-canbus` (AGPL-3.0) and
  `altelch/iso-tp` (GPL-3.0) are the near misses; the survey in RFC-0083 §3
  records why each was rejected so nobody re-litigates it.
* The library builds for at least one MCU target with no allocator.

**W1.**
* A pico peer opens an ISO-TP link on `vcan0` and exchanges PDUs with a
  zenoh-rs peer.
* `_z_link_get_socket` returns the real descriptor — unlike the multicast CAN
  link, which cannot and which is what broke RFC-0080's unicast attempt.

**W2.**
* The link is registered in `_z_open_link` and **not** in `_z_listen_link`.
  A test or a comment states why: pico dials out, and `_zp_unicast_accept_task`
  is a listen-side path it must never enter.
* An endpoint with `tx_id == rx_id` is refused.
* With `Z_FEATURE_LINK_ISOTP=0` the image is byte-identical to today's.

**W3.**
* A pico peer and a zenoh-rs peer establish a unicast session over `vcan0`.
* Pub/sub works both directions with a payload past the MTU.
* **A query completes** — a pico `z_get` against a zenoh-rs queryable, or the
  reverse. This is the first evidence the ceiling is gone.

**W4.**
* The same W3 tests pass with the vendored library substituted for the kernel
  socket on `unix`.
* Any behavioural difference from the kernel is characterised and written down
  rather than worked around silently.
* `N_As` is addressed explicitly: if the platform's send is asynchronous, either
  the hook blocks until transmit-confirm or a timer is added. RFC-0083 §3 notes
  no portable library supplies this.

**W5.**
* The Zephyr port uses Zephyr's own ISO-TP, not the vendored library.
* Zephyr's implementation is `[EXPERIMENTAL]` in its Kconfig; its conformance
  and implementation test suites are run, and the result recorded, before the
  island depends on it for services.

**W6.** The gate.
* A nano-ros node and a ROS 2 node exchange a **service call** over CAN, in both
  roles: nano-ros as client, and nano-ros as server.
* No router and no TCP endpoint anywhere in the path.
* `candump` capture recorded.

**W7.**
* `docker/can-demo/` gains a unicast mode showing the service call.
* The README stops saying services do not work over CAN, and says instead which
  link carries which semantics.

## 4. Test method

**Tier 1 — `vcan0`, one laptop.** Everything through W4. The kernel and the
vendored library on the same interface, compared directly.

**Tier 2 — `native_sim`.** Zephyr's ISO-TP against a Linux peer, as phase-377
did for the multicast link.

**Tier 3 — hardware.** MR-CANHUBK344 to a Linux host. Still untouched, and now
carrying more weight: ISO-TP flow control has timing behaviour that `vcan`, with
no bit rate at all, cannot exercise even in principle.

## 5. Risks

**`vcan` cannot test flow control honestly. CONFIRMED, and it is the one risk
this phase did not retire.** `STmin`, `BS` and the `N_Bs`/`N_Cr` timers exist to
pace a real bus; on a zero-latency virtual interface they are nearly no-ops, so
a conformance bug in the timing behaviour survives every test available here.
Two independent sources now say so:

* every Tier 1 result in this document ran on `vcan`, which has no bit rate;
* **Zephyr's own conformance suite skips `stmin` in all four configurations**
  (§10) — its authors reached the same conclusion about simulated buses.

Tier 3 hardware is the only thing that closes this. Nothing in this phase should
be read as evidence about latency, bandwidth or arbitration.

**Zephyr's ISO-TP is marked experimental. RESOLVED, with a caveat.** Its
conformance and implementation suites pass, 77 of 85 cases, 0 failures (§10).
The caveat is the 8 skips, and `stmin` is among them — see above.

**Two implementations of `unix` is deliberate duplication.** It is a testing
oracle, not an accident, and the phase should keep both rather than delete the
kernel path once the library works — the day the library regresses, the oracle
is what finds it.

## 6. W1–W3 result

Done, on branch `phase-394-can-unicast-over-isotp` in the zenoh-pico submodule
(`ca7ce9a9`). zenoh-pico opened a session to zenoh-rs over `vcan0`, delivered
pub/sub, and **a query returned a reply** — the capability the multicast CAN
link of RFC-0080 cannot provide.

`candump` during the query shows ISO 15765-2 exactly as specified:

```
vcan0  200   [8]  10 18 C1 09 F2 32 F5 35     FirstFrame, FF_DL = 0x018 = 24
vcan0  201   [3]  30 00 00                    FlowControl, CTS, BS=0, STmin=0
vcan0  200   [8]  21 17 AC D2 78 DD 5C 42     ConsecutiveFrame, SN=1
vcan0  200   [8]  22 2B C1 F0 0A FF DD 0A     ConsecutiveFrame, SN=2
vcan0  200   [5]  23 00 08 27 01              ConsecutiveFrame, SN=3
```

Two bugs, both from taking the multicast CAN link as the template when the
relevant difference is that this link is **unicast**:

* `_z_link_get_socket` had no `_Z_LINK_TYPE_ISOTP` case, so it fell to
  `default:` and returned `NULL`. CAN and IVC return `NULL` legitimately — they
  have no descriptor to wait on — but the *unicast* transport dereferences the
  result without a NULL check (`_z_new_transport_client`), so this was a
  segfault during session open, after a handshake that had otherwise succeeded.
* `_z_f_link_read_socket_isotp` was a stub that logged and returned `SIZE_MAX`,
  again copied from CAN, where a read must filter on the receive identifier and
  so cannot go through a bare descriptor. ISO-TP binds the identifier pair into
  the socket, and the unicast transport reads through `_read_socket_f` on every
  inbound batch. Fixed by adding `_z_read_isotp_socket`, an fd-only entry point
  that `_z_read_isotp` also delegates to.

One trap worth recording, because it cost a debugging cycle and will recur:
**`include/zenoh-pico/config.h` is generated into the source tree and is also
checked in.** Configuring the build rewrites it, so `git checkout -- config.h`
to tidy the worktree silently removes `Z_FEATURE_LINK_ISOTP`, and the next
`cmake --build` — which does not re-run configure — compiles the link out. The
symptom is `Unable to open session!` with no ISO-TP logging at all, which reads
like a link bug. Re-run `cmake -S . -B <dir>` before each build and revert the
generated files only at commit time. The same applies to `library.json`,
`zenohpico.pc` and `include/zenoh-pico.h`, which configuring also rewrites —
in this tree it reverted a deliberate Zephyr socket-timeout carve-out that had
nothing to do with this phase.

A second trap, recorded because it briefly looked like a bug in the link. The
reply to a query appeared to arrive only 1 run in 5, with the zenoh-rs queryable
logging `Responding` every time — a convincing "the reply is lost on the way
back". It was not: the harness kills its children rather than letting them exit,
and **block-buffered stdout is discarded on SIGTERM**, so a run that worked
perfectly logged nothing. Under `stdbuf -o0` it is 3/3. `scripts/test/isotp-pico-interop.sh`
runs everything unbuffered for this reason.

## 7. W6 result — the gate

A ROS 2 service call served by a nano-ros node, over CAN:

```
requester: making request: example_interfaces.srv.AddTwoInts_Request(a=20, b=22)

response:
example_interfaces.srv.AddTwoInts_Response(sum=42)
```

`scripts/test/isotp-ros-interop.sh` runs it, in **both roles** and with **no
router and no TCP endpoint anywhere in the path**:

```
role SERVER: ros2 service call  <--ISO-TP over CAN-->  nano-ros service server
role CLIENT: nano-ros client    <--ISO-TP over CAN-->  ros2 add_two_ints_server
```

Both peers load a session config derived from the installed rmw_zenoh default
with the connect endpoint (the local `rmw_zenohd`) removed and the TCP listener
*replaced* by the ISO-TP one — replaced, not appended, because leaving a TCP
listener would let the two processes find each other without the bus and prove
nothing. The harness greps the generated config to prove no TCP endpoint
survived, rather than asserting it in a comment.

`candump` over one run of each: 140 frames / 11 FirstFrame–FlowControl pairs
for the server role, 135 / 9 for the client role.

The client role checks for the example's own result line, `Result of
add_two_ints: 5`. An earlier version grepped the client log for `42` — the
number the *server* role uses — and passed on a substring of a liveliness
keyexpr, which is random hex. It reported a pass on a run whose real result it
had never looked at.

An early version of that guard grepped the config for `"tcp/` without stripping
comments and killed a good run: the stock json5 is heavily documented and its
own prose contains `"tcp/10.10.10.10:7447"` as an example.

### Actions, both roles

W6's criteria name a service call, but a service is the easy case. An **action**
is a whole conversation — goal request, accept, a stream of feedback, a result
request, and a status topic — so it drives queries, replies **and** pushed data
across the same ISO-TP face at once. Both roles pass, on
`example_interfaces/action/Fibonacci` against ROS 2's own minimal action nodes:

| role | result | bus |
| --- | --- | --- |
| nano-ros **serves** `/fibonacci`, `ros2 action send_goal` drives it | `Goal finished with status: SUCCEEDED`, `[0,1,1,2,3,5]`, 6 feedback samples | 534 frames, 35 FF/FC pairs |
| ROS 2 serves, nano-ros **client** drives it | `Result received: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]`, 9 feedback samples | 536 frames, 26 FF/FC pairs |

`--role action-server` / `--role action-client`; `--role all` runs all four.

Both action assertions were wrong on their first outing and reported FAIL on
runs that had in fact completed correctly — worse than no assertion:

* `ros2 action send_goal` prints the result as a **YAML block list**
  (`sequence:` then `- 0`, `- 1`, …), not `sequence=[0, 1, ...]`. The role now
  matches the terminal status plus the server's own `Goal succeeded`.
* order = 10 yields **eleven** terms ending **55**, not twelve ending 89. The
  role now matches the exact expected sequence, so an off-by-one in either
  implementation fails instead of passing on a substring.

One operational note: the action roles are minutes long, and a leftover peer
from an earlier run on the same identifier pair will stall them. Check the bus
is clear before believing a hang.

### Parameters

`ros2 param` is **six services** (get / set / list / describe / get_types /
set_atomically) plus the `/parameter_events` topic — the widest service surface
in ROS, and none of it exists on a multicast face. Full round trip against a
nano-ros node whose only link is ISO-TP:

```
ros2 node list      -> /param_talker
ros2 param list     -> publish_period_ms
ros2 param get      -> Integer value is: 120
ros2 param set 250  -> Set parameter successful
ros2 param get      -> Integer value is: 250
```

834 frames, 50 FirstFrame–FlowControl pairs. `scripts/test/isotp-ros-params.sh`
asserts the **round trip** rather than any single call: a `get` alone would pass
against a node that ignores `set` entirely.

Two things this test needs that the service and action ones do not:

* **A router on the CAN side.** Every `ros2 param` invocation is a short-lived
  process with its own session. Pointing those straight at the identifier pair
  makes each one a new listener on it while the node is still reconnecting to
  the last, and the handshake collides — `Received invalid message instead of an
  OpenSyn`. ISO-TP addresses a peer by a *directed pair*, so exactly one peer may
  own an end; a persistent router is what provides that. It is also how a real
  deployment looks: an MCU on the bus, a router on the vehicle computer. The
  node's only link is still ISO-TP, so the parameter traffic still crosses CAN.
* **`NROS_ENTRY_SPIN_MS=forever`.** The launch arm of `nros::main!` runs an
  env-gated *bounded* spin, prints `nros: application complete` and exits. Without
  it the node is gone before any `ros2 param` call arrives, and the only symptom
  is an empty graph — which reads like a transport failure and is not one.

The node is `native_rust_params_entry` from `examples/workspaces/features`,
whose `system.toml` already declares `features = ["param_services"]`; it gained
a `link-isotp` passthrough so the CAN link can be switched on from the command
line.

### Action cancel

The happy path never touches the cancel service. `--role action-cancel` does:
ROS 2's own cancel client sends a goal, cancels it, and both ends have to agree
across the bus.

```
ros2:     Sending goal            t+0.000
ros2:     canceling goal          t+3.011
nano-ros: Publish feedback
nano-ros: Goal canceled
ros2:     Goal was canceled       t+3.023
```

526 frames, 35 FirstFrame–FlowControl pairs. The assertion needs **both** ends:
the client alone would print `Goal was canceled` for a request that timed out on
its own side, so the server's own line is what says the cancel actually crossed
CAN and was honoured.

Two things had to change in `examples/native/rust/action-server` first, and the
first is a plain bug:

* **A cancel the server accepted was then reported as `Succeeded`.** `on_cancel`
  answered `CancelResponse::Accept` and `tick` completed the goal as `Succeeded`
  regardless — a lie the client cannot detect. `tick` now sees `Canceling` in the
  goal status it already collects, and finishes the goal as `Canceled` with
  whatever sequence had been computed.
* **The goal has to still be running when the cancel arrives.** Measured: ROS 2's
  cancel client cancels **exactly 3.0 s** after sending, and the server computed
  all eleven terms in the single tick that saw the goal — about four
  milliseconds. The cancel path was unreachable, not broken. The server now
  supports emitting one term every `NROS_FIB_STEP_TICKS` ticks; the knob is
  read with `option_env!` because the crate is `no_std`, and **defaults to 0 =
  the old all-in-one-tick behaviour**, so every existing test is unaffected —
  confirmed by re-running `--role action-server` on a default build and getting
  the same `[0,1,1,2,3,5]` and the same 534 frames.

**This is what RFC-0080 could not do.** zenoh routes queries to unicast faces
only, so a service call over the multicast CAN link never reaches a queryable
at all. It is not a CAN limitation and never was — it is a property of zenoh's
multicast transport, and giving CAN a real unicast face removes it.

What it took, beyond the pico link itself:

* `scripts/can/build-zenohc-can.sh` grew a `--link can|isotp` selector. The
  ISO-TP `libzenohc.so` is built from the `feat/can-unicast-isotp-ros` fork
  branch, which is version 1.8.0 — the version the installed
  `zenoh_cpp_vendor` ships, and the script refuses to build a mismatch. It is
  substituted by `LD_LIBRARY_PATH` alone: no ROS rebuild, because a cargo
  feature adds no C API.
* `link-isotp` on `zpico-sys`, forwarded by `nros-rmw-zenoh`, plus an `isotp`
  field through `LinkFeatures` / `LinkPolicy` so the generated pico config
  header carries `Z_FEATURE_LINK_ISOTP`. Deliberately separate from
  `link-can`: they are different links, not two modes of one.

Two harness details worth keeping. Humble's `ros2 service call` has **no
`--no-daemon` flag** — it is not a universal ros2 option — so the harness stops
the daemon instead; a stray daemon inherits the environment and holds a session
on the bus after the test. And the harness sources ROS itself rather than
trusting the caller's shell, with no `set -u` anywhere, because `setup.bash`
dereferences unset variables and aborts under it.

## 8. W0 and W4 result — the vendored library and its oracle

**W0.** `SimonCahill/isotp-c` at `abb9e552df0e7ca0148c146124795341d57124fe`,
MIT, vendored to `zenoh-pico/third_party/isotp-c/` with provenance and the
licence verbatim in `VENDOR.md`. Only the six library files are taken; upstream's
build system, tests and submodules are not.

`scripts/can/isotp-c-mcu-check.sh` builds it for a bare-metal Cortex-M4 and
fails if an allocator appears. It does not: the caller supplies both buffers to
`isotp_init_link`. What it does need from outside is the three hooks plus
`memcpy`, `memset`, `__assert_func` and `snprintf` — the last two from the
assert and debug paths, both removable with `-DNDEBUG` and a discarding
`isotp_user_debug`, neither on a hot path.

`isotp_config.h` is the one vendored file that is edited, because it is
upstream's designated configuration point: `ISO_TP_USER_SEND_CAN_ARG` is turned
on so the send hook knows which socket to use. `isotp.c` and `isotp.h` stay
verbatim. The vendored translation unit is also exempted from this project's
`-Werror=conversion` — it assigns `uint8_t` into 4-bit bitfields deliberately,
and patching upstream to silence that would make the copy non-verbatim and have
to be redone at every bump.

**W4.** `src/system/unix/isotp_vendored.c` implements the same four platform
functions on a **raw** SocketCAN socket plus the vendored library, selected by
`Z_FEATURE_LINK_ISOTP_VENDORED=1`. The W3 tests pass unchanged against it:
pub/sub delivered, and a query returned a reply, with a kernel-ISO-TP zenoh-rs
on the other end as the reference implementation.

### The one behavioural difference, characterised

Same workload, same peer, `candump` on both:

| | frames | flow control pico sends |
| --- | --- | --- |
| kernel `CAN_ISOTP` | 80 | `BS=0x00 STmin=0x00` |
| vendored `isotp-c` | 81 | `BS=0x08 STmin=0x00` |

Both are conformant. They differ in the **block size** a receiver asks for:
the kernel says `BS=0`, meaning "send the rest of the PDU without stopping",
while the library's `ISO_TP_DEFAULT_BLOCK_SIZE` is 8, meaning "eight
consecutive frames, then wait for me again". The cost is one extra round trip
per eight frames — for a full 4095-byte PDU, roughly 73 of them.

This is left at 8 rather than matched to the kernel. On the platforms the
vendored library actually serves, the receiver is an MCU with a small buffer and
a slow CPU, and being able to pace the sender every eight frames is the point of
the mechanism. Set `ISO_TP_DEFAULT_BLOCK_SIZE` to 0 in `isotp_config.h` to match
the kernel where the receiver can take it.

### N_As

`isotp_user_send_can` returns when the frame has been handed to the driver, not
when the bus has confirmed it, so the ISO 15765-2 `N_As` timer cannot be
measured from inside it. RFC-0083 §3 records that no portable ISO-TP library
supplies this. The `unix` port does **not** add a timer: a `write()` to a
SocketCAN socket either queues the frame or fails, and the failure it can
actually produce — `ENOBUFS`, a full transmit queue — is reported back as
`ISOTP_RET_NOSPACE` so the library retries. A port on a platform with a genuinely
asynchronous transmit must block in the hook until transmit-confirm or add its
own timer; this is written here so that port does not have to rediscover it.

### A stray process, again

The first W4 run failed with every frame acknowledged twice: two flow-control
frames for one first frame, two first frames back. That reads exactly like a
duplicate-send bug in the library under test. It was not. An
`add_two_ints_server` left over from the W6 client role had been sitting on
`vcan0` for eleven minutes answering flow control on the same identifier pair.

`ros2 run` is a wrapper that execs the node as a child, so killing the pid it
returns leaves the node alive. The harness now `setsid`s each child and kills
the process **group**. This is the third time in this campaign that a stray
process has produced a convincing false result; the lesson that keeps paying is
to check what else is on the bus before believing a finding about the code.
## 9. W7 result — the demo container

`docker/can-demo/` now carries **both** links in one `libzenohc.so`, built from
`feat/can-links-ros` in the zenoh fork — a merge of the two ROS-based branches
whose every conflict was the two of them adding their own entry to the same
list. One artifact, because the demo's point is the contrast.

```sh
docker/can-demo/run.sh --zenoh <fork> --unicast
```

runs **both halves of the argument in one container**:

```
3u. the SAME service call over the MULTICAST link -- expected to fail
    multicast service call: rc=124, replies=0
4u. the same call over the ISO-TP UNICAST link
    server: isotp/vcan0#tx_id=0x201;rx_id=0x200
    client: isotp/vcan0#tx_id=0x200;rx_id=0x201
5u. results
    example_interfaces.srv.AddTwoInts_Response(sum=42)
    ISO 15765-2 on the wire:
      first frames  (1x): 19
      flow controls (3x): 19
```

and asserts both: the unicast call must return `sum=42`, and the multicast call
must return nothing. A demo that showed only the working case would leave the
reader to take the broken case on trust — and the failing half is the more
surprising claim, so it is the one that needs demonstrating.

The existing modes still pass on the same image: the default multicast topic
demo, and the `--negative` control that puts the two ROS peers in disjoint
identifier bands and asserts the listener hears nothing.

`--unicast` needs the `can-isotp` kernel module on the host as well as `vcan`.
`run.sh` checks for it before building, so the failure names the host command
that fixes it rather than surfacing inside the container.

The README and the demo's own closing summary no longer say services do not
work over CAN. They now say which link carries which semantics, and that the
multicast restriction is a property of zenoh's multicast transport rather than
of CAN.
## 10. W5 result — the Zephyr port, its suites, and what they do not cover

**Done.** The port is `src/system/zephyr/isotp.c`, on Zephyr's own
`subsys/canbus/isotp` — `isotp_bind` / `isotp_recv` / `isotp_send` — and not on
the vendored library. Same rule the `unix` port follows with the kernel socket:
where the platform implements the protocol, the platform's implementation wins.

`CONFIG_NROS_ZENOH_LINK_ISOTP` in `zephyr/Kconfig` `select`s `ISOTP` and maps to
`Z_FEATURE_LINK_ISOTP` through the existing `_nros_configure_zenoh_feature`
bridge. Verified in the generated `.config`: setting it alone brings up
`CONFIG_ISOTP=y`.

### It compiles

All three ISO-TP translation units build for `native_sim/native/64` against
Zephyr 3.7.0 with `Z_FEATURE_LINK_ISOTP=1`: `src/link/config/isotp.c`,
`src/link/unicast/isotp.c`, and `src/system/zephyr/isotp.c`.

One real bug, and the compiler found it. The port was written against
`struct isotp_msg_id` as it looked **before** Zephyr 3.7 — an `id_type` member
set to `ISOTP_STD_ADDR` / `ISOTP_FIXED_ADDR`. In 3.7 there is no `id_type`:
addressing mode is in `flags` (`ISOTP_MSG_IDE` for the 29-bit identifier), and
`std_id`/`ext_id` are a **union** over the same storage, so exactly one may be
written or the 29-bit value is silently truncated. Reaching for a removed member
fails to build rather than misbehaving on a bus, which is the good outcome and
the one the code's own comment had predicted for this mistake.

### Zephyr's own suites pass

```
5/5 native_sim/native/64  tests/subsys/canbus/isotp/{conformance,implementation}  PASSED
77 of 85 test cases executed, 8 skipped, 0 failed
```

Run with `scripts/twister -T <zephyr>/tests/subsys/canbus/isotp -p native_sim/native/64`.
Note the board qualifier: plain `-p native_sim` selects the 32-bit variant and
fails to build on a host without 32-bit glibc headers, naming
`bits/libc-header-start.h` rather than the real cause.

**The 8 skips are the interesting part**, and they are the reason this criterion
existed rather than taking `[EXPERIMENTAL]` on trust either way:

| skipped case | in | why it matters |
| --- | --- | --- |
| `stmin` | **all four** conformance configs | the separation-time pacing |
| `canfd_rx_dl_validation` | the two non-FD configs | CAN FD length validation |
| `canfd_mandatory_padding` | the two non-FD configs | CAN FD padding |

`stmin` is skipped in **every** configuration. That is the same hole §5 already
names for `vcan`: a simulated bus has no bit rate, so `STmin`, `BS` and the
`N_Bs`/`N_Cr` timers are nearly no-ops and a conformance bug in the timing
behaviour survives every test available here — Zephyr's own included. Tier 3
hardware is the only thing that closes it, and this is now the second
independent source saying so.

### Tier 2 — done

A `native_sim` image and a Linux zenoh-rs peer share a host `vcan0` over ISO-TP.
`scripts/test/isotp-zephyr-tier2.sh`:

```
[tier2] PASS: 59 /chatter messages Zephyr -> CAN -> Linux zenoh peer
        741 frames, 68 FirstFrame-FlowControl pairs
```

The Zephyr side's only link is the bus. `native_sim`'s devicetree chooses a
loopback CAN controller that never leaves the process, so
`cmake/zephyr/native-sim-can-host.overlay` enables the
`zephyr,native-linux-can` node instead and points it at `vcan0`.

Three things had to be fixed first, and **none of them were about CAN**:

* **Zephyr 3.7's `posix_features.h` omits `ARG_MAX`, `CHILD_MAX` and
  `IOV_MAX`** while `sysconf.h` references them, so
  `lib/posix/options/sysconf.c` does not compile. The Kconfig default is
  `POSIX_SYSCONF_IMPL_FULL if CPP`, so this hits every C++ Zephyr app with
  `POSIX_SINGLE_PROCESS` — an upstream bug, not ours. It was pre-existing here
  too: the same example with every CAN option removed failed identically.
  `cmake/zephyr/posix-sysconf-minimal-libc.conf` selects the macro
  implementation, which does not reference them.
* **`setvbuf` and `_IO*BF` do not exist in Zephyr's minimal libc**, and 35
  files call `setvbuf(stdout, NULL, _IONBF, 0)`. Minimal libc's stdout is a
  per-character hook, so it is already unbuffered and the call is a no-op there.
  `zephyr/libc-compat/nros_libc_compat.h` supplies both, force-included, keyed
  on `_IONBF` so picolibc and newlib are untouched. (`CONFIG_PICOLIBC` was tried
  first: it is not selectable on `native_sim` here, and neither gap closes.)
* **The Zephyr ISO-TP port never called `can_start()`.** Ours, and the one that
  cost the most. A Zephyr CAN controller does not transmit until it is started,
  and a send on a stopped controller surfaces no error the ISO-TP layer can see:
  `isotp_send` queues the first frame, nothing reaches the bus, and the only
  symptom is `Reception of next FC has timed out` once a second while `candump`
  shows an idle interface. It reads like a missing peer rather than a local
  fault. The multicast CAN port had always started its controller, refcounted
  per device; the ISO-TP port now does the same.

One more trap, for whoever builds this next: **the locator address is the
Zephyr DEVICE name from the devicetree (`can`), not the host interface**. The
overlay is what maps that device onto `vcan0`. Getting it wrong fails at session
open with no frames and no diagnostic.

### A note on the working environment

This checkout is shared with other work. Three times during this phase the
`zenoh-pico` submodule was reset out from under an edit — twice by
`scripts/zephyr/setup.sh`, which re-provisions in-tree sources as it runs, and
once along with the `nros` checkout itself. Nothing was lost, because everything
was committed and pushed before each reset, but the `id_type` fix above was
silently discarded by one of them and had to be redone. The Zephyr work was
finished in a dedicated `git worktree` for that reason, and that is the
recommended way to do long builds here.

The SDK is worth one line too: `scripts/zephyr/setup.sh` fetches it with `curl`
by deliberate choice (documented in the script — a single spelling of "which SDK
does this host need" was worth more than the speed). On this connection that ran
at roughly 5 MB/min. Fetching the same tarball with `aria2c -x16` took 90
seconds, and the checksum in `nros-sdk-index.toml` verifies it either way.

---

## 11. Close-out — what is proven, what is not, what is next

### Proven

* The zenoh-pico ISO-TP link, on **three** platform backends: the Linux kernel
  `CAN_ISOTP` socket, the vendored `isotp-c` on a raw SocketCAN socket, and
  Zephyr's `subsys/canbus/isotp`. The first is the oracle the second is judged
  against; the third is the island's real platform.
* Every ROS 2 semantic that RFC-0080's multicast link could not carry:
  services, actions, action cancellation, parameters, graph introspection —
  nano-ros ↔ ROS 2, both roles.
* The reason the multicast link cannot carry them, demonstrated rather than
  asserted: `docker/can-demo/run.sh --unicast` runs the same service call over
  both links in one container and asserts the multicast one returns nothing.
* The vendored library is MIT, uses no allocator, and builds for a bare-metal
  Cortex-M4 (`scripts/can/isotp-c-mcu-check.sh`).
* **Tier 2**: a Zephyr `native_sim` image and a Linux zenoh-rs peer exchanging
  ROS topic traffic over ISO-TP on a shared `vcan0`
  (`scripts/test/isotp-zephyr-tier2.sh`).

### Not proven

* **Anything about timing.** See §5. `vcan` has no bit rate; Zephyr's own suite
  skips `stmin`. No latency, bandwidth or arbitration claim in this phase is
  supported by evidence.
* **More than one peer per bus.** ISO-TP addresses a peer by a directed
  identifier pair; nothing here tests several pairs sharing one physical bus,
  where arbitration and bus load start to matter.
* **Anything upstream.** No issue filed, no PR opened, ECA and `Signed-off-by`
  outstanding.

### Next, in the order that retires the most risk

1. **Tier 3 hardware.** MR-CANHUBK344 to a Linux host, on a real bit rate. This
   is the only item that closes the one open risk, and everything else is
   waiting behind an assumption it would test.
2. **Several peers on one bus.** Two identifier pairs first, then contention.
3. **Upstream.** An issue on `eclipse-zenoh/zenoh` describing the multicast
   query limitation, and PRs for the two link crates. The branches are pushed;
   the ECA and sign-off are the author's to give.
---

## 12. Branch inventory

Eight branches across four repositories, two sets, same names throughout.
**`feat/can-links`** is based on each project's upstream main and is the set to
open PRs from. **`feat/can-links-ros`** is based on what ROS actually ships and
is the set to build against today.

| repo | `feat/can-links` (upstream main) | `feat/can-links-ros` (ROS stable) |
| --- | --- | --- |
| `jerry73204/zenoh` | `93cf1b3e5` — on main, 1.10.0 | `bf01b3ac1` — on 1.8.0 |
| `jerry73204/zenoh-pico` | `75bbb28e` — on upstream main | `0fdd49ce` — on release/1.8.0 |
| `jerry73204/zenoh-c` | `0c401df8` — on main | `911db8e8` — on `05bd370`, the commit rmw_zenoh pins |
| `jerry73204/rmw_zenoh` | `a24b450` — on rolling | `5b4c693` — on humble |

`jerry73204/zenoh-pico`'s **`nano-ros`** branch (`8e08e8b8`) is separate from
both and is what this repository's submodule tracks. The two `feat/can-links*`
branches are PORTS of the same work onto clean upstream bases; nothing moved off
`nano-ros`.

**Merge order is fixed: zenoh → zenoh-c → rmw_zenoh.** The zenoh-c and rmw_zenoh
changes name features that do not exist until the link crates land in zenoh, and
each commit says so.

### The `-ros` branches carry one extra commit each, marked NOT PR MATERIAL

Both repoint a dependency at the fork so the set builds before anything is
upstream: `zenoh-c` points `zenoh` at `jerry73204/zenoh#feat/can-links-ros`, and
`rmw_zenoh` points `zenoh_c_vendor` at `jerry73204/zenoh-c#feat/can-links-ros`.
Drop those commits to make either a PR candidate.

### Verified

`colcon build --packages-select zenoh_cpp_vendor rmw_zenoh_cpp` on the humble
set: **2 packages finished, exit 0**, only pre-existing upstream `-Wswitch`
warnings. The vendored `libzenohc.so` carries both links, and a ROS 2 service
call over that artifact returns `sum=42` across 140 frames.

The `feat/can-links` set is **not** built, and cannot be until the crates are
upstream — it deliberately carries no fork repoint, because that is the hunk
that would sink the PR.

### The trap that cost the first colcon build

**`zenoh-c`'s `Cargo.toml` is GENERATED.** `CMakeLists.txt` runs
`configure_file(Cargo.toml.in -> Cargo.toml)` at build time, so the committed
manifest is an artifact on the cmake path and edits to it alone are silently
overwritten. The feature passthroughs went into `Cargo.toml` first; the build
then re-resolved `zenoh` from upstream main and failed with

```
package `zenoh-c` depends on `zenoh` with feature `transport_can`
but `zenoh` does not have that feature
```

which names the symptom and not the cause. Patch **`Cargo.toml.in`** — and both
of them, parent and `build-resources/opaque-types/`, for the same reason the
size probe needs both.

Related, and already documented in `scripts/can/build-zenohc-can.sh`: rewrite
the zenoh dependency rather than `[patch]`ing it. A patch section leaves the
original git source for cargo to resolve, and zenoh-c's build script invokes the
opaque-types sub-build with `--offline`, where resolving a branch from a cached
checkout fails outright.
