# TSN Assessment for Safety Island Communication

## Context

nano-ros targets vehicle ECU architectures with mixed-criticality components:
- **RT Linux nodes**: ADAS perception, planning, high-level control (non-safety or ASIL-A/B)
- **Safety islands**: Independent MCUs running safety-critical functions (ASIL-C/D) — emergency stop, command gating, watchdog monitoring

These components communicate over Ethernet. This document assesses whether IEEE 802.1 Time-Sensitive Networking (TSN) can provide the I/O safety and determinism guarantees needed for safety island communication.

## Current nano-ros Network Stack

### Supported Platforms

| Platform            | MCU            | Ethernet Hardware                 | Network Stack     |
|---------------------|----------------|-----------------------------------|-------------------|
| MPS2-AN385 (QEMU)   | Cortex-M3      | LAN9118 (SMSC911x)                | smoltcp           |
| STM32F4             | Cortex-M4F     | Built-in MAC (stm32-eth)          | smoltcp           |
| ESP32-C3            | RISC-V         | WiFi (esp-radio) / OpenETH (QEMU) | smoltcp           |
| FreeRTOS (Phase 54) | Cortex-M3+     | LAN9118 (lwIP driver)             | lwIP              |
| NuttX (Phase 55)    | Cortex-A7      | virtio-net                        | NuttX BSD sockets |
| Native/POSIX        | x86_64/aarch64 | OS-managed                        | zenoh-pico native |

### Network Capabilities

- **Protocols**: TCP, UDP unicast, TLS (mbedTLS)
- **QoS**: Reliability (best-effort/reliable), history depth, durability — all at the application/middleware layer
- **Transport**: zenoh-pico (pub/sub + RPC) or XRCE-DDS (agent-based)
- **Safety**: E2E CRC-32 + sequence tracking (Phase 35), formal verification (Kani + Verus)

### TSN Gap

**None of the current platforms have TSN-capable Ethernet hardware.** The LAN9118, STM32F4 built-in MAC, and ESP32-C3 WiFi are standard Ethernet interfaces without:

- Hardware timestamping for IEEE 802.1AS time sync
- Priority queue scheduling (802.1Qbv gate control lists)
- Credit-based shaping (802.1Qav)
- Per-stream filtering/policing (802.1Qci)
- Frame replication/elimination (802.1CB)

TSN is a **hardware-driven** technology — the Ethernet MAC/switch silicon must implement scheduling, policing, and replication at wire speed. Software cannot achieve the nanosecond-precision gate timing required. The CPU's role is configuration and management, not real-time frame scheduling.

---

## TSN Standards Relevant to Safety Islands

### IEEE 802.1AS — Time Synchronization (gPTP)

Foundation for all time-aware TSN mechanisms. Layer 2 profile of IEEE 1588 PTP.

- **Precision**: Sub-microsecond (100–500 ns typical) with hardware timestamping
- **Mechanism**: Peer-to-peer delay measurement, GrandMaster election (BMCA)
- **Requirement**: Hardware timestamp support in MAC/PHY is essential
- **Embedded consideration**: Minimal CPU overhead; Zephyr has a built-in gPTP stack

### IEEE 802.1Qbv — Time-Aware Shaper (Scheduled Traffic)

The primary mechanism for hard real-time guarantees.

- Each egress port has 8 priority queues, each with a transmission gate (open/closed)
- A **Gate Control List (GCL)** defines a repeating time schedule (typically 0.25–10 ms cycle)
- **Guard bands** prevent lower-priority frames from blocking scheduled windows
- **Exclusive windows** give safety-critical traffic zero contention
- All bridges must synchronize their GCLs via 802.1AS

**Safety value**: Worst-case latency is analytically provable via network calculus. A brake-by-wire message at priority 7 with a 50 us exclusive window every 1 ms, over 3 hops, has worst-case latency bounded at ~1.15 ms regardless of other traffic.

### IEEE 802.1Qci — Per-Stream Filtering and Policing (PSFP)

Ingress policing for fault containment.

- **Stream identification**: Map frames to streams by MAC, VLAN, or IP 5-tuple
- **Stream gates**: Time-synchronized ingress gates (frames outside expected window are dropped)
- **Flow metering**: Token bucket per stream to detect excess traffic

**Safety value**: Babbling idiot protection — a malfunctioning ECU flooding the network cannot disrupt other streams. Critical for ISO 26262 Freedom from Interference (FFI).

### IEEE 802.1CB — Frame Replication and Elimination for Reliability (FRER)

Seamless redundancy without failover delay.

- Frames are duplicated and sent over disjoint paths
- Sequence numbering detects duplicates, loss, and misordering
- Listener accepts first copy, discards duplicates
- **Zero-delay failover**: surviving path already carries live traffic

**Safety value**: Essential for ASIL-D where communication loss is unacceptable. Maps directly to ISO 26262 diagnostic coverage for communication failures.

### IEEE 802.1Qav — Credit-Based Shaper (CBS)

Statistical bounded latency for less time-critical traffic.

- Credit counter per SR queue; transmit only when credit >= 0
- **Class A**: max 2 ms over 7 hops; **Class B**: max 50 ms
- No time synchronization required (simpler than TAS)

**Safety value**: Suitable for ASIL-B sensor data (ADAS cameras, lidar) where bounded but not hard real-time latency suffices.

### IEEE 802.1DG-2025 — Automotive TSN Profile

Recently published standard defining mandatory/optional TSN features for in-vehicle Ethernet. This is the key reference for automotive TSN deployments.

---

## TSN Hardware for Safety Islands

Safety islands require Cortex-M class MCUs (deterministic, certifiable, no Linux). TSN-capable Cortex-M options:

