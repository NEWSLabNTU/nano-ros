# Zonal Vehicle Architecture and nano-ros

## Overview

The automotive industry is transitioning from **domain-based** to **zonal** electrical/electronic (E/E) architectures. This document surveys the concept, compares it to current architectures, catalogs known implementations and accessible development platforms, and analyzes how nano-ros fits into the zonal architecture landscape.

## Background: Domain vs. Zonal Architecture

### Domain Architecture (Current)

Traditional vehicles organize electronics by **functional domain** — powertrain, body, ADAS, infotainment, chassis. Each domain has its own set of ECUs with dedicated wiring and communication buses (predominantly CAN/LIN).

```
                        Vehicle
  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐
  │Powertrain│  │   Body   │  │   ADAS   │  │Infotain- │
  │ Domain   │  │ Domain   │  │ Domain   │  │  ment    │
  │ ECU  ECU │  │ ECU  ECU │  │ ECU  ECU │  │ ECU  ECU │
  │ ECU  ECU │  │ ECU  ECU │  │ ECU  ECU │  │ ECU  ECU │
  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘
     CAN bus       CAN bus       CAN bus       CAN bus
```

Characteristics:
- **100-150 ECUs** per vehicle, each with its own processor and wiring
- **~3 km of cable** harness, significant weight and assembly cost
- ECUs for a single domain are **physically scattered** across the vehicle
- Each domain has its own software stack and update mechanism
- Adding cross-domain features (e.g., ADAS using body sensors) requires complex inter-domain bridges

### Zonal Architecture (Emerging)

Zonal architecture reorganizes electronics by **physical location**. The vehicle is divided into zones (front-left, front-right, rear-left, rear-right, cabin, etc.). Each zone has a **zone controller** — a mid-power MCU/SoC that manages all local sensors and actuators regardless of functional domain, communicating with a **central vehicle computer** (HPC) over high-speed Ethernet.

```
                  Central Vehicle Computer (HPC)
                 ┌──────────────────────────┐
                 │  Linux / Adaptive AUTOSAR │
                 │  ADAS fusion, planning,   │
                 │  OTA updates, diagnostics │
                 └──┬─────┬─────┬─────┬─────┘
        Ethernet    │     │     │     │   (1-10 Gbps backbone)
        TSN         │     │     │     │
  ┌─────────┐ ┌────┴──┐ ┌┴─────┴┐ ┌──┴──────┐
  │ Front-  │ │Front- │ │ Rear- │ │  Rear-  │
  │  Left   │ │ Right │ │ Left  │ │  Right  │
  │  Zone   │ │ Zone  │ │ Zone  │ │  Zone   │
  │  Ctrl   │ │ Ctrl  │ │ Ctrl  │ │  Ctrl   │
  └──┬──┬───┘ └──┬──┬─┘ └──┬──┬─┘ └──┬──┬──┘
   CAN LIN     CAN LIN   CAN LIN   CAN LIN
   (local      (local     (local     (local
   sensors/    sensors/    sensors/   sensors/
   actuators)  actuators)  actuators) actuators)
```

Characteristics:
- **4-8 zone controllers** + 1 central HPC replace 100+ ECUs
- **~1.5 km of cable** (50% reduction; Tesla Model 3 achieved 85% harness weight reduction)
- Zone controllers handle **local I/O, power distribution, and preprocessing**
- Centralized logic on HPC, zone controllers relay data over Ethernet
- OTA updates propagated from central computer to zones
- Cross-domain features are trivial — all data flows through the central computer

### Key Differences

| Aspect                | Domain Architecture    | Zonal Architecture                                 |
|-----------------------|------------------------|----------------------------------------------------|
| Organizing principle  | By function            | By physical location                               |
| ECU count             | 100-150                | 4-8 zone controllers + 1 HPC                       |
| Wiring harness        | ~3 km, heavy           | ~1.5 km, 50-85% lighter                            |
| Backbone network      | Multiple CAN buses     | Ethernet TSN + local CAN/LIN                       |
| Software model        | Distributed, per-ECU   | Centralized logic, zone I/O                        |
| Update mechanism      | Per-ECU flashing       | OTA to HPC, propagated to zones                    |
| Cross-domain features | Complex bridges        | Native — all data at HPC                           |
| Industry timeline     | Legacy through present | 2025-2030 transition; ~40% of new vehicles by 2034 |

## Known Implementations

### Production Vehicles

| Vehicle          | Architecture                           | Details                                                                                               |
|------------------|----------------------------------------|-------------------------------------------------------------------------------------------------------|
| Tesla Model 3/Y  | 3 zones (front, left-body, right-body) | Pioneer of zonal architecture. Reduced wiring from 3 km to 1.5 km. Custom ECUs, proprietary firmware. |
| Tesla Cybertruck | Further consolidated zonal             | More aggressive integration than Model 3. Proprietary.                                                |
| Rivian R1T/R1S   | Zonal with central compute             | Custom silicon + zonal controllers. Closed platform.                                                  |

None of these are accessible for third-party development — firmware is proprietary and locked.

### Development Platforms

#### Zone Controller Dev Kits

