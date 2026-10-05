---
id: 1674
title: "Zephyr native_sim + Cyclone DDS delivers nothing again — every e2e cell,
  C and C++, `received 0 sample(s)` — and each image floods
  `os_sockWaitsetWait: select failed` fast enough to OOM the test runner"
status: resolved
type: bug
area: [zephyr, rmw, testing]
severity: high
found: 2026-10-03
related: [0155, 0968, 1673]
---

## Symptom

Every Cyclone DDS e2e cell on Zephyr `native_sim` fails, in both languages, on
every retry:

| cell | result |
| --- | --- |
| `example_e2e::case_11_cyclonedds_c_pubsub_e2e` | FAIL 3/3, ~23 s |
| `example_e2e::case_12_cyclonedds_cpp_pubsub_e2e` | FAIL 3/3, ~23 s |
| `example_e2e::case_14_cyclonedds_c_service_e2e` | FAIL 3/3, ~28 s |
| `example_e2e::case_15_cyclonedds_cpp_service_e2e` | FAIL 3/3, ~28 s |
| `example_e2e::case_17_cyclonedds_c_action_e2e` | FAIL, ~80 s |
| `example_e2e::case_18_cyclonedds_cpp_action_e2e` | FAIL, ~80 s |

```
[cyclonedds/cpp/Pubsub] listener received 0 sample(s), expected ≥1
```

The eight `boot_smoke` cells for the same images PASS — they boot. Then they
deliver nothing.

Each image's console repeats one line:

```
[00:00:00.000,002] <inf> cyclonedds: cyclone: os_sockWaitsetWait: select failed, retcode = -
```

with the clock frozen at `00:00:00.000,002`, i.e. a busy loop that never
yields time.

## It is NOT issue 1673's change — measured with an A/B

Found while verifying 1673, which changed these images' build. To separate the
two, the C++ talker and listener were rebuilt from `origin/main`'s own
`CMakeLists.txt` (1673's generator change is a no-op whenever an example's hand
descriptor block is present, so this reproduces `main` exactly) and the one cell
re-run:

```
example_e2e::case_12_cyclonedds_cpp_pubsub_e2e   FAIL [22.622s]
[cyclonedds/cpp/Pubsub] listener received 0 sample(s), expected ≥1
```

Identical. The failure predates 1673 and does not depend on it. The C images
could not be checked the same way: on `main` they do not LINK (that is 1673).

## It has happened before

Issue **0155** — "Zephyr+CycloneDDS lane: images boot, net ready, then silence —
no publish, no error" — is this symptom, and is resolved. So this is a
regression of a fixed defect, not a new one. It may also be among the ~12
unreproduced tier-2 runtime failures issue **0968** counts; this is a
reproduction of at least six.

## The flood is a hazard of its own

One `nextest` run of these six cells produced **566,291,852** copies of that
line — a **55 GB** captured log — and `cargo-nextest`'s in-memory capture grew
to **91 GB** resident before the kernel OOM-killed it:

```
Out of memory: Killed process … (cargo-nextest) … anon-rss:91374324kB
```

So a single failing Cyclone native_sim cell can take down the machine it runs
on, and every other session sharing it. Independently of the delivery fix:

* the Zephyr Cyclone log line inside that loop wants rate-limiting at the
  source, or the loop wants to yield;
* the e2e harness should bound what it captures from a guest console.

A bounded reproduction that is safe to run, used for the A/B above:

```sh
timeout 300 cargo nextest run -p nros-tests --cargo-profile nros-relwithdebinfo \
  -E 'binary(zephyr) and test(=example_e2e::case_12_cyclonedds_cpp_pubsub_e2e)' \
  --no-capture --retries 0 2>&1 | grep -v "select failed"
```

`--no-capture` streams instead of buffering, and the filter keeps the flood off
the disk.

## Cause — measured, 2026-10-05

**Zephyr's `select` refuses Cyclone's socket waitset as too large.**
`subsys/net/lib/sockets/sockets_select.c` copies the fd set into
`struct zsock_pollfd pfds[CONFIG_NET_SOCKETS_POLL_MAX]` and, one fd past that,
returns `-1` with `errno = ENOMEM` before any socket is asked anything. Zephyr's
default is **3** (`default 3`, nothing in the tree raised it). Cyclone's receive
loop treats the failure as transient and retries at once — hence the busy loop,
the frozen clock and the flood.

Not a patch reversion like #0155's first cause: the NSOS patches (getsockname,
getifaddrs, recvmsg) are applied in the workspace, and the Cyclone fork carries
the self-pipe as a commit (`4aa337b0`).