### NXP i.MX RT1180 (Best Fit)

- **Cores**: Cortex-M7 @ 800 MHz + Cortex-M33 @ 240 MHz
- **TSN**: Integrated Gbps switch with hardware 802.1AS, 802.1Qbv, 802.1Qav, 802.1Qci, 802.1CB
- **Package**: 10x10 mm BGA
- **RTOS**: FreeRTOS, Zephyr (no Linux required)
- **SDK**: MCUXpresso with TSN configuration APIs
- **Fit**: Ideal safety island MCU — TSN hardware offload + dual-core lockstep potential

### NXP S32K3 Series

- **Cores**: Cortex-M7 (lockstep available), ASIL-D certified
- **Ethernet**: ENET with some TSN features (802.1AS, 802.1Qbv)
- **Note**: Already recommended as nano-ros safety island MCU in the AUTOSAR gap analysis
- **Fit**: Primary safety MCU; TSN support varies by variant

### TI Sitara AM64x

- **Cores**: Cortex-A53 + Cortex-R5F + Cortex-M4F + 2x PRU
- **TSN**: Hardware TSN in CPSW (Common Platform Switch), IEEE 1588v2
- **Fit**: Heterogeneous — Cortex-R5F handles safety, Cortex-A53 runs Linux; TSN is hardware-managed

### Renesas RZ/T2H

- **Cores**: Cortex-A55 + Cortex-R52
- **TSN**: Hardware EtherCAT, PROFINET, and TSN
- **Fit**: Industrial MPU with real-time safety core

### Microchip LAN9668

- **Type**: Dedicated 8-port Gbps TSN switch IC (not an MCU)
- **TSN**: Full hardware TSN (802.1AS, Qbv, Qav, Qci, CB)
- **Fit**: Companion chip — pair with any MCU to add TSN switching

### Key Insight

The **i.MX RT1180** is the most practical TSN-capable Cortex-M target for safety islands: it has an integrated TSN switch, runs bare-metal/RTOS, and fits nano-ros's `no_std` architecture. The **S32K3** (already in the roadmap) has partial TSN support. For full TSN switching, a **LAN9668** companion chip alongside an S32K3 safety MCU is another architecture option.

---

## TSN and the nano-ros Software Stack

### smoltcp: No TSN Support

smoltcp is a Layer 3/4 software TCP/IP stack. It does not implement:

- Hardware timestamp integration (802.1AS)
- Traffic class / priority queue management (802.1Qbv/Qav)
- VLAN PCP (Priority Code Point) handling
- Time-aware scheduling

This is architecturally expected: TSN features are hardware-driven at the MAC/switch level. A software TCP/IP stack needs to **interact with** TSN hardware by marking frames with appropriate priority and VLAN tags, not implement the shaping itself.

### What nano-ros Would Need

1. **TSN hardware abstraction traits** (new `nros-tsn` or `zpico-tsn` crate):
   - GCL configuration (802.1Qbv): cycle time, gate entries per queue
   - CBS parameters (802.1Qav): idleSlope, sendSlope per queue
   - Stream filter/meter configuration (802.1Qci)
   - FRER tables (802.1CB)
   - Hardware timestamp access

2. **gPTP / 802.1AS stack**:
   - The `statime` Rust crate (PTP v2.1) is a starting point
   - Would need adaptation to gPTP's Layer 2 profile
   - Zephyr has a built-in gPTP stack (usable for Zephyr platform)

3. **Frame priority marking**:
   - smoltcp would need VLAN PCP support for outgoing frames
   - Map zenoh topics to TSN traffic classes based on QoS profile

4. **Scheduling-aware transport**:
   - Applications need to know when to transmit (aligned to their TAS window)
   - `SO_TXTIME` equivalent for zenoh-pico publish calls

5. **Platform driver for TSN-capable MAC**:
   - i.MX RT1180 ENET driver with TSN register access
   - Or LAN9668 switch management driver

### Existing Rust TSN Ecosystem

| Project | Status | Relevance |
|---------|--------|-----------|
| `statime` | Active, usable | PTP v2.1 in Rust; adaptable to gPTP; has STM32 example |
| smoltcp | Active, no TSN | Would need VLAN PCP patches for frame priority marking |
| (nothing) | — | No Rust crates for 802.1Qbv, Qci, CB, or TSN switch drivers |

The TSN driver/configuration layer would need to be written from scratch for each target MCU.

---

## TSN I/O Safety Guarantees Assessment

### What TSN Provides

| Safety Property | TSN Mechanism | Guarantee Level |
|-----------------|---------------|-----------------|
| **Bounded latency** | 802.1Qbv (TAS) | Analytically provable worst-case (sub-ms achievable) |
| **Bounded jitter** | 802.1Qbv guard bands + 802.1AS | Tens of microseconds (comparable to FlexRay) |
| **Fault containment** | 802.1Qci (PSFP) | Babbling idiot protection; per-stream policing |
| **Redundancy** | 802.1CB (FRER) | Zero-delay failover; dual/triple path |
| **Loss detection** | 802.1CB sequence numbers | Per-frame loss/duplication detection |
| **Time consistency** | 802.1AS (gPTP) | Sub-microsecond across all nodes |
| **Mixed criticality** | TAS + CBS + PSFP | ASIL-D and QM traffic on same wire |

### What TSN Does NOT Provide