| Platform                    | MCU         | Architecture            | Connectivity          | Availability           | Rust Support                                            |
|-----------------------------|-------------|-------------------------|-----------------------|------------------------|---------------------------------------------------------|
| **Infineon/Flex FlexZoneX** | AURIX TC4x  | TriCore                 | CAN-FD, Ethernet, LIN | Shipping late Q1 2026  | Yes (HighTec Rust compiler, ISO 26262 ASIL-D qualified) |
| **Renesas RH850/U2A**       | RH850/U2A   | Renesas proprietary     | CAN-FD, Ethernet, LIN | Starter kits available | No                                                      |
| **NXP S32Z/E EVBs**         | S32Z2/S32E2 | Cortex-R52 + Cortex-M33 | CAN-FD, Ethernet      | Available (enterprise) | No (NXP toolchain only)                                 |

The Infineon/Flex FlexZoneX kit is the most relevant for nano-ros due to AURIX Rust support. It features ~30 reusable building blocks, 50+ power distribution channels, 40 connectivity channels, and 10 load-control channels. Pre-orders open via Infineon's FlexZoneX page. Software stack includes Vector tooling for AUTOSAR integration.

#### Affordable AURIX Boards (Same MCU Family)

| Board                           | MCU                      | Key Interfaces        | Price | Available From  |
|---------------------------------|--------------------------|-----------------------|-------|-----------------|
| **AURIX TC375 Lite Kit**        | TC375 (TriCore, 3 cores) | CAN, 10/100M Ethernet | ~$99  | DigiKey, Mouser |
| **AURIX TC397 Application Kit** | TC397 (TriCore, 6 cores) | CAN-FD, Ethernet      | ~$258 | DigiKey, Mouser |

These are not zone-controller-specific but use the same AURIX MCU family and have the two key interfaces (CAN + Ethernet) present in zone controllers.

#### Software / Simulation Platforms

| Platform                                     | Description                                                                                                                                                                           | Status                                   |
|----------------------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------------------------------------|
| **AGL SoDeV**                                | Open-source SDV reference platform combining AGL Unified Code Base + Zephyr RTOS + Xen hypervisor + Linux containers. Led by Panasonic, Honda, Toyota, Mazda, Renesas. QEMU-runnable. | Announced Dec 2025, available early 2026 |
| **Renesas Zone-ECU Virtualization Platform** | Pre-configured software for RH850/U2A MCUs with demonstrator and benchmark environment for zone-ECU design exploration.                                                               | Available                                |

### Related Middleware and Standards

| Technology                       | Role                                                           | Relevance to nano-ros                                              |
|----------------------------------|----------------------------------------------------------------|--------------------------------------------------------------------|
| **AUTOSAR Classic**              | RTOS + BSW for zone controllers                                | Competing middleware; nano-ros is lighter and Rust-native          |
| **AUTOSAR Adaptive**             | POSIX-based platform for HPC                                   | Runs on the central computer; zone controllers talk to it          |
| **DDS / DDS-XRCE**               | OMG standard for pub/sub; XRCE variant for constrained devices | nano-ros rmw-xrce backend implements DDS-XRCE                      |
| **Zenoh**                        | Lightweight pub/sub protocol, ROS 2 compatible                 | nano-ros rmw-zenoh backend; zenoh router on HPC, zenoh-pico on MCU |
| **SOME/IP**                      | AUTOSAR service-oriented middleware over Ethernet              | Used in some zone architectures; nano-ros does not implement this  |
| **Ethernet TSN**                 | Time-Sensitive Networking for deterministic Ethernet           | Transport-layer concern; transparent to nano-ros                   |
| **micro-ROS**                    | ROS 2 for microcontrollers (C, XRCE-DDS only)                  | Direct competitor; nano-ros offers Rust safety + dual backends     |
| **ros2_bridge / SOME/IP bridge** | Bridges ROS 2 to AUTOSAR Adaptive                              | Enables ROS 2 nodes to interop with AUTOSAR systems                |

## nano-ros Integration Analysis

### Where nano-ros Fits

nano-ros targets the **zone controller tier** — MCU-class hardware running an RTOS with real-time constraints and severe memory budgets. This is the exact environment zone controllers operate in.

```
┌───────────────────────────────────────────────────────────┐
│                    Central HPC                             │
│  ┌─────────────┐  ┌─────────────┐  ┌──────────────────┐  │
│  │ ROS 2 nodes │  │ zenohd      │  │ XRCE-DDS Agent   │  │
│  │ (rclcpp)    │  │ router      │  │                  │  │
│  └──────┬──────┘  └──────┬──────┘  └────────┬─────────┘  │
│         └────────┬───────┘                   │            │
│              DDS/Zenoh                   DDS-XRCE         │
└──────────────┬───────────────────────────┬────────────────┘
               │     Ethernet backbone     │
┌──────────────┴───────────────────────────┴────────────────┐
│                   Zone Controller                          │
│  ┌──────────────────────────────────────────────────────┐ │
│  │  nano-ros (nros)                                      │ │
│  │  ├── rmw-zenoh (zenoh-pico client)                   │ │
│  │  │   OR rmw-xrce (Micro-XRCE-DDS client)            │ │
│  │  ├── platform: bare-metal / FreeRTOS / Zephyr        │ │
│  │  └── pub/sub, services, actions                      │ │
│  └──────────────────────────────────────────────────────┘ │
│  RTOS: FreeRTOS, Zephyr, NuttX, or bare-metal             │
│  Local bus: CAN-FD / LIN  ←→  sensors, actuators          │
│  Uplink: Ethernet  ←→  central HPC                        │
└───────────────────────────────────────────────────────────┘
```

### Strengths (Already Supported)

