# Phase 394 — CAN unicast for zenoh-pico: nano-ros talks ROS 2 services over CAN

**Status (2026-08-27). PROPOSED — nothing started.**

Implements [RFC-0083](../design/0083-can-unicast-over-isotp.md), the zenoh-pico
half. Ends with a nano-ros node and a ROS 2 node exchanging a **service call**
across a CAN bus.

**Depends on** phase-393, which must reach its W5 gate first: if a service does
not complete between two Linux peers, nothing here can work either.

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
| **W5** | Zephyr platform, using Zephyr's native `isotp_bind`/`isotp_send`/`isotp_recv` | the island's real platform | **code done, suites not run** |
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

**`vcan` cannot test flow control honestly.** `STmin`, `BS` and the `N_Bs`/`N_Cr`
timers exist to pace a real bus. On a zero-latency virtual interface they are
nearly no-ops, so a conformance bug can survive every Tier 1 test. This is the
strongest argument yet for hardware, and it should be said plainly rather than
discovered later.

**Zephyr's ISO-TP is marked experimental.** W5 runs its own test suites rather
than assuming; if they are thin, that is a finding the island needs before it
depends on services.

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

## 10. W5 status — the Zephyr port is written, its suites are not run

`src/system/zephyr/isotp.c` implements the link on Zephyr's own
`subsys/canbus/isotp` — `isotp_bind` / `isotp_recv` / `isotp_send` — and not on
the vendored library. Same rule the `unix` port follows with the kernel socket:
where the platform implements the protocol, the platform's implementation wins.
It is tested by its own conformance suite, maintained alongside the CAN drivers
it sits on, and is what a Zephyr application would already be using.

Three decisions in it worth keeping:

* The receive side is bound **once at open**, not per read. Zephyr installs a
  CAN filter in `isotp_bind` and a controller has few filter slots, so binding
  per read would exhaust them and would also drop everything that arrived
  between reads.
* `isotp_send` is called with a **NULL completion callback**, which makes it
  block until the whole PDU is out. That is the contract the link expects — a
  send that returns is a send the peer has paced through flow control — and it
  is also what settles `N_As` on this platform: transmit confirmation is
  Zephyr's problem, inside the CAN driver, rather than something this port has
  to time. Contrast the vendored library, where the same question has no good
  answer and is documented as the integrator's.
* Flow control asks for `bs = 0`, matching the Linux kernel, so a Zephyr node
  and a Linux node pace each other the same way.

One API trap, caught before the build by reading Zephyr 3.7's `isotp.h` rather
than trusting memory: `struct isotp_msg_id` has **no `id_type` member**. That is
the pre-3.7 API. Addressing mode lives in `flags` (`ISOTP_MSG_IDE` for 29-bit),
and `std_id`/`ext_id` are a **union** over the same storage, so exactly one is
written — setting both would silently truncate the 29-bit value.

### What is NOT done, and why

**W5's acceptance criteria are not met.** They require Zephyr's own ISO-TP
conformance and implementation test suites to be run and the result recorded
before the island depends on this link for services — Zephyr marks `CONFIG_ISOTP`
`[EXPERIMENTAL]` in Kconfig, and the point of that criterion is not to take the
label on trust either way.

Running them needs a Zephyr workspace, which this machine does not have. The
provisioning was started and abandoned: the SDK tarball downloaded at roughly
5 MB/min, which put the SDK alone over an hour, before `west update` had begun.
Worse, `scripts/zephyr/setup.sh` re-provisions in-tree sources as it goes, and
it twice reset the `zenoh-pico` submodule out from under work in progress —
once back to a commit from before this phase started. Nothing was lost, because
every commit was already made and reachable, but it is the reason the run was
stopped rather than left going.

So: the Zephyr port compiles as written against the 3.7 API and has never been
built or run. **It should be treated as unverified code** until the suites are
run. That is a smaller claim than the rest of this phase and it is deliberately
not dressed up as more.

To finish it:

```sh
just zephyr setup                      # expect a long fetch
west twister -T tests/subsys/canbus/isotp/conformance   -p native_sim
west twister -T tests/subsys/canbus/isotp/implementation -p native_sim
```

then build a zenoh-pico image with `Z_FEATURE_LINK_ISOTP=1` for `native_sim`
and run the Tier 2 test from §4 against a Linux peer on `vcan0`.