- **End-to-end data integrity**: TSN uses standard Ethernet CRC (frame-level). It does not provide application-level CRC, sequence tracking, or freshness validation. nano-ros's E2E safety protocol (Phase 35) is still needed on top of TSN.
- **Authentication/encryption**: TSN does not include MACsec or any authentication. A compromised node with correct timing can still inject valid frames. Application-level source authentication remains necessary.
- **Application-level watchdog**: TSN detects network-level failures but not application-level hangs. nano-ros's heartbeat/watchdog supervision is still required.

### Layered Safety Architecture

TSN and nano-ros's existing E2E safety complement each other at different layers:

```
Layer 5 — Application Safety
  │  nano-ros watchdog supervision, safety bag invariants
  │  Application-level heartbeat monitoring
  │
Layer 4 — E2E Data Safety (nano-ros Phase 35)
  │  CRC-32 integrity, sequence counter, freshness validation
  │  Source GID authentication, SafetyValidator state machine
  │
Layer 3 — Transport (zenoh-pico)
  │  Pub/sub topic routing, QoS (reliable/best-effort)
  │  TLS encryption (Phase 53)
  │
Layer 2 — Network Safety (TSN)  ← NEW
  │  802.1Qbv: bounded latency, exclusive time windows
  │  802.1Qci: fault containment, babbling idiot protection
  │  802.1CB: seamless redundancy, loss detection
  │  802.1AS: synchronized time base
  │
Layer 1 — Physical
  │  Ethernet CRC, link integrity
```

TSN provides guarantees at the network layer that cannot be achieved in software. nano-ros's E2E safety provides guarantees at the application layer that TSN cannot provide. Both are needed for a complete safety argument per ISO 26262.

---

## Comparison: TSN vs Current nano-ros Transport

| Property                   | Current (TCP/smoltcp)                    | With TSN                         |
|----------------------------|------------------------------------------|----------------------------------|
| **Worst-case latency**     | Unbounded (TCP retransmit, buffer bloat) | Bounded, provable (802.1Qbv)     |
| **Jitter**                 | ms-range (OS/RTOS scheduling dependent)  | us-range (hardware TAS)          |
| **Fault containment**      | None (one node can flood network)        | 802.1Qci policing                |
| **Redundancy**             | None (single TCP connection)             | 802.1CB dual-path                |
| **Time sync**              | None                                     | 802.1AS sub-us precision         |
| **Mixed criticality**      | Same priority for all traffic            | Per-stream priority isolation    |
| **Frame integrity**        | Ethernet CRC + TCP checksum + E2E CRC-32 | Ethernet CRC + E2E CRC-32 (same) |
| **Hardware required**      | Any Ethernet MAC                         | TSN-capable MAC/switch           |
| **Certification evidence** | Software analysis only                   | Hardware + software analysis     |

---

## Zephyr TSN Driver Reality

Zephyr defines a comprehensive TSN API surface (`ethernet.h`, `ethernet_mgmt.h`, `gptp.h`, `ptp_clock.h`), but **driver-level implementation is extremely sparse**. Most TSN capability flags and config handlers exist only in the API headers and test fakes — not in production drivers.

### TSN API vs Driver Implementation

| TSN Standard          | Zephyr API                 | Config Type                                | Drivers Implementing                         | Status     |
|-----------------------|----------------------------|--------------------------------------------|----------------------------------------------|------------|
| 802.1Qav (CBS)        | `ETHERNET_QAV`             | `ETHERNET_CONFIG_TYPE_QAV_PARAM`           | **SAM GMAC only**                            | 1 driver   |
| 802.1Qbv (TAS)        | `ETHERNET_QBV`             | `ETHERNET_CONFIG_TYPE_QBV_PARAM`           | **NXP NETC DSA only**                        | 1 driver   |
| 802.1Qbu (Preemption) | `ETHERNET_QBU`             | `ETHERNET_CONFIG_TYPE_QBU_PARAM`           | **None** (test fake only)                    | 0 drivers  |
| SO_TXTIME             | `ETHERNET_TXTIME`          | `ETHERNET_CONFIG_TYPE_TXTIME_PARAM`        | **None real** (flag-only stubs)              | 0 drivers  |
| Priority Queues       | `ETHERNET_PRIORITY_QUEUES` | `ETHERNET_CONFIG_TYPE_PRIORITY_QUEUES_NUM` | **SAM GMAC only**                            | 1 driver   |
| PTP Clock             | `ETHERNET_PTP`             | —                                          | SAM GMAC, NXP ENET, NXP NETC, STM32, XMC4xxx | 5+ drivers |
| gPTP (802.1AS)        | `gptp_event_capture()`     | —                                          | SAM E70, FRDM-K64F, Nucleo-H7xx, native_sim  | Full stack |

### Boards with Working TSN Scheduling

Only **2 real hardware drivers** implement any TSN traffic scheduling:

#### Microchip SAM E70 Xplained — QAV (Credit-Based Shaper)

- **Driver**: `drivers/ethernet/eth_sam_gmac.c`
- **MCU**: Cortex-M7 @ 300 MHz, 2 MB flash, 384 KB SRAM
- **TSN features**: QAV (idle slope, delta bandwidth, status), 6 hardware priority queues, PTP/gPTP
- **Maturity**: Production-quality. Antmicro validated on real hardware. Community-tested.
- **Boards**: SAM E70 Xplained (`sam_e70_xplained`), SAM V71 Xult (`sam_v71_xult`)
- **Price**: ~$100

The QAV implementation supports the full parameter set:
- `ETHERNET_QAV_PARAM_TYPE_STATUS` — enable/disable CBS per queue
- `ETHERNET_QAV_PARAM_TYPE_IDLE_SLOPE` — reserved bandwidth (bits/sec)
- `ETHERNET_QAV_PARAM_TYPE_DELTA_BANDWIDTH` — percentage of link bandwidth