1. **Target environment match** — nano-ros compiles for Cortex-M/R targets, supports FreeRTOS/Zephyr/NuttX/ThreadX/bare-metal with the same API
2. **Dual RMW backends** — Zenoh (zenoh-pico) and XRCE-DDS, both with Ethernet UDP transport. Zenoh offers direct ROS 2 interop via rmw_zenoh on the HPC side.
3. **`no_std` Rust** — memory safety without Linux, critical for ASIL-rated zone controllers
4. **Formal verification** — Kani (160 harnesses) + Verus (102 proofs) provide evidence for safety arguments
5. **Small footprint** — core library fits in constrained MCU flash/RAM budgets
6. **ROS 2 protocol compatibility** — zone controllers can publish/subscribe to the same topics as Linux-based ROS 2 nodes on the HPC

### Gaps

| Gap                         | Description                                                                                                                                                                                       | Severity              | Mitigation                                                                                                                                                                                                             |
|-----------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-----------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **CAN-FD transport**        | Zone controllers aggregate local CAN/LIN sensors. nano-ros has only Ethernet/UDP transport. A CAN-FD transport (likely for the XRCE backend) would be needed for intra-zone sensor communication. | Medium                | Ethernet handles zone-to-HPC communication (the primary use case). CAN sensor data can be bridged in application code. A CAN transport would be a future phase.                                                        |
| **TriCore toolchain**       | AURIX TC3xx/TC4x (the dominant zone controller MCU family) uses Infineon's TriCore architecture, not ARM. Requires the HighTec Rust compiler instead of upstream `rustc`.                         | Medium                | The core `nros` crate is architecture-agnostic (`no_std` + `core`). C backends (zenoh-pico, XRCE-DDS) build with any C compiler. ARM-based alternatives (STM32H7, NXP S32K3) work with standard Rust toolchains today. |
| **SOME/IP interop**         | Some OEM zone architectures use SOME/IP rather than DDS for service communication. nano-ros does not implement SOME/IP.                                                                           | Low                   | DDS-XRCE and Zenoh are the ROS 2 ecosystem standards. SOME/IP bridging can be done at the HPC level (ros2_bridge).                                                                                                     |
| **ISO 26262 certification** | Production automotive deployment requires ISO 26262 qualification of the Rust toolchain and library.                                                                                              | High (for production) | HighTec's ASIL-D qualified Rust compiler for AURIX is a start. Library certification is a long-term effort independent of architecture support.                                                                        |
| **TSN awareness**           | Ethernet TSN provides deterministic scheduling. nano-ros doesn't participate in TSN configuration.                                                                                                | Low                   | TSN operates at the transport layer, transparent to application middleware. The OS/driver stack handles TSN.                                                                                                           |

### TSN Support Landscape

TSN (Time-Sensitive Networking) is critical for zonal architectures — it guarantees
deterministic Ethernet delivery between zone controllers and the central HPC. The
TSN standard suite includes IEEE 802.1AS (gPTP clock sync), 802.1Qbv (Time-Aware
Shaper / TAS — gate-based scheduled traffic), 802.1Qav (Credit-Based Shaper / CBS),
and 802.1Qbu/802.3br (Frame Preemption / FPE).

#### RTOS-Integrated TSN Support

| RTOS                   | gPTP (802.1AS)       | TAS (802.1Qbv)        | CBS (802.1Qav) | FPE (802.1Qbu) | Status                                                                       |
|------------------------|----------------------|------------------------|----------------|-----------------|------------------------------------------------------------------------------|
| **ThreadX / NetX Duo** | Yes                  | **Yes**                | **Yes**         | **Yes**          | Full TSN stack: CBS, TAS (EST), FPE, PTP. IEC 61508 SIL 4 certified.         |
| **Zephyr**             | Yes (Intel/Antmicro) | Partial (2 drivers)    | Yes             | No               | gPTP and CBS merged in mainline. 802.1Qbv driver support hardware-specific.  |
| **FreeRTOS**           | Via NXP GenAVB       | **Via NXP GenAVB**     | Via NXP GenAVB  | **Via NXP GenAVB** | NXP GenAVB/TSN provides full profile on NXP silicon.                        |
| **NuttX**              | PTP only             | No                     | No              | No               | No TSN-specific support.                                                     |
| **VxWorks 7**          | Yes                  | Yes                    | Yes             | Yes              | Commercial. DO-178C, IEC 62443 certified.                                    |
| **QNX**                | Yes                  | Via Excelfore          | Via Excelfore   | Via Excelfore    | Commercial. ISO 26262 ASIL D. Excelfore eAVB/TSN stack ported.              |
| **INTEGRITY**          | Yes                  | Via Excelfore          | Via Excelfore   | Via Excelfore    | Commercial. DO-178C DAL A, ISO 26262 ASIL D.                                |

#### Open-Source TSN Stacks for RTOS

Several open-source TSN stacks exist beyond RTOS-integrated support:

| Stack                      | License      | Standards                               | RTOS Support                  | Hardware Dependency        |
|----------------------------|--------------|-----------------------------------------|-------------------------------|----------------------------|
| **NXP GenAVB/TSN**         | BSD 3-Clause | 802.1AS, Qav, **Qbv**, **Qbu**, 802.3br | FreeRTOS, **Zephyr**, Linux   | NXP silicon (i.MX, RT1180) |
| **TI enet-tsn-stack**      | Open source  | 802.1AS-2020, Qav, **Qbv**, **Qbu**     | FreeRTOS                      | TI Sitara (AM243x, AM64x)  |
| **Excelfore xl4-gptp**     | GPLv2        | 802.1AS only (gPTP)                     | Linux (ports to others exist) | Hardware-agnostic          |
| **Avnu OpenAvnu / gptp**   | Open source  | 802.1AS, AVB                            | Linux (portable design)       | Hardware-agnostic          |
| **ptp4FreeRTOS**           | Open source  | 802.1AS (linuxptp port)                 | FreeRTOS                      | Xilinx ZCU102 (Cortex-R5)  |
| **open62541 (OPC UA TSN)** | MPLv2        | OPC UA PubSub over TSN                  | Linux (TSN transport layer)   | Linux tc-taprio dependent  |

**NXP GenAVB/TSN** is the most complete open-source RTOS TSN stack. It covers nearly
the full TSN profile (gPTP, TAS, CBS, FPE) and runs on FreeRTOS, Zephyr, and Linux
via an RTOS abstraction layer. It is BSD-licensed and available on GitHub (`NXP/GenAVB_TSN`).
The key limitation is NXP hardware dependency — it requires NXP silicon with TSN-capable
Ethernet MACs (i.MX 8M, i.MX 93, i.MX RT1180).

**TI enet-tsn-stack** offers similar coverage for TI Sitara processors on FreeRTOS,
developed as part of the OPC UA over TSN collaboration (ADI, Arm, AWS, TI, etc.).

#### TSN Hardware (SoC-Integrated)

Most embedded TSN deployments use hardware offload in the Ethernet MAC/switch, with
software handling only the control plane (gPTP negotiation, gate list configuration):

| SoC Family        | Vendor  | TSN Standards                 | Software Stack               | RTOS Support                |
|-------------------|---------|-------------------------------|------------------------------|-----------------------------|
| i.MX RT1180       | NXP     | 802.1AS, Qav, Qbv, Qbu        | GenAVB/TSN (open source)     | FreeRTOS, **Zephyr**, Linux |
| i.MX 8M / i.MX 93 | NXP     | 802.1AS, Qav, Qbv, Qbu        | GenAVB/TSN (open source)     | FreeRTOS, Zephyr, Linux     |
| S32G2/G3          | NXP     | 802.1AS-Rev, Qbv, Qbu         | GenAVB/TSN + Excelfore       | Linux, INTEGRITY, QNX       |
| AM243x / AM64x    | TI      | 802.1AS-2020, Qav, Qbv, Qbu   | enet-tsn-stack (open source) | FreeRTOS                    |
| STM32MP25         | ST      | 802.1AS, Qbv, Qbu (TTTech IP) | Linux kernel drivers         | Linux                       |
| RZ/N2L            | Renesas | 802.1AS-2020, Qbv, Qbu        | Renesas FSP                  | RTOS/bare-metal             |
| fido5100/fido5200 | ADI     | TSN-ready, multi-protocol     | ADI drivers                  | Bare-metal, any RTOS        |

#### GenAVB/TSN Architecture

GenAVB/TSN **bypasses the RTOS networking stack entirely**. It has its own Ethernet MAC
drivers (`net_port_netc_sw.c`, `net_port_enet_qos.c`) that talk directly to NXP
hardware for deterministic access to timestamping, credit-based shapers, and scheduled
traffic gates. It uses the RTOS only for OS primitives (mutexes, tasks, timers) via an
abstraction layer.

This means nano-ros does **not** link to GenAVB/TSN directly. The interaction is
indirect:
1. GenAVB/TSN configures TSN at the hardware/driver level (gPTP sync, gate schedules)
2. nano-ros sends/receives ROS 2 messages over standard UDP/IP sockets (via zenoh-pico)
3. The TSN-configured network ensures nano-ros traffic gets deterministic latency

On Zephyr Cortex-A targets (Harpoon), GenAVB/TSN runs in a Jailhouse hypervisor
inmate cell, further isolating it from normal Zephyr networking.

#### GenAVB/TSN Platform Support

GenAVB/TSN's Zephyr and FreeRTOS support targets **different SoC classes**:

| RTOS     | Supported SoCs                                  | Core Class | Config Files                                      |
|----------|------------------------------------------------|------------|---------------------------------------------------|
| Zephyr   | i.MX 8M Mini/Nano/Plus, i.MX 93, i.MX 95       | Cortex-A53/A55 (MPU) | `config_zephyr_imx8m*_ca53.cmake`, `config_zephyr_imx9*_ca55.cmake` |
| FreeRTOS | **i.MX RT1180 (MIMXRT1189)**, RT1052, RT1176    | **Cortex-M7/M33 (MCU)** | `config_freertos_rt1189_cm7.cmake`, `_cm33.cmake` |

