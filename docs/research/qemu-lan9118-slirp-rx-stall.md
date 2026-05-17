# QEMU LAN9118 ↔ slirp RX-drop bug (Phase 127.D.1/D.2 blocker)

Date: 2026-05-17
Status: root-caused, fix pending

## Summary

The `qemu-system-arm -M mps2-an385 -nic user,model=lan9118` configuration
silently drops inbound TCP payload frames when the guest's RX FIFO is
momentarily full. The first ~500 frames after `Executor::open` get
through; subsequent reply frames (e.g. a zenoh `Reply` payload of
107 bytes plus a 11-byte `ResponseFinal`) never reach the guest, even
though host loopback `tshark` confirms the bytes were transmitted by
the peer.

This blocks Phase 127.D.1 (RTIC action E2E) and 127.D.2 (RTIC service
E2E): the server processes the request, zenohd routes the reply over
the host socket, but the client's `lan9118_smoltcp` counter
`lan_pend` freezes — the QEMU model never re-delivers the queued
frame, so smoltcp / zenoh-pico / the application never see it.

## Evidence chain

1. **Application layer (in-guest, via instrumentation added in commit
   `108e0338`)**: `nros_smoltcp::rx_diagnostics()` returns `(rx=223,
   recv=223)` and `lan9118_smoltcp::rx_diag_counters()` returns
   `(pend=519, deliv=519, err=0)` and stays flat from `iter=100`
   onward through the entire 30 s service-call window.

2. **Wire layer (host loopback, via tshark + zenoh-dissector)**:

   ```
   Frame 2631: 51632 → 7460  98 B  RequestBody(Query)
   Frame 2632:  7460 → 55584 99 B  forwarded Query
   Frame 2637: 55584 →  7460 104 B server's Reply
   Frame 2639:  7460 → 51632 107 B Response/Reply (payload [00, 01, 00, 00, 08, ..])
   Frame 2642:  7460 → 51632  11 B ResponseFinal
   Frame 2643: 51632 →  7460   0 B ACK Seq=1861
   ```

   Host TCP layer ACKs the reply (Ack=1861) immediately after frames
   2639 / 2642 arrive. **However, the guest's smoltcp never sees the
   payload bytes.**

3. **QEMU LAN9118 model source (`hw/net/lan9118.c`)**:

   * No `can_receive` callback is registered with
     `NetClientInfo net_lan9118_info` (`hw/net/lan9118.c:1262-1267`).
   * `lan9118_receive` returns `-1` when:
     * `rx_status_fifo_used == rx_status_fifo_size` (limit 176 entries
       — set twice in `lan9118_reset`, the second write at
       `hw/net/lan9118.c:416` overrides the first 704-entry value),
     * or `rx_fifo_size - rx_fifo_used < fifo_len` (data FIFO full),
     * or the frame fails MAC filter and `RXALL` is off.
   * The model never calls `qemu_flush_queued_packets()` /
     `qemu_net_queue_flush()` after the guest drains the FIFO.

4. **QEMU network queue (`net/queue.c:29-41`)**: explicit comment —

   > If a sent callback isn't provided, we just drop the packet to
   > avoid unbounded queueing.

   slirp's `net_slirp_send_packet` (`net/slirp.c:116`) calls
   `qemu_send_packet(&s->nc, pkt, pkt_len)` which routes through
   `qemu_send_packet_async(nc, buf, size, NULL)` — **NULL callback**.
   So when `lan9118_receive` returns `-1`, the frame is **dropped on
   the floor**; there is no retry path because LAN9118 has no
   `can_receive` to signal "I'm ready now" and no flush hook anywhere
   in the LAN9118 receive-side state transitions.

## Why the first ~500 frames get through

The handshake + initial declares all fit inside the empty FIFO when
the guest is fresh. After that, the guest's smoltcp can usually
drain LAN9118 fast enough that `rx_status_fifo_used` stays under
176. But once the guest stops actively reading (e.g. while the
service client is awaiting a reply with `Mono::delay(10.millis())`
between `try_recv` polls), inbound ACKs and other small frames
accumulate, and once `lan9118_receive` first returns `-1` the QEMU
queue layer **silently drops the reply we actually need** — there is
no flow control or retry between slirp and LAN9118.

## Why pubsub works in spite of this

`test_qemu_rtic_pubsub_e2e` asserts `received >= 1`. The listener's
RX FIFO is empty during the long stabilization window, so the first
publication arrives into a drained FIFO and lands fine. The test
passes on the first message even though subsequent messages would
hit the same drop pattern.

