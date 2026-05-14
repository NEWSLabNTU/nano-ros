# FreeRTOS LAN9118 Debugging

This page collects the low-level FreeRTOS + lwIP + QEMU LAN9118 notes
that are useful when the platform guide is not enough.

## Stack

- Board: QEMU MPS2-AN385, Cortex-M3, 25 MHz.
- Ethernet: LAN9118 MMIO at `0x4020_0000`, IRQ 13.
- Software: FreeRTOS, lwIP threaded mode (`NO_SYS=0`), zenoh-pico over
  BSD sockets.

Default task layout:

| Priority | Task | Role |
|---|---|---|
| 4 | `tcpip_thread` | lwIP protocol processing |
| 4 | poll task | drains LAN9118 RX FIFO into lwIP |
| 4 | zenoh read / lease | zenoh-pico background I/O |
| 3 | app task | nano-ros executor and user code |
| 0 | idle | must execute WFI for QEMU networking |

The poll task must run at least as high as the zenoh read task; if it
cannot drain the RX FIFO, TCP keep-alives are missed and zenoh sessions
expire.

## LAN9118 Registers

Useful direct registers:

| Offset | Name | Purpose |
|---|---|---|
| `0x00` | `RX_DATA_PORT` | RX FIFO data |
| `0x20` | `TX_DATA_PORT` | TX FIFO data |
| `0x40` | `RX_STAT_PORT` | RX packet status |
| `0x50` | `ID_REV` | chip ID / revision |
| `0x54` | `IRQ_CFG` | interrupt output configuration |
| `0x58` | `INT_STS` | interrupt status |
| `0x6C` | `RX_CFG` | RX configuration |
| `0x70` | `TX_CFG` | TX configuration |
| `0x7C` | `RX_FIFO_INF` | RX FIFO status / bytes used |
| `0x80` | `TX_FIFO_INF` | TX FIFO free space |
| `0xA4` | `MAC_CSR_CMD` | indirect MAC CSR command |
| `0xA8` | `MAC_CSR_DATA` | indirect MAC CSR data |

Indirect MAC CSR registers include `MAC_CR` (TXEN/RXEN), `ADDRH`,
`ADDRL`, `MII_ACC`, and `MII_DATA`. Always wait for the CSR busy bit
to clear before starting another access.

## QEMU Delivery Model

QEMU routes LAN9118 traffic through a legacy hub:

```text
guest LAN9118 NIC <-> QEMU hub <-> backend (Slirp / TAP / socket)
```

When the guest transmits, QEMU delivers the frame synchronously through
the hub. Slirp or TAP replies, such as ARP responses, may arrive in the
RX FIFO before the guest TX register write returns. This is useful for
diagnostics: check `RX_FIFO_INF` immediately after TX, before yielding
to FreeRTOS.

QEMU drops frames that arrive before `MAC_CR_RXEN` is enabled, and its
LAN9118 model does not flush queued packets when RXEN changes. Enable
RX before traffic starts.

## Common Pitfalls

**Diagnostic shows zero RX packets.** The poll task may have consumed
the packet before diagnostics read the register. Check `RX_FIFO_INF`
immediately after TX or temporarily prevent the poll task from running.

**Network init hangs.** lwIP threaded mode requires the FreeRTOS
scheduler to be running before `tcpip_init()`. Initialize networking
from a task, not before `vTaskStartScheduler()`.

**HardFault on first context switch.** The Cortex-M3 FreeRTOS port
expects `vPortSVCHandler`, `xPortPendSVHandler`, and
`xPortSysTickHandler` at the vector table positions. Avoid wrapper
functions except for the guarded SysTick case.

**lwIP core-lock assertion.** Keep `LWIP_TCPIP_CORE_LOCKING=0`; the
setup path calls netif functions from the app task after `tcpip_init()`.

**Intermittent stalls.** Always write `IRQ_CFG_DEFAULT` (`0x22000111`)
during LAN9118 init, even when polling.

**Corrupted RX frames.** `RX_STAT_PORT` packet length includes the
4-byte FCS. Read all aligned FIFO words, including CRC bytes, but pass
only `pkt_len - 4` bytes to lwIP.

**Second QEMU node cannot connect.** Seed `rand()` uniquely per node.
zenoh-pico uses `z_random_fill()` for session IDs; identical QEMU boots
otherwise produce duplicate IDs.

**`lwIP ASSERT: Invalid mbox`.** The application stack is too small.
The executor arena lives on the FreeRTOS task stack; use 64 KB for
service and action examples.

**Action client times out on result.** Manual-polling action servers
created with `create_action_server()` must call
`try_handle_get_result()` explicitly after completing a goal.

**No inbound frames in QEMU.** The idle hook must execute WFI so QEMU's
main event loop can service network file descriptors:

```c
void vApplicationIdleHook(void) {
    __asm__ volatile("wfi");
}
```

## Debugging Tips

- Use ARM semihosting for early `printf` output.
- Launch QEMU with `-monitor telnet:127.0.0.1:4444,server,nowait` and
  run `info network` or `info qtree`.
- Send a diagnostic ARP request and check for immediate RX FIFO data.
- Confirm `ID_REV`, `MAC_CR` TXEN/RXEN, `TX_FIFO_INF`, `RX_FIFO_INF`,
  and `INT_STS` during bring-up.

## References

- FreeRTOS-Plus-TCP MPS2_AN385 driver.
- Zephyr `drivers/ethernet/eth_smsc911x.c`.
- ARM CMSIS LAN9220 Ethernet driver templates.
- QEMU `hw/net/lan9118.c`.
- nano-ros `packages/drivers/lan9118-smoltcp/`.