#### NXP MIMXRT1180-EVK — QBV (Time-Aware Shaper)

- **Driver**: `drivers/ethernet/dsa/dsa_nxp_imx_netc.c` (DSA switch path)
- **MCU**: Cortex-M7 @ 792 MHz + Cortex-M33 @ 240 MHz
- **TSN features**: QBV (gate control lists, cycle time, base time), PTP/gPTP, 5-port Ethernet switch
- **Maturity**: New (merged Feb 2025, PR #82307). Uses NXP's validated SDK underneath.
- **Boards**: MIMXRT1180-EVK (`mimxrt1180_evk`), i.MX943 EVK (`imx943_evk`)
- **Price**: ~$300–900

The QBV implementation supports:
- `ETHERNET_QBV_PARAM_TYPE_STATUS` — enable/disable TAS per port
- `ETHERNET_QBV_PARAM_TYPE_TIME` — base time, cycle time, extension time
- `ETHERNET_QBV_PARAM_TYPE_GATE_CONTROL_LIST` — per-TC gate open/closed + time interval
- `ETHERNET_QBV_PARAM_TYPE_GATE_CONTROL_LIST_LEN` — GCL length

**Important**: QBV is only available via the DSA switch driver, not the main NETC endpoint driver (`eth_nxp_imx_netc.c`). The main endpoint driver exposes PTP only.

### Boards with PTP/gPTP Only (No Traffic Scheduling)

These boards have hardware timestamping (prerequisite for gPTP) but no traffic shaping:

| Board                   | MCU          | Driver            | PTP | gPTP Sample |
|-------------------------|--------------|-------------------|-----|-------------|
| NXP FRDM-K64F           | Cortex-M4    | `eth_nxp_enet.c`  | Yes | Yes         |
| STM32 Nucleo-H743ZI     | Cortex-M7    | `eth_stm32_hal.c` | Yes | Yes         |
| STM32 Nucleo-H745ZI-Q   | Cortex-M7+M4 | `eth_stm32_hal.c` | Yes | Yes         |
| STM32 Nucleo-F767ZI     | Cortex-M7    | `eth_stm32_hal.c` | Yes | Yes         |
| NXP MIMXRT1050/1060-EVK | Cortex-M7    | `eth_nxp_enet.c`  | Yes | Yes         |
| Infineon XMC4xxx        | Cortex-M4    | `eth_xmc4xxx.c`   | Yes | No          |

### Hardware with TSN Silicon but No Zephyr Driver Support

Several boards have TSN-capable Ethernet hardware where the Zephyr driver does not expose TSN features:

| Hardware | Zephyr Driver | HW TSN Capability | Driver Status |
|----------|--------------|-------------------|---------------|
| **Intel i225/i226** | `eth_intel_igc.c` | Full (QAV, QBV, QBU, LaunchTime) | **None** — basic link only |
| **NXP i.MX NETC endpoint** | `eth_nxp_imx_netc.c` | Full via NETC IP | **PTP only** |
| **NXP ENET QoS** | `eth_nxp_enet_qos_mac.c` | TSN in hardware | **None** — not even PTP |
| **Synopsys DW MAC** | `eth_dwmac.c` | QAV/QBV in variants | **None** |
| **Synopsys DWC XGMAC** | `eth_dwc_xgmac.c` | TSN in hardware | **None** |
| **STM32 H7/F7** | `eth_stm32_hal.c` | PTP + MAC queues | **PTP only** |
| **Xilinx GEM** | `eth_xlnx_gem.c` | TSN-capable variants | **None** |

The Intel i225/i226 is the most notable gap — it's the most widely-used TSN NIC in industry, supports the full TSN suite, and Zephyr's driver implements nothing beyond basic link configuration.

### Zephyr TSN Samples

| Sample | Features | Target Hardware |
|--------|----------|-----------------|
| `samples/net/ethernet/gptp/` | gPTP clock sync | SAM E70, FRDM-K64F, Nucleo-H7xx, native_sim |
| `samples/net/gptp/` | gPTP with DSA | MIMXRT1180 (5 ports), i.MX943 (5 ports) |
| `samples/net/ptp/` | IEEE 1588 PTP | Nucleo-H7xx, native_sim |
| `samples/net/sockets/txtime/` | SO_TXTIME | native_sim only (no real HW) |

### Zephyr TSN API Details

**Qav configuration** (via `net_mgmt()`):

```c
struct ethernet_req_params params;
params.qav_param.queue_id = 1;
params.qav_param.type = ETHERNET_QAV_PARAM_TYPE_IDLE_SLOPE;
params.qav_param.idle_slope = 750000; // bits per second
net_mgmt(NET_REQUEST_ETHERNET_SET_QAV_PARAM, iface, &params, sizeof(params));
```

**Qbv gate control** (via `net_mgmt()`):

```c
struct ethernet_req_params params;
params.qbv_param.port_id = 0;
params.qbv_param.type = ETHERNET_QBV_PARAM_TYPE_GATE_CONTROL_LIST;
params.qbv_param.state = ETHERNET_QBV_STATE_TYPE_ADMIN;
params.qbv_param.gate_control.gate_status[7] = true;  // TC7 open
params.qbv_param.gate_control.time_interval = 50000;   // 50 us
params.qbv_param.gate_control.row = 0;
net_mgmt(NET_REQUEST_ETHERNET_SET_QBV_PARAM, iface, &params, sizeof(params));
```

**gPTP synchronized time**:

```c
struct net_ptp_time slave_time;
bool gm_present;
gptp_event_capture(&slave_time, &gm_present);
```

**Socket priority** (application-level traffic class selection):

```c
int prio = 7;  // Highest priority (maps to TC7)
setsockopt(sock, SOL_SOCKET, SO_PRIORITY, &prio, sizeof(prio));
```

**SO_TXTIME** (scheduled transmission):

```c
bool enable = true;
setsockopt(sock, SOL_SOCKET, SO_TXTIME, &enable, sizeof(enable));
// Then use sendmsg() with SCM_TXTIME ancillary data
```

---

## FreeRTOS TSN Support

### Native TSN: None

FreeRTOS-Plus-TCP has no TSN features. A `dev/tsn` branch once existed in the FreeRTOS-Plus-TCP repository but was never merged or released. The mainline TCP/IP stack provides standard sockets without priority queues, traffic shaping, or hardware timestamp integration.

### Vendor TSN Stacks

TSN on FreeRTOS is exclusively provided by silicon vendors, tightly coupled to their hardware:

| Vendor Stack | Hardware | TSN Features | License |
|-------------|----------|-------------|---------|
| **NXP GenAVB/TSN** | i.MX RT1180, RT1176, RT1189, i.MX 8/9 | 802.1AS (gPTP), 802.1Qbv (TAS), 802.1Qav (CBS), 802.1Qci (PSFP), SRP | BSD-3-Clause |
| **TI enet-tsn-stack** | AM64x, AM243x (Sitara) | 802.1AS (gPTP), 802.1Qbv (TAS), LLDP | BSD-3-Clause |

**NXP GenAVB/TSN** is the most complete embedded TSN stack available. It runs on FreeRTOS and bare-metal, supports the full 802.1DG automotive profile subset, and is open source. The stack is validated on NXP's MCUXpresso SDK and includes ready-to-run examples for TSN bridge and endpoint configurations.

**TI enet-tsn-stack** provides gPTP and TAS for TI's Sitara processors. It integrates with TI's ENET low-level driver (ENET-LLD) and runs on FreeRTOS via the Processor SDK.

### Open-Source gPTP

**flexPTP** (MIT license) is a portable gPTP implementation for FreeRTOS + lwIP. It runs on STM32 MCUs with hardware timestamping (STM32F4/F7/H7) and provides 802.1AS time synchronization without any vendor lock-in. However, it provides only time sync — no traffic scheduling.

### Summary

FreeRTOS TSN support is vendor-driven. For safety island use with FreeRTOS, the **NXP i.MX RT1180 + GenAVB/TSN** combination is the most practical path — it provides a complete TSN stack with hardware offload, open-source code, and NXP's validation. For non-NXP hardware, only gPTP (via flexPTP) is available; no traffic scheduling exists.

---

## NuttX TSN Support

### PTP Support

NuttX has basic IEEE 1588v2 PTP support:

- **PTP daemon** (`ptpd`): Merged October 2023. Implements PTP v2 master/slave synchronization using NuttX's BSD socket API
- **PTP clock driver framework**: Merged December 2025. Provides `CLOCK_PTP` via `clock_gettime()` for hardware-timestamped PTP time

### Hardware Timestamp Support

Only **STM32F4** has a working PTP clock driver with hardware timestamps in NuttX. The STM32 Ethernet HAL provides the hardware timestamp registers needed for PTP.

### TSN Traffic Scheduling: None

NuttX has **no traffic scheduling capabilities**:

- No TX timestamp support (only RX timestamps via PTP clock)
- No `SO_PRIORITY` socket option
- No `SO_TXTIME` or `SCM_TXTIME`
- No traffic class queues or qdisc-like mechanisms
- No credit-based shaper or time-aware shaper
- No ingress policing (802.1Qci equivalent)
- No VLAN priority mapping

### Summary

NuttX provides PTP time synchronization (useful as a gPTP foundation) but nothing for TSN traffic scheduling. For safety island use, NuttX would require significant driver and stack development to support any TSN scheduling features. NuttX is not a practical choice for TSN-dependent safety island communication.

---

## VxWorks TSN Support

### Native TSN: Production-Ready

VxWorks (Wind River) has the most mature RTOS TSN implementation, with native kernel-level support:

| Feature | Status | Details |
|---------|--------|---------|
| 802.1AS (gPTP) | Native | Kernel-integrated, hardware timestamp support |
| 802.1Qbv (TAS) | Native | Gate control list configuration via APIs |
| 802.1Qbu (Preemption) | Native | Frame preemption support |
| IEEE 1588v2 (PTP) | Native | Full PTP stack |
| TSN Configuration | Via `tsntool` | CLI + API for GCL, CBS, stream config |

### Supported Hardware

VxWorks TSN is validated on Intel i210/i225 (primary reference platform), NXP i.MX8, NXP LS1028A, and TI Sitara AM65x. Intel i225 is the primary TSN NIC for VxWorks development.

### Safety Certification

VxWorks is safety-certifiable (IEC 61508 SIL 3, ISO 26262 ASIL D via VxWorks 653 / MILS architecture). Combined with native TSN, it provides the strongest safety + TSN story of any RTOS. However, VxWorks is proprietary and requires per-unit licensing, making it impractical for open-source projects.

### Relevance to nano-ros

VxWorks is not a current nano-ros platform target, but its TSN implementation serves as a reference architecture for what a production-quality RTOS TSN stack looks like. The `tsntool` CLI pattern (configure TSN from userspace via device ioctls) could inform nano-ros's own TSN configuration API design.

---

## QNX TSN Support

### Native Support: gPTP Only

QNX Neutrino provides a native gPTP/PTP implementation (`ptp4l` port and QNX-native PTP stack) with hardware timestamp support. Traffic scheduling relies on QNX's standard socket priority (`SO_PRIORITY`) and VLAN tagging — no native TAS or CBS configuration APIs exist.

### Excelfore/Avnu Partnership

QNX's TSN story is extended through partnerships:

- **Excelfore xl4tsn**: gPTP, SRP (802.1Qat), and LLDP for QNX. Open-source gPTP component (Apache 2.0)
- **Excelfore xl4combase**: Cross-platform TSN communication base library (QNX, Linux, FreeRTOS)
- **Avnu Alliance**: QNX is an Avnu member; reference implementations available

### Supported Hardware

Primary platforms: NXP i.MX8 (automotive), Intel x86 (industrial), TI Jacinto (ADAS). QNX's io-pkt network stack handles the Ethernet drivers, with vendor-specific TSN extensions.

### Relevance to nano-ros

QNX is a Cortex-A / x86 microkernel RTOS used in automotive (BlackBerry QNX). It occupies a different niche from nano-ros's Cortex-M targets. Its gPTP + Excelfore stack architecture is relevant as a reference for how middleware can layer TSN onto an RTOS without native kernel TSN support.

---

## Eclipse ThreadX / Azure RTOS TSN Support

### NetX Duo TSN API: Comprehensive

Eclipse ThreadX (formerly Azure RTOS) provides the most comprehensive TSN API surface among open-source RTOSes through NetX Duo:

| Feature | API | Status |
|---------|-----|--------|
| 802.1Qav (CBS) | `nx_shaper_cbs_*` | API defined |
| 802.1Qbv (TAS) | `nx_shaper_tas_*` | API defined |
| 802.1Qbu (FPE) | `nx_shaper_fpe_*` | API defined |
| IEEE 1588 (PTP) | `nx_ptp_client_*` | Implemented |
| Priority Queues | `nx_shaper_*` | API defined |
| Traffic Class Mapping | `nx_shaper_default_mapping_*` | API defined |

The shaper APIs provide a unified interface for CBS, TAS, and frame preemption configuration. PTP client is fully implemented and tested.

### Driver Dependency

The TSN shaper APIs are **hardware-driver-dependent** — they require Ethernet drivers that implement the shaper callbacks. The reference implementation targets NXP i.MX RT1170/1180 with the ENET QOS peripheral. Without a supporting driver, the APIs are inert.

### Open Source

ThreadX was open-sourced by Microsoft (Eclipse Foundation, MIT license). The full NetX Duo source including TSN APIs is available. This makes it potentially usable as a reference implementation for nano-ros TSN API design.

### Relevance to nano-ros

ThreadX's `nx_shaper_*` API design is the best open-source reference for an RTOS-level TSN abstraction layer. The separation of shaper configuration (CBS/TAS/FPE as orthogonal features) from driver implementation is a clean pattern that nano-ros could adopt for its `nros-tsn` API.

---

## Other RTOSes

### PikeOS (SYSGO)

Hypervisor/RTOS for aerospace and defense. TSN support through partnership with SOC-E (FPGA-based TSN IP cores). Targets DO-178C DAL A / IEC 62443 certification. Not relevant for nano-ros's embedded MCU targets.

### INTEGRITY (Green Hills)

Safety-certified RTOS (EAL 6+). TSN via Excelfore stacks (same as QNX path). Targets automotive and aerospace. Proprietary, expensive licensing.

### LynxOS-178 (Lynx Software Technologies)

DO-178C DAL A certified RTOS. Participates in IEEE P802.1DP (aerospace TSN profile). Targets avionics Ethernet (AFDX successor). Not relevant for MCU-class targets.

### SafeRTOS (WITTENSTEIN)

IEC 61508 SIL 3 pre-certified variant of FreeRTOS. **No networking stack** — SafeRTOS provides only the kernel (tasks, queues, semaphores). TSN would require integrating an external stack. Not a practical TSN platform.

### RTEMS

Open-source RTOS (primarily space/scientific). Has libbsd networking but **no TSN support** — no PTP, no traffic scheduling, no hardware timestamp integration. The community is small and focused on space applications.

### Xenomai / PREEMPT_RT Linux

Not traditional RTOSes but relevant to safety island architectures:

- **PREEMPT_RT Linux**: Full TSN support via Linux `tc-taprio`, `tc-cbs`, `tc-etf`, `ptp4l`/`phc2sys`. Most complete TSN implementation available but requires Linux (not suitable for ASIL-D safety islands)
- **Xenomai**: Co-kernel approach for hard real-time on Linux. RTnet provides real-time Ethernet but no TSN. Xenomai 4 (EVL) is moving toward mainline PREEMPT_RT

---

## Commercial TSN Middleware

Cross-RTOS TSN middleware stacks that can run on multiple platforms:

| Middleware | Vendor | RTOS Support | TSN Features | License |
|-----------|--------|-------------|-------------|---------|
| **GenAVB/TSN** | NXP | FreeRTOS, bare-metal, Linux | Full (gPTP, TAS, CBS, PSFP, SRP) | BSD-3-Clause |
| **Excelfore xl4tsn** | Excelfore | Linux, QNX, FreeRTOS | gPTP, SRP, LLDP | Apache 2.0 (gPTP) |
| **SOC-E TSN IP** | SOC-E | PikeOS, VxWorks, bare-metal | Full (FPGA cores) | Proprietary |
| **acontis TSN** | acontis | VxWorks, QNX, Linux, INTEGRITY, INtime | Full stack | Proprietary |
| **TTTech Slate** | TTTech | Linux, proprietary | Full (automotive OEM) | Proprietary |
| **Fraunhofer IPMS** | Fraunhofer | Any (FPGA IP cores) | Full (research/industrial) | Research license |
| **port GmbH** | port (NXP) | FreeRTOS, Zephyr, bare-metal | TSN stack (acquired by NXP → GenAVB) | Acquired |

**Key insight**: NXP's GenAVB/TSN (open source, BSD-3) is the only complete, free TSN stack for embedded RTOSes. All other complete stacks are proprietary or hardware-locked (FPGA IP).

---

## Cross-RTOS TSN Comparison

| RTOS | gPTP/PTP | TAS (Qbv) | CBS (Qav) | PSFP (Qci) | FPE (Qbu) | FRER (CB) | Open Source | Safety Cert |
|------|----------|-----------|-----------|------------|-----------|-----------|-------------|-------------|
| **Zephyr** | Yes (native) | 1 driver (NETC) | 1 driver (GMAC) | No | No | No | Yes | IEC 61508 (in progress) |
| **FreeRTOS** | flexPTP / vendor | Via NXP GenAVB | Via NXP GenAVB | Via NXP GenAVB | No | No | Yes | No (SafeRTOS separate) |
| **NuttX** | Yes (ptpd) | No | No | No | No | No | Yes | No |
| **VxWorks** | Yes (native) | Yes (native) | Yes (native) | No info | Yes (native) | No info | No | IEC 61508 SIL 3 |
| **QNX** | Yes (native + Excelfore) | Via middleware | Via middleware | No | No | No | No | IEC 61508 SIL 3 |
| **ThreadX** | Yes (NetX Duo) | API defined | API defined | No | API defined | No | Yes (MIT) | IEC 61508 SIL 4 |
| **INTEGRITY** | Via Excelfore | Via middleware | Via middleware | No | No | No | No | EAL 6+ |
| **PikeOS** | Via SOC-E | Via SOC-E FPGA | Via SOC-E FPGA | Via SOC-E FPGA | No | No | No | DO-178C DAL A |
| **PREEMPT_RT Linux** | Yes (linuxptp) | Yes (tc-taprio) | Yes (tc-cbs) | Yes (tc-flower) | Yes (ethtool) | Yes (kernel) | Yes | No |
| **RTEMS** | No | No | No | No | No | No | Yes | No |
| **SafeRTOS** | No networking | — | — | — | — | — | No | IEC 61508 SIL 3 |

### Key Takeaways

1. **PREEMPT_RT Linux** has the most complete TSN implementation but cannot run on ASIL-D safety islands (not certifiable for highest safety levels)
2. **VxWorks** is the only traditional RTOS with production-ready native TSN, but it's proprietary
3. **NXP GenAVB/TSN** on FreeRTOS is the best open-source option for MCU-class TSN — complete stack, hardware-validated, BSD-3 licensed
4. **Zephyr** has the API framework but only 2 drivers with actual TSN scheduling
5. **ThreadX/NetX Duo** has the best-designed TSN API (clean shaper abstraction) but needs driver implementations
6. **NuttX** and **RTEMS** are not viable TSN platforms without significant development
7. **No RTOS** provides native 802.1CB (FRER) — seamless redundancy remains a gap across all platforms

---

## nano-ros Zephyr TSN Integration Path

nano-ros already integrates with Zephyr via C FFI wrappers in `zpico-zephyr`. TSN would follow the same pattern:

```
Rust application (nano-ros node)
    │
    ▼
nros-tsn Rust API (new module)
    │  configure_qbv_schedule(), get_ptp_time(), set_stream_priority()
    │
    ▼
zpico-zephyr C FFI (new zpico_tsn.c)
    │  net_mgmt(NET_REQUEST_ETHERNET_SET_QBV_PARAM, ...)
    │  gptp_event_capture(), setsockopt(SO_PRIORITY)
    │
    ▼
Zephyr ethernet_api driver callbacks
    │
    ▼
Hardware (GMAC priority queues / NETC gate control lists / PTP clock)
```

### Steps

1. **C wrapper** (`zpico-zephyr/src/zpico_tsn.c`): Thin functions calling Zephyr `net_mgmt()`, `gptp_event_capture()`, `ptp_clock_get()`, `setsockopt(SO_PRIORITY/SO_TXTIME)`
2. **Rust FFI** (`zpico-zephyr/src/tsn.rs`): `extern "C"` declarations
3. **Safe Rust API**: Types for GCL entries, CBS parameters, PTP time, stream priority
4. **Kconfig** (`zephyr/Kconfig`): `CONFIG_NROS_TSN` enabling `CONFIG_NET_GPTP`, `CONFIG_PTP_CLOCK`, `CONFIG_NET_TC_TX_COUNT=8`

### Blocker

The software layering is straightforward. The blocker is **driver maturity**: only SAM GMAC (QAV) and NXP NETC DSA (QBV) have working implementations. No single driver provides QAV + QBV + QBU together.

---

## Recommendations

### Short Term (No Hardware Change)

nano-ros's existing E2E safety protocol (CRC-32, sequence tracking, freshness) provides application-level safety over standard Ethernet. For development and testing, this is sufficient. The safety islands can function with current hardware (STM32F4, Cortex-M3 QEMU) using standard TCP/UDP.

### Medium Term (TSN Evaluation)

1. **Start with SAM E70 Xplained for QAV prototyping** — cheapest board (~$100) with the most mature TSN driver (production-quality QAV, validated gPTP). Cortex-M7 is the same class as safety island targets. Build the nano-ros Zephyr TSN C FFI + Rust API against this hardware first.
2. **Add MIMXRT1180-EVK for QBV prototyping** — the only board with working gate control lists. More expensive (~$300–900) and newer driver (Feb 2025), but provides the hard real-time scheduling that QAV cannot.
3. **Prototype gPTP** using the `statime` crate on STM32F4 (it has hardware timestamping in the MAC) to validate the time synchronization layer for bare-metal (non-Zephyr) platforms.
4. **Add VLAN PCP support to smoltcp** — minimal change needed to tag outgoing frames with priority, enabling TSN-aware switches to schedule nano-ros traffic correctly even without full TSN on the MCU itself.

### Long Term (Production Safety)

For a production vehicle ECU with ASIL-D communication requirements:

1. **TSN-capable safety MCU** (NXP S32K3 with TSN, or i.MX RT1180)
2. **TSN switch** (integrated or LAN9668 companion) for multi-node communication
3. **Full TSN stack**: 802.1AS (time sync) + 802.1Qbv (scheduled traffic) + 802.1Qci (policing) + 802.1CB (redundancy)
4. **nano-ros E2E on top**: CRC-32 + sequence tracking provides the application-level safety argument that TSN's network-level guarantees cannot
5. **Formal verification of TSN configuration**: Prove that the GCL schedule meets all deadline requirements (network calculus)

### Not Recommended

- **Software-only TSN emulation**: TSN scheduling requires nanosecond precision that software cannot achieve. Don't try to implement 802.1Qbv in smoltcp.
- **WiFi for safety-critical**: ESP32-C3 WiFi cannot provide deterministic latency. Keep WiFi for QM/diagnostic traffic only.
- **TSN without E2E safety**: TSN alone doesn't provide data integrity above the Ethernet CRC. The existing E2E protocol remains essential.

---

## References

### IEEE Standards
- IEEE 802.1 TSN Task Group: https://1.ieee802.org/tsn/
- IEEE 802.1DG-2025 (Automotive TSN Profile)
- IEEE 802.1Qbv (Time-Aware Shaper)
- IEEE 802.1Qci (Per-Stream Filtering and Policing)
- IEEE 802.1CB (Frame Replication and Elimination)
- IEEE 802.1AS-2020 (Generalized Precision Time Protocol)

### Hardware
- NXP i.MX RT1180: https://www.nxp.com/products/processors-and-microcontrollers/arm-microcontrollers/i-mx-rt-crossover-mcus/i-mx-rt1180-crossover-mcu-dual-core-arm-cortex-m7-and-cortex-m33-with-tsn-switch:i.MX-RT1180
- NXP S32K3: https://www.nxp.com/products/processors-and-microcontrollers/s32-automotive-platform/s32k-general-purpose-mcus/s32k3-microcontrollers-for-general-purpose:S32K3
- Microchip LAN9668: https://www.microchip.com/en-us/product/lan9668

### Software
- statime (Rust PTP): https://github.com/pendulum-project/statime
- smoltcp: https://github.com/smoltcp-rs/smoltcp
- Zephyr gPTP: https://docs.zephyrproject.org/latest/connectivity/networking/api/gptp.html
- Zephyr TSN overview: https://docs.zephyrproject.org/latest/connectivity/networking/api/tsn.html
- Zephyr 802.1Qav: https://docs.zephyrproject.org/latest/connectivity/networking/api/8021Qav.html
- Zephyr SO_TXTIME sample: https://docs.zephyrproject.org/latest/samples/net/sockets/txtime/README.html
- Zephyr DSA: https://docs.zephyrproject.org/latest/connectivity/networking/dsa.html
- Antmicro Zephyr TSN: https://antmicro.com/blog/2019/09/time-sensitive-networking-in-zephyr/
- NXP NETC DSA driver (PR #82307): https://github.com/zephyrproject-rtos/zephyr/pull/82307
- Zephyr DSA rearchitecture discussion (#87091): https://github.com/zephyrproject-rtos/zephyr/discussions/87091

### Rust i.MX RT Ecosystem
- imxrt-ral (PAC): https://github.com/imxrt-rs/imxrt-ral (RT1189 support unreleased, on master)
- imxrt-hal: https://github.com/imxrt-rs/imxrt-hal (no RT1180 support yet)
- embassy-imxrt: https://github.com/OpenDevicePartnership/embassy-imxrt (RT500/600 only)

### RTOS TSN Stacks
- NXP GenAVB/TSN: https://github.com/NXP/GenAVB_TSN (BSD-3-Clause, FreeRTOS + bare-metal + Linux)
- TI enet-tsn-stack: https://github.com/TexasInstruments/enet-tsn-stack (BSD-3, AM64x/AM243x)
- flexPTP: https://github.com/nicx-next/flexPTP (MIT, gPTP on FreeRTOS + lwIP)
- Eclipse ThreadX NetX Duo: https://github.com/eclipse-threadx/netxduo (MIT, TSN shaper APIs)
- NuttX PTP daemon: https://github.com/apache/nuttx-apps/tree/master/netutils/ptpd
- Excelfore xl4tsn: https://github.com/xl4-shiro/xl4tsn (Apache 2.0 gPTP component)
- VxWorks TSN: https://www.windriver.com/products/vxworks (proprietary)
- acontis TSN: https://www.acontis.com/en/tsn.html (proprietary, multi-RTOS)

### nano-ros Safety Documentation
- `docs/research/safety-critical-platform-study.md` — cross-domain safety platforms
- `docs/research/autosar-iso26262-gap-analysis.md` — ASIL gap analysis
- `docs/design/e2e-safety-protocol-integration.md` — E2E CRC-32 design
- `docs/research/autoware-safety-island-architecture.md` — safety island architecture
- `docs/roadmap/archived/phase-35-safety-hardening.md` — E2E implementation
