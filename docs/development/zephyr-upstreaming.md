# Zephyr patch upstreaming (Phase 205.B)

**Purpose.** Track the nano-ros Zephyr patches that are *generic Zephyr fixes*
(not nano-ros-specific) and stage everything a maintainer needs to open the
upstream PRs. As each lands upstream + ships in a tested Zephyr line, drop it
from `zephyr/patches.yml` + the `scripts/zephyr/*-patch.sh` equivalent and
narrow the tested-version matrix (less drift risk for the 205.A starter
template).

**This doc does not open PRs** — that is human follow-up (Zephyr CLA, a fork on
`zephyrproject-rtos/zephyr`). It packages the diffs, clean commit messages,
repro, and the post-merge cleanup so the PR is a copy-paste-and-rebase.

> Patches carry inline `/* nano-ros: … Phase 180.A … */` markers so we can find
> them in a workspace. **Strip those markers** when preparing the upstream
> branch — upstream wants a generic comment (or none). The commit messages below
> are already nano-ros-free.

## Status

| Patch (in-tree) | Files | Upstreamable | Upstream PR |
|---|---|---|---|
| `nsos-recvmsg` | `drivers/net/nsos_sockets.c` | **yes** | _not opened_ |
| `native-sim-ipproto-ip` (guest) | `drivers/net/nsos_socket.h`, `drivers/net/nsos_sockets.c` | **yes** | _not opened_ |
| `nsos-adapt-ipproto-ip` (host) | `drivers/net/nsos_adapt.c` | **yes** | _not opened_ |
| `nsos-getsockname` | `drivers/net/nsos_sockets.c` (+ adapt) | **yes (candidate)** | _not opened_ |
| `nsos-getifaddrs` | NSOS getifaddrs path | **yes (candidate)** | _not opened_ |
| `nsos-mcjoin-mreq` | `drivers/net/nsos_sockets.c` | **yes (candidate)** | _not opened_ |
| `pthread-mutex-unlock` | POSIX `k_mutex` | **no** (downstream-only) | n/a |
| `cargo-features` / `rust-cargo-extra-args` / per-arch rust target | `modules/lang/rust/**` | **yes** (zephyr-lang-rust) | _not opened_ |
| cyclonedds-on-Zephyr (threads, log-flush, sockwaitset, udp-rcvbuf, mcjoin) | cyclonedds fork | **yes** (eclipse-cyclonedds) | baked in fork pin |

Each in-tree patch exists in two forms that must stay in sync: a `.patch`
(`zephyr/patches/<name>-4.4.patch`, `west patch`, Zephyr 4.x) and an
anchor-based script (`scripts/zephyr/<name>-patch.sh` / `*-4.4.sh`, also covers
3.7 LTS which has no `west patch`). Upstreaming removes the *need* for both.

---

## PR 1 — NSOS: implement `recvmsg()`

**Target:** `zephyrproject-rtos/zephyr` → `drivers/net/nsos_sockets.c`.
**Source diff:** `zephyr/patches/nsos-recvmsg-4.4.patch` (script:
`scripts/zephyr/nsos-recvmsg-patch.sh` 3.7 / `-4.4.sh` 4.4).

**Commit message:**
```
drivers: net: nsos: implement recvmsg()

The Native Simulator Offloaded Sockets (NSOS) driver leaves nsos_recvmsg()
an ENOTSUP stub. Datagram users that read via recvmsg() (one iovec +
msg_name — the common UDP receive shape) therefore get a hard failure on
every receive and busy-spin the receive thread.

Implement the single-iovec form by delegating to nsos_recvfrom(), reusing
its poll/block handling and host<->guest sockaddr translation. Multi-iovec
recvmsg() remains unsupported (returns ENOTSUP) — no NSOS user needs it yet.

Signed-off-by: <your name> <your email>
```

**Repro / test (before):** any native_sim app doing UDP `recvmsg()` logs
`UDP recvmsg sock N: ret 0 retcode -1` and spins. **After:** receives succeed.
nano-ros's `just zephyr` cyclonedds native_sim e2e exercise this path
(`test_zephyr_*_cyclonedds_pubsub_e2e`).

---

## PR 2 — NSOS: IPv4 multicast `setsockopt`/`getsockopt` (guest + host)

Open as **one PR** — the guest marshalling and the host forwarder are useless
apart.

