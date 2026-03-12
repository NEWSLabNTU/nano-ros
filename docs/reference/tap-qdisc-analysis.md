# TAP Qdisc Analysis for QEMU Networked Tests

## Problem

QEMU TAP devices need a queue discipline (qdisc) that handles two conflicting
requirements:

1. **No drops during QEMU emulation** — QEMU's TCG (Tiny Code Generator)
   translates guest instructions at variable speed. During heavy processing
   (zenoh-pico session establishment, service request handling), QEMU doesn't
   read from the TAP fd for tens to hundreds of milliseconds. Packets must be
   queued, not dropped.

2. **No stale packets between tests** — when QEMU is killed between E2E tests,
   unread packets persist in the qdisc. If a new QEMU reads stale packets
   with matching TCP 4-tuples, smoltcp's state machine gets confused
   (unexpected RSTs kill fresh SYN handshakes).

## Qdiscs Tested

### `fq_codel` (Linux default)

```bash
tc qdisc replace dev tap-qemu0 root fq_codel limit 1000 target 500ms interval 2s
```

**Result: consistently breaks service/action tests.**

- CoDel's delay-based Active Queue Management drops packets whose sojourn time
  exceeds `target`. Even with `target 500ms` (100x the default 5ms) or
  `target 30s`, tests fail.
- Per-flow scheduling (deficit round-robin across ~1024 flow buckets) changes
  packet delivery order compared to strict FIFO. This disrupts zenoh-pico's
  timing-sensitive service reply path — the server processes the request and
  sends a reply, but the client's `try_recv()` returns `ServiceRequestFailed`
  because the reply arrives too late.
- ECN marking (enabled by default) is irrelevant since smoltcp doesn't
  negotiate ECN, but the different code path may contribute.

Tested targets: 500ms, 30s. Both produce consistent service test failures.

### `noqueue` (standard for virtual interfaces)

```bash
tc qdisc replace dev tap-qemu0 root noqueue
```

**Result: drops too many packets, service test usually fails.**

- `noqueue` is the standard qdisc for veth pairs and container virtual
  interfaces. It eliminates kernel-side queuing entirely — packets are either
  delivered immediately or dropped.
- Eliminates stale packet accumulation (packets dropped when no reader).
- But drops packets when QEMU can't read fast enough during processing
  pauses. TCP retransmits after ~200ms (initial RTO), but zenoh-pico's
  internal get timeout expires before the retransmit arrives, causing
  `ServiceRequestFailed`.
- Service test passes ~1/3 of runs in isolation; consistently fails in batch.

### `pfifo` (chosen)

```bash
tc qdisc replace dev tap-qemu0 root pfifo limit 1000
```

**Result: best reliability for QEMU tests.**

- Simple FIFO queue, never drops packets (queue grows up to `limit`).
- QEMU's emulation pauses are absorbed by the queue — packets wait until
  QEMU reads them. No timeout-related failures.
- Stale packet accumulation is solved by ephemeral port seeding (see below),
  not by the qdisc.

## Stale Packet Mitigation

With `pfifo`, stale packets from killed QEMU processes persist in the queue
(up to 1000 packets). This is mitigated by two mechanisms:

### 1. Ephemeral port seeding via ARM semihosting

The firmware (`nros-mps2-an385`) seeds smoltcp's ephemeral port counter from
the host's wall clock using ARM semihosting `SYS_TIME` (operation 0x11):

```rust
fn semihosting_time() -> u32 {
    let result: u32;
    unsafe {
        core::arch::asm!(
            "bkpt #0xAB",
            inout("r0") 0x11_u32 => result,
            in("r1") 0_u32,
        );
    }
    result
}

// In init_network():
let host_time = semihosting_time() as u16;
let ip_byte = config.ip[3] as u16;
zpico_smoltcp::seed_ephemeral_port(host_time.wrapping_add(ip_byte.wrapping_mul(251)));
```

QEMU's DWT cycle counter is deterministic (TCG replays the same instruction
count every run), so it cannot provide entropy. The host wall clock changes
every second, giving each QEMU run a different ephemeral port.

With different ports, stale TCP packets from previous sessions don't match
any socket in smoltcp and are silently ignored.

### 2. Best-effort kernel cleanup

`cleanup_tap_network()` attempts to destroy stale TCP sockets and ARP entries
using `ss -K` and `ip neigh del`. These require `CAP_NET_ADMIN` and fail
silently without privileges. Nextest retries (configured in
`.config/nextest.toml`) handle any residual flakiness.

## Summary

| Qdisc | During test | Stale packets | Service test reliability |
|-------|-------------|---------------|------------------------|
| `fq_codel` | Per-flow reordering + CoDel drops | Auto-cleaned by CoDel | Consistently fails |
| `noqueue` | Drops when QEMU busy | None (dropped immediately) | Usually fails (~33% pass) |
| `pfifo` | No drops (queued) | Accumulate but harmless with port seeding | Best (flaky ~10-20%) |

`pfifo` is configured by `scripts/qemu/setup-network.sh`.