**How many fds the waitset holds, read off the host** (NSOS sockets are host
sockets, so `strace` sees them): `epoll_ctl(…, EPOLL_CTL_ADD, …)` registers
exactly **four** — the TCP self-pipe's read end and three UDP sockets: discovery
unicast (`0.0.0.0:13160`), data unicast (`0.0.0.0:13161`), and one bound to the
enumerated interface address (`127.0.0.1:0`). That is with multicast already
off: the native_sim baseline in `cyclone_config.hpp` sets
`<AllowMulticast>false</AllowMulticast>`, and its own comment describes this very
symptom ("the multicast RX fd select()s as failed") — phase 180 kept the count at
three by turning multicast off.

**The cap boundary, measured on one image:** 3 fails, **4 works, 5 works** —
`select failed` 2988 lines → 0, and the talker publishes.

**When the fourth socket arrived — leading candidate, not bisected:**
`63dfd2507a` (2026-07-17), "real net_if enumeration in zephyr
`ddsrt_getifaddrs`", nine days after #0155 was resolved. Once `getifaddrs`
returns real interfaces, Cyclone binds a socket to the chosen interface address,
which is the one socket here bound to `127.0.0.1`. If so, every Zephyr Cyclone
native_sim e2e cell has delivered nothing since mid-July, behind fixtures nobody
re-ran (`lane=all` had never reached these cells on this host).

## Fix — three layers, each covering what the others cannot

1. **`zephyr/Kconfig`: `configdefault NET_SOCKETS_POLL_MAX` → 8** under
   `NROS_RMW_CYCLONEDDS`, beside the module's other Cyclone defaults. 8 is the
   measured 4 plus room for what this config does not do but others do —
   multicast adds sockets, so does a second participant — at a cost of
   `8 x sizeof(zsock_pollfd)` on the select caller's stack. A board or prj.conf
   can still override it. A fresh configure, measured, resolves it to 8.
2. **`CONFIG_NET_SOCKETS_POLL_MAX=8` in all 22 `prj-cyclonedds.conf`.** Needed
   because **a Kconfig default never reaches an existing build dir**: Zephyr's
   kconfig step logs `Loaded configuration '<build>/zephyr/.config'` and keeps
   the recorded value while the merged conf fragments are unchanged — every
   incremental Cyclone fixture stayed at 3 after step 1 alone. A fragment change
   moves the merged set, so the dirs regenerate on their own. These files are the
   3.7 path's established home for Cyclone's structural values
   (`NET_PKT_RX_COUNT`, `DYNAMIC_THREAD_STACK_SIZE`, …).
3. **A compile-time floor in the backend** (`session.cpp`, under `__ZEPHYR__`):
   `CONFIG_NET_SOCKETS_POLL_MAX < 4` is `#error … (issue 1674)`. Any image a site
   misses — a new example, a board that lowers it — fails to BUILD naming this
   issue, instead of booting into the flood. Measured: with steps 1 and 3 only,
   every stale build dir stopped exactly there.

## Verified

* All **18** Cyclone Zephyr images (C, C++, Rust × talker/listener/service
  client+server/action client+server) rebuild INCREMENTALLY — no reset — and each
  resolves to 8 and relinks.
* **19 / 19 Cyclone cells pass** on native_sim, with `select failed` appearing
  **0** times: all nine e2e cells (pubsub, service and action in C, C++ and Rust)
  plus the boot smokes. Before: all six C and C++ e2e cells failed.
* **Bonus:** the C and C++ Cyclone SERVICE e2e cells pass — the residual #0157 was
  split off as "never delivers a reply". Not re-attributed here; noted so the
  next reader of #0157 runs them.

## Still open, recorded rather than fixed

* (issue 1696) **The busy loop itself** is in Cyclone's receive thread, which retries a failed
  `select` immediately and logs each time. A back-off there would keep the next
  cause of a failing `select` from flooding a console. That is a change to the
  Cyclone fork, which the agent does not push; left for the maintainer.
* (issue 1697) **The harness capture bound.** `cargo-nextest` buffered 91 GB of one cell's
  console before the kernel killed it. Bounding what the e2e harness captures
  from a guest console is a separate fix.
* (issue 1698) **`<err> os: tid 0x… is in use!` ×5 at every Cyclone boot** —
  `k_thread_stack_free` refusing to free the stacks of Cyclone's running threads.
  Present with the fix and the cells pass, so it is not this defect; it reads as
  a stack-handling mismatch between Zephyr's dynamic threads and ddsrt's thread
  teardown, and leaks rather than corrupts.
* (issue 1699) **The module's other `configdefault` values are unproven on existing dirs.**
  Every one visible in a Cyclone `.config` is ALSO hard-set by the per-example
  `prj-cyclonedds.conf`, so nothing shows them reaching an incremental build —
  by step 2's mechanism, they would not.