**Target:** `drivers/net/nsos_socket.h` + `drivers/net/nsos_sockets.c` (guest)
and `drivers/net/nsos_adapt.c` (host).
**Source diffs:** `zephyr/patches/native-sim-ipproto-ip-4.4.patch` +
`zephyr/patches/nsos-adapt-ipproto-ip-4.4.patch` (scripts:
`scripts/zephyr/native-sim-ipproto-ip-patch{,-4.4}.sh` +
`scripts/zephyr/nsos-adapt-ipproto-ip-patch{,-4.4}.sh`).

**Commit message:**
```
drivers: net: nsos: support IPv4 multicast socket options

NSOS forwards socket options to the host kernel through the midplane, but
has no IPPROTO_IP arm, so guest setsockopt(IP_ADD_MEMBERSHIP) /
IP_MULTICAST_{IF,TTL,LOOP} return EOPNOTSUPP. A native_sim app can never
join an IPv4 multicast group host-side, so multicast receive is
dead-on-arrival (e.g. RTPS/SPDP discovery).

Add the NSOS_MID_IP_* option constants + struct nsos_mid_ip_mreq
(nsos_socket.h), a NET_IPPROTO_IP case in the guest
nsos_setsockopt/getsockopt that marshals the options into the midplane
(nsos_sockets.c), and the matching NSOS_MID_IPPROTO_IP forwarder in the
host nsos_adapt_setsockopt/getsockopt that unmarshals and calls the real
host setsockopt(IPPROTO_IP, …) (nsos_adapt.c). The membership arm reads the
two leading struct in_addr so one path covers both ip_mreq (8 B) and
ip_mreqn (12 B).

Signed-off-by: <your name> <your email>
```

**Repro / test:** before, a native_sim listener never joins `239.255.0.1` and
receives no multicast; after, multicast join reaches the host kernel and frames
arrive. Covered by nano-ros native_sim cyclonedds discovery e2e.

---

## Candidate NSOS PRs (3.7-line scripts, same rationale)

`nsos-getsockname`, `nsos-getifaddrs`, `nsos-mcjoin-mreq`
(`scripts/zephyr/nsos-*-patch.sh`) are the same class of generic NSOS gap
(unimplemented `getsockname`, `getifaddrs`, dual-`mreq` multicast join). They are
not yet in `patches.yml` (4.x added some natively / they were only needed on the
3.7 line). Before upstreaming, re-check each against current Zephyr `main` — some
may already be fixed upstream; drop those, file PRs for the rest with a commit
message in the same shape as PR 1/2.

## zephyr-lang-rust

`scripts/zephyr/cargo-features-patch.sh`, `rust-cargo-extra-args-patch.sh`, and
the per-arch target registration (`cortex-a9`/`cortex-r`/`aarch64-rust-patch.sh`)
patch `modules/lang/rust`. Pursue upstream in
`zephyrproject-rtos/zephyr-lang-rust` so the rust examples need no in-tree patch
— this also removes the lang-rust-shape-drift the Phase 202.5 version-tolerant
patch papers over. Upstream-paced; track here as PRs open.

## cyclonedds-on-Zephyr

The five cyclonedds patches (threads, log-flush, sockwaitset, udp-rcvbuf,
mcjoin) are baked into the nano-ros cyclonedds fork pin
(`third-party/dds/cyclonedds`), not `west patch`-delivered. Upstream them to
`eclipse-cyclonedds/cyclonedds`; once released, bump the fork pin to a tag that
includes them and drop the local commits.

---

## Post-merge cleanup (per patch, once upstream + in a tested Zephyr release)

1. Confirm the fix is in the Zephyr (or lang-rust / cyclonedds) release the
   tested pin uses (`west.yml` 3.7.0 LTS / `west-4.4.yml` 4.4.0 — see
   `docs/development/zephyr-version-support.md`).
2. Remove the `patches.yml` entry **and** the matching `scripts/zephyr/*-patch.sh`
   (both the 3.7 + 4.4 variants). Keep them as long as *any* supported line still
   needs them — drop per-line only.
3. Drop the patch invocation from `scripts/zephyr/patches/<line>.sh`.
4. Update this table + `docs/development/zephyr-version-support.md`'s patch note,
   and narrow the 205.A template's tested-version matrix.