**GenAVB/TSN on Zephyr does NOT support the i.MX RT1180.** The RT1180 is supported
only under FreeRTOS. An [NXP community post](https://community.nxp.com/t5/Zephyr-Project/Build-Gen-AVB-TSN-stack-for-MIMXRT1180-EVK-running-Zephyr/td-p/2133940)
confirms there are no guidelines or timeline to add Zephyr support for the RT1180.

This constraint shapes the development paths below.

#### Assessment

ThreadX + NetX Duo remains the only RTOS with a **vendor-integrated, certified** TSN
stack in its mainline networking library. NXP GenAVB/TSN provides equivalent TSN
profile coverage on FreeRTOS and Zephyr, but with platform constraints:
- On MCU-class hardware (RT1180, Cortex-M7): **FreeRTOS only**
- On MPU-class hardware (i.MX 8M/93, Cortex-A): **Zephyr or FreeRTOS**

For nano-ros on zone controllers (MCU-class), the practical TSN options are:
1. **FreeRTOS + GenAVB/TSN** on i.MX RT1180 (full TSN, open source, NXP-supported)
2. **ThreadX + NetX Duo TSN** on FRDM-MCXE31B (full TSN, SIL 4 certified)
3. **Zephyr + native gPTP** on any supported board (gPTP + CBS only, no TAS/FPE)

### Prior Art: NXP DDS-TSN Demo

NXP published an open-source [DDS-TSN integration demo](https://github.com/NXP/dds-tsn)
demonstrating ROS 2 over TSN. It uses:
- 3 Linux machines (Ubuntu 20.04) connected via an NXP SJA1110 TSN switch
- NXP i.MX 8M NavQ embedded boards
- ROS 2 Foxy with Fast DDS or RTI Connext DDS
- Gazebo simulation of a moose-test (time-critical evasive maneuver)

This demo proves the concept but runs on **Linux, not an RTOS**. It cannot meet the
hard real-time guarantees of a zone controller. A nano-ros + RTOS implementation
would bring this to the MCU tier with deterministic scheduling.

### Taiwanese SDV Platforms

| Platform                      | Organization       | Architecture                     | Accessibility                                                    |
|-------------------------------|--------------------|----------------------------------|------------------------------------------------------------------|
| **Foxconn MIH Open EV**       | Foxconn / MIH      | Zonal EEA, open platform         | EVKit SDK available for order through MIH consortium             |
| **FIH HPC Platform**          | FIH (Foxconn sub.) | ZCU-based zonal, HPC integration | OEM-targeted, CES 2026 showcase, not available as standalone kit |
| **Realtek RTL9072 / RTL9075** | Realtek            | Automotive Ethernet switch ICs   | Linux kernel driver merged upstream, no standalone eval boards   |
| **ITRI / TADA**               | ITRI               | Research and industry bridging   | Collaboration-oriented, not direct dev kit sales                 |

The Taiwanese SDV ecosystem is **OEM-oriented** — focused on supplying platforms to
car makers rather than selling accessible development kits. The most accessible option
is **Foxconn MIH EVKit**, but it targets vehicle-level integration rather than zone
controller firmware development. No TSN evaluation boards from Taiwanese vendors were
found.

**Realtek RTL9072/RTL9075** (15-port, up to 5GbE, PCIe SR-IOV) could serve as the
in-vehicle Ethernet backbone connecting zone controllers, but would need to be sourced
through a board integrator.

### Recommended Development Path: ROS 2 Zonal System with TSN

Three paths exist for building a nano-ros zonal system with TSN, each with different
trade-offs between TSN coverage, platform maturity, and hardware cost.

#### Hardware Options

| Board                    | MCU                                   | TSN Support                  | CAN-FD    | Price | GenAVB/TSN    | nano-ros Platform |
|--------------------------|---------------------------------------|------------------------------|-----------|-------|---------------|-------------------|
| **NXP MIMXRT1180-EVK**   | i.MX RT1180 (Cortex-M7 800 MHz + M33) | **GbE TSN switch** (5 ports) | CAN-FD    | ~$900 | FreeRTOS only | platform-freertos |
| **NXP FRDM-MCXE31B**     | MCX E31 (Cortex-M7, 120 MHz)          | 10/100M Ethernet + TSN       | 3x CAN-FD | ~$47  | Not supported | platform-threadx  |
| **TI AM243x LaunchPad**  | AM2434 (Cortex-R5F, quad-core)        | GbE + TSN (PRU-ICSSG)        | No        | ~$79  | Not supported | (none yet)        |
| **AURIX TC375 Lite Kit** | TC375 (TriCore, 3 cores)              | 10/100M Ethernet             | CAN       | ~$99  | Not supported | (none yet)        |

#### Path A: FreeRTOS + GenAVB/TSN on i.MX RT1180 (Full TSN)

Uses NXP's open-source GenAVB/TSN stack on FreeRTOS — the only supported RTOS for
GenAVB/TSN on MCU-class hardware (i.MX RT1180).

```
Phase A1: nano-ros on FreeRTOS + i.MX RT1180 EVK  ← board bring-up
    │    nano-ros platform-freertos exists (Phase 54)
    │    Need RT1180 board crate + FreeRTOS BSP
    │
    ▼
Phase A2: GenAVB/TSN integration                   ← TSN enablement
    │    GenAVB/TSN runs on FreeRTOS on RT1180
    │    (config_freertos_rt1189_cm7.cmake)
    │    Configure gPTP sync, TAS gate schedules
    │    GenAVB/TSN manages TSN at driver level,
    │    nano-ros uses standard sockets via zenoh-pico
    │
    ▼
Phase A3: Zone controller demo                     ← end-to-end
         nano-ros → zenoh-pico → FreeRTOS → GenAVB/TSN → Linux HPC
```

**Advantages**:
- Full TSN profile (gPTP + TAS + CBS + FPE) — open source, BSD-licensed
- `platform-freertos` exists (Phase 54, 54.1–54.11 done)
- NXP provides integrated SDK (MCUXpresso + GenAVB/TSN + FreeRTOS)
- Most capable TSN hardware: 5-port GbE switch on-chip

**Trade-offs**:
- i.MX RT1180 EVK is expensive (~$900)
- GenAVB/TSN is NXP-hardware-specific
- GenAVB/TSN bypasses the RTOS networking stack (has own Ethernet MAC drivers)
- No simulation mode — requires physical hardware for any GenAVB/TSN testing
- Cross-compilation without hardware is possible (standard ARM GCC + MCUXpresso SDK)

#### Path B: ThreadX + NetX Duo TSN (Certified TSN)

Uses the ThreadX platform backend (Phase 58, in progress) with NetX Duo's built-in
certified TSN stack.

```
Phase B1: Complete ThreadX platform backend        ← current (Phase 58, 58.1-58.7 done)
    │    Finish QEMU RISC-V target, E2E tests
    │
    ▼
Phase B2: NetX Duo TSN + zenoh-pico integration    ← TSN enablement
    │    Wire zenoh-pico transport through NetX Duo BSD sockets
    │    with TSN QoS (TAS gate scheduling for ROS 2 traffic)
    │
    ▼
Phase B3: FRDM-MCXE31B board bring-up              ← hardware target
    │    ThreadX + NetX Duo on MCX E31
    │    nano-ros rmw-zenoh over TSN Ethernet
    │    zenohd on Linux PC as router/bridge to ROS 2
    │
    ▼
Phase B4: Zone controller demo                      ← end-to-end
         CAN-FD sensor input → nano-ros → TSN Ethernet → ROS 2 HPC
```

**Advantages**:
- NetX Duo TSN is IEC 61508 SIL 4 certified
- FRDM-MCXE31B is affordable (~$47) with TSN + CAN-FD
- TSN stack is integrated into the RTOS networking library (uses standard sockets)
- TSN configuration is orthogonal to nano-ros — standard socket API throughout

**Trade-offs**:
- ThreadX platform backend is incomplete (Phase 58 in progress)
- More work before reaching TSN integration phase
- Requires physical hardware for TSN testing (ThreadX Linux sim does not emulate TSN MACs)

#### Path C: Zephyr + Native gPTP (Partial TSN, No Hardware Needed)

Uses nano-ros's mature Zephyr platform backend with Zephyr's built-in gPTP. Provides
partial TSN (time synchronization + CBS traffic shaping) without requiring hardware
or external TSN stacks.

```
Phase C1: nano-ros on Zephyr native_sim + gPTP     ← software-only evaluation
    │    Zephyr gPTP works on native_sim
    │    Pair with linuxptp ptp4l on host
    │    Validate nano-ros pub/sub with time-synchronized network
    │
    ▼
Phase C2: nano-ros on Zephyr + real hardware        ← board bring-up
    │    Any Zephyr board with HW-timestamped Ethernet
    │    (i.MX RT1180 EVK, FRDM-K64F, SAM-E70, etc.)
    │    gPTP + CBS with hardware timestamping
    │
    ▼
Phase C3: Upgrade to full TSN (optional)            ← if needed
         Add GenAVB/TSN (Path A) or switch to ThreadX (Path B)
         for TAS gate scheduling and frame preemption
```

**Advantages**:
- `platform-zephyr` is mature and tested (7+ example apps) — no new backend work
- **Evaluable without hardware** on native_sim (gPTP + linuxptp over TAP interface)
- Can start development immediately with zero hardware cost
- Zephyr's gPTP is mainline, maintained, and board-agnostic

**Trade-offs**:
- Only gPTP (802.1AS) and CBS (802.1Qav) — no TAS (802.1Qbv) or FPE (802.1Qbu)
- TAS gate scheduling requires hardware offload (not available in Zephyr's native stack)
- Software timestamping on native_sim has microsecond jitter (not nanosecond)

#### Evaluation Without Hardware

All three TSN stacks have different simulation capabilities:

| Component                      | native_sim | QEMU x86 | Linux x86 | Hardware Required |
|--------------------------------|------------|----------|-----------|-------------------|
| nano-ros + rmw-zenoh on Zephyr | Yes        | Yes      | N/A       | No                |
| Zephyr built-in gPTP (802.1AS) | Yes        | Yes      | N/A       | No                |
| NXP GenAVB/TSN (full profile)  | No         | No       | No        | Yes (NXP silicon) |
| TI enet-tsn-stack              | No         | No       | No        | Yes (TI Sitara)   |
| NetX Duo TSN                   | No         | No       | No        | Yes               |
| linuxptp (ptp4l, host-side)    | N/A        | N/A      | Yes       | No                |
| Cross-compile GenAVB/TSN       | N/A        | N/A      | **Yes**   | No (build only)   |

**What works without hardware — Zephyr gPTP on native_sim + linuxptp (Path C):**

Zephyr's built-in gPTP (IEEE 802.1AS) compiles and runs as a native Linux executable
on `native_sim`, using the TAP/TUN networking interface (`zeth`). It pairs with
linuxptp's `ptp4l` daemon on the Linux host for time synchronization testing:

```bash
# Zephyr side (runs as Linux process, uses TAP interface)
west build -b native_sim samples/net/gptp
./build/zephyr/zephyr.exe

# Linux host side (software timestamping — no hardware clock needed)
sudo ptp4l -2 -f gPTP-zephyr.cfg -i zeth -m -q -l 6 -S
```

This enables testing nano-ros pub/sub over a gPTP-synchronized virtual network link
without any evaluation board. The `-S` flag enables software timestamping.

**Cross-compilation without hardware (Paths A and B):**

GenAVB/TSN and NetX Duo can both be cross-compiled without a physical board — they use
standard ARM GCC toolchains and vendor SDKs. You can build the full nano-ros + TSN
firmware image on a Linux x86 development machine; you just cannot run or test it
without flashing to hardware.

```bash
# GenAVB/TSN cross-compilation (Path A) — no board needed
cmake . -Bbuild -DTARGET=freertos_rt1189_cm7 -DCONFIG=endpoint_tsn
cmake --build build

# nano-ros cross-compilation for FreeRTOS Cortex-M7 — no board needed
cargo build --target thumbv7em-none-eabihf --features "rmw-zenoh,platform-freertos"
```

**What each path can evaluate without hardware:**

| Phase                       | Path A (FreeRTOS)       | Path B (ThreadX)        | Path C (Zephyr)              |
|-----------------------------|-------------------------|-------------------------|------------------------------|
| nano-ros + RTOS integration | Partial (no RT1180 sim) | Yes (ThreadX Linux sim) | **Yes (native_sim)**         |
| gPTP time synchronization   | No                      | No                      | **Yes (native_sim + ptp4l)** |
| Full TSN (TAS, CBS, FPE)    | No — needs RT1180 EVK   | No — needs FRDM-MCXE31B | N/A (CBS only)               |
| Cross-compile full stack    | **Yes**                 | **Yes**                 | **Yes**                      |
| Zone controller demo        | No — needs hardware     | No — needs hardware     | No — needs hardware          |

**Recommended starting point**: Path C allows the most progress without hardware
investment. Start with Zephyr native_sim to validate nano-ros + gPTP integration,
then graduate to Path A or B when hardware is available for full TSN evaluation.

#### TSN Integration Details (Common to All Paths)

nano-ros does not interact with TSN directly. It publishes via zenoh-pico, which
uses standard BSD sockets. The TSN stack (GenAVB/TSN, NetX Duo, or Zephyr gPTP)
operates at the driver/OS level, configuring network hardware for deterministic
delivery. The application or board crate configures TSN parameters during
initialization:

1. **gPTP clock sync** — Synchronize the zone controller's clock with the TSN
   network. All three paths provide gPTP support.
2. **Time-Aware Shaper (TAS)** — Configure gate schedules assigning nano-ros/zenoh
   traffic to a dedicated TSN traffic class with a guaranteed time slot per cycle.
   (Paths A and B only — requires hardware offload.)
3. **Credit-Based Shaper (CBS)** — Traffic shaping for bandwidth reservation.
   Available in all three paths (Zephyr has native CBS support).
4. **Frame Preemption (FPE)** — Allows high-priority TSN frames to preempt
   lower-priority traffic mid-transmission. (Paths A and B only.)

#### Target Demo Setup

```
┌─────────────────────────────┐     TSN Ethernet      ┌──────────────────────┐
│     Zone Controller Board   │  (802.1Qbv scheduled) │    Linux PC (HPC)    │
│  ┌───────────────────────┐  │◄──────────────────────►│  ┌────────────────┐  │
│  │ nano-ros (nros)       │  │     Ethernet           │  │ zenohd router  │  │
│  │  rmw-zenoh            │  │                        │  │ ROS 2 nodes    │  │
│  │  platform-freertos,   │  │                        │  │ (rclcpp)       │  │
│  │  -threadx, or -zephyr │  │                        │  └────────────────┘  │
│  └───────────┬───────────┘  │                        │                      │
│  ┌───────────┴───────────┐  │                        │                      │
│  │ zenoh-pico (client)   │  │                        │                      │
│  └───────────┬───────────┘  │                        │                      │
│  ┌───────────┴───────────┐  │                        │                      │
│  │ TSN stack             │  │                        │                      │
│  │ GenAVB / NetX Duo /   │  │                        │                      │
│  │ Zephyr gPTP+CBS       │  │                        │                      │
│  └───────────┬───────────┘  │                        │                      │
│  ┌───────────┴───────────┐  │                        │                      │
│  │ RTOS kernel           │  │                        │                      │
│  └───────────────────────┘  │                        │                      │
│              │               │                        │                      │
│         CAN-FD bus           │                        │                      │
│     (local sensors)          │                        │                      │
└─────────────────────────────┘                        └──────────────────────┘
```

The demo validates the zonal architecture pattern: local I/O over CAN-FD,
deterministic backbone over TSN Ethernet, centralized processing on the HPC.

## References

### Zonal Architecture
- Zonal Architecture Overview (Promwad) — https://promwad.com/news/zonal-architecture-vehicle-electronics
- Zonal vs Domain Architecture (EE Times) — https://www.eetimes.com/automotive-architectures-domain-zonal-and-the-rise-of-central/
- Zone Architecture and Ethernet (EE Times) — https://www.eetimes.com/zone-architecture-ethernet-drive-vehicle-of-the-future/
- Zonal Architecture 101 (onsemi) — https://www.onsemi.com/company/news-media/blog/automotive/en-us/zonal-architecture-101-reducing-vehicle-system-development-complexity
- Microchip RCP and Zonal Architecture — https://www.microchip.com/en-us/about/media-center/blog/2026/rcp-and-concurrent-paradigm-shift-to-zonal-architecture
- Tesla Zonal Architecture — https://www.laitimes.com/en/article/3n8cl_43xcy.html

### Hardware Platforms
- Infineon/Flex FlexZoneX Zone Controller Dev Kit — https://www.infineon.com/market-news/2025/infatv202601-038
- Infineon Rust Support for AURIX (HighTec compiler) — https://www.infineon.com/design-resources/development-tools/sdk/rust
- Infineon AURIX TC4Dx for Zone Controllers — https://www.infineon.com/products/microcontroller/32-bit-tricore/aurix-tc4x/tc4dx
- AURIX TC375 Lite Kit — https://www.infineon.com/evaluation-board/KIT-A2G-TC375-LITE
- Renesas RH850/U2A Zone/Domain MCU — https://www.renesas.com/en/products/rh850-u2a
- NXP S32Z/E Development Boards — https://www.nxp.com/design/design-center/development-boards-and-designs
- NXP FRDM-MCXE31B (MCX E31, TSN + CAN-FD) — https://www.digikey.com/en/products/detail/nxp-usa-inc/FRDM-MCXE31B/27569139
- NXP i.MX RT1180 EVK (GbE TSN switch) — https://www.nxp.com/part/MIMXRT1180-EVK
- NXP i.MX RT1180 Zephyr board support — https://docs.zephyrproject.org/latest/boards/nxp/mimxrt1180_evk/doc/index.html

### TSN and Networking
- Eclipse ThreadX NetX Duo (TSN APIs) — https://github.com/eclipse-threadx/netxduo
- NetX Duo TSN Documentation — https://github.com/eclipse-threadx/rtos-docs/blob/main/rtos-docs/netx-duo/chapter5.md
- Zephyr gPTP Documentation — https://docs.zephyrproject.org/latest/connectivity/networking/api/gptp.html
- Zephyr TSN Support (Antmicro) — https://zephyrproject.org/antmicros-work-with-time-sensitive-networking-support-in-the-zephyr-rtos/
- NXP GenAVB/TSN Stack (BSD-3, FreeRTOS/Zephyr/Linux) — https://github.com/NXP/GenAVB_TSN
- NXP GenAVB/TSN RTOS Apps (demo applications) — https://github.com/NXP/GenAVB_TSN-rtos-apps
- NXP RTOS Abstraction Layer — https://github.com/NXP/rtos-abstraction-layer
- NXP Harpoon Apps (Zephyr + GenAVB/TSN on Cortex-A) — https://github.com/NXP/harpoon-apps
- TI enet-tsn-stack (FreeRTOS) — https://github.com/TexasInstruments/enet-tsn-stack
- Excelfore xl4-gptp (GPLv2, gPTP only) — https://github.com/xl4-shiro/excelfore-gptp
- Avnu OpenAvnu / gptp — https://github.com/Avnu/gptp
- ptp4FreeRTOS (linuxptp port) — https://github.com/syedsk/ptp4FreeRTOS
- OpenTSN (FPGA-based) — https://github.com/hakiri/openTSN
- OPC UA PubSub over TSN (OSADL / open62541) — https://www.osadl.org/OPC-UA-PubSub-over-TSN.opcua-tsn.0.html
- OPC UA over TSN on FreeRTOS (ADI, Arm, AWS, TI collaboration) — https://blog.freertos.org/Community/Blogs/2023/opc-ua-tsn-and-freertos
- NXP DDS-TSN Integration Demo — https://github.com/NXP/dds-tsn
- NXP ROS2-DDS-TSN Demo Discussion — https://discourse.openrobotics.org/t/ros2-dds-tsn-integration-demo-by-nxp/22776
- Wind River VxWorks TSN — https://www.windriver.com/tsn-solutions
- TTTech Slate TSN IP — https://www.tttech-industrial.com/products/slate
- Fraunhofer IPMS Ethernet/TSN IP Cores — https://www.ipms.fraunhofer.de/en/Components-and-Systems/Components-and-Systems-Data-Communication/ip-cores/IP-Cores-Ethernet.html
- AMD/Xilinx TSN Solution — https://xilinx-wiki.atlassian.net/wiki/spaces/A/pages/25034864/Xilinx+TSN+Solution
- linuxptp (Linux kernel PTP/gPTP) — https://linuxptp.sourceforge.net/
- TSN Documentation Project — https://tsn.readthedocs.io/timesync.html
- Zephyr gPTP Sample (native_sim + linuxptp) — https://docs.zephyrproject.org/latest/samples/net/gptp/README.html
- NXP Community: GenAVB/TSN on i.MX RT1180 Zephyr — https://community.nxp.com/t5/Zephyr-Project/Build-Gen-AVB-TSN-stack-for-MIMXRT1180-EVK-running-Zephyr/td-p/2133940

### Taiwanese SDV Platforms
- Foxconn MIH Open EV Platform — https://www.mih-ev.org/
- Realtek RTL9072/RTL9075 Automotive Ethernet Switch — https://www.realtek.com/en/products/automotive-ethernet
- ITRI / TADA (Taiwan Advanced Automotive Technology Development Association) — https://www.itri.org.tw/

### ROS 2 and Automotive
- Making ROS 2 Automotive Ready — https://www.electronicdesign.com/markets/automotive/video/21240334/making-ros-2-automotive-ready
- AUTOSAR and ROS 2 for SDV (Apex.AI) — https://www.apex.ai/post/autosar-and-ros-2-for-software-defined-vehicle
- Micro XRCE-DDS / micro-ROS — https://micro.ros.org/docs/concepts/middleware/Micro_XRCE-DDS/
- AGL SoDeV Reference Platform — https://www.automotivelinux.org/announcements/sodev/