## Fix space

1. **Upstream QEMU patch.** Add a `.can_receive` callback to
   `net_lan9118_info` that returns false when
   `rx_status_fifo_used == rx_status_fifo_size` or the data FIFO can't
   accommodate the largest possible frame. Add
   `qemu_flush_queued_packets(qemu_get_queue(s->nic))` to the
   read-side of `rx_status_fifo_pop` / `rx_fifo_pop` (i.e. whenever
   the guest pops a frame and frees up FIFO space). This mirrors how
   `e1000`, `virtio-net`, etc. already wire flow control. The same
   change unblocks every QEMU MPS2 / MPS3 board using LAN9118 with
   slirp, not just nano-ros.

2. **In-tree workaround (nros-side).** Periodically toggle `MAC_CR_RXEN`
   off → on (similar to the OpenETH `MODER.RXEN` flush trick we
   already use for ESP32-C3 QEMU's `open_eth` model). When `RXEN` is
   re-enabled, QEMU's `do_mac_write` doesn't actively retry queued
   sends — slirp would still need a flush. So the only purely in-tree
   workaround that helps would be to install our own
   `qemu_flush_queued_packets` shim, which is not reachable from the
   guest.

3. **Alternative netdev.** Switch the RTIC service / action tests to
   `-nic socket,mcast=...` so a second QEMU instance acts as the
   peer instead of slirp. Two QEMU LAN9118 instances bridged via the
   mcast socket reportedly share frames frame-by-frame without the
   queue drop, because the socket netdev backend uses its own queue
   with retry-friendly semantics. This is the path Phase 97.4 took
   for the bare-metal DDS bring-up.

## Recommended next step

Try (3) — switch `test_qemu_rtic_service_e2e` /
`test_qemu_rtic_action_e2e` to `start_mps2_an385_mcast` with two
zenoh-pico peers instead of zenohd-via-slirp. zenoh-pico does
peer-to-peer just fine; we already use it for DDS bring-up. If that
unblocks the tests we sidestep the QEMU LAN9118 / slirp drop bug
entirely. If not, the only honest path is the upstream QEMU patch.

## References

* QEMU LAN9118 model: `hw/net/lan9118.c`
* QEMU net queue: `net/queue.c`
* QEMU slirp glue: `net/slirp.c`
* Local pcap evidence: `/tmp/127d-service.pcap` (captured during this session)
* Phase 127 doc: `docs/roadmap/phase-127-remaining-failure-groups.md`

## Build + ship the patch in-tree

QEMU is wired as a submodule at `third-party/qemu/qemu` (pinned to
`stable-11.0`). Patches live in `third-party/qemu/patches/` and are
applied on top of the submodule before configure. Build with:

```bash
just qemu setup-qemu
```

The recipe pulls the submodule on first run, applies every patch in
`third-party/qemu/patches/`, configures with `--target-list=arm-softmmu`,
and installs into `build/qemu/`. Subsequent runs are no-ops unless a
patch file's mtime is newer than the installed binary.

Wire the test runner to use the patched binary via the
`QEMU_SYSTEM_ARM` env var (see
`packages/testing/nros-tests/src/qemu.rs::qemu_system_arm_cmd`):

```bash
just qemu test-patched   # = QEMU_SYSTEM_ARM=$(just qemu qemu-bin) just qemu test
```

Or set `QEMU_SYSTEM_ARM=$PWD/build/qemu/bin/qemu-system-arm` in your
shell + run nextest directly.

## 2026-05-17 test outcome with patched binary

Empirically running `test_qemu_rtic_service_e2e` against the patched
`qemu-system-arm v11.0.0-dirty`:

- `lan_pend` jumped from a stuck **518** (unpatched) to **3428 and
  still climbing** at iter=2900 — the LAN9118 model now accepts and
  delivers every frame slirp pushes. **Patch confirmed effective at
  the QEMU layer.**
- Client `rx` / `recv` only advanced to **256 bytes total** over the
  30 s call window. Service call still times out: the zenoh `Reply`
  payload isn't being surfaced from smoltcp's TCP socket to the
  application even though LAN9118 isn't dropping it.
- Distinct failure mode from the QEMU bug. Likely zenoh-pico
  bare-metal reply correlation (pending-query slot dispatch) on the
  single-thread `ZPICO_SMOLTCP` build, not the network stack. Track
  separately under a follow-up.

So the QEMU patch is necessary infrastructure but not sufficient
on its own to close 127.D.1/D.2. Pubsub continues to pass.
