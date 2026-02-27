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

### Recommended Development Path

**Phase 1 — Demonstration (accessible today):**
- Use existing ARM-based hardware (STM32H7 or NXP S32K3 with Ethernet + CAN-FD)
- Run nano-ros with `rmw-zenoh` or `rmw-xrce` over Ethernet to a Linux PC running ROS 2
- Demonstrates the zone-controller-to-HPC communication pattern
- Bridge local CAN sensor data in application code

**Phase 2 — AURIX TriCore port:**
- Obtain AURIX TC375 Lite Kit (~$99) + HighTec Rust compiler
- Port `nros` to TriCore (primarily toolchain setup; `no_std` core is arch-agnostic)
- Validate zenoh-pico and XRCE-DDS C libraries build for TriCore
- Demonstrate on the hardware that zone controller OEMs actually use

**Phase 3 — CAN-FD transport (future):**
- Add CAN-FD custom transport for the XRCE backend
- Enables direct DDS-XRCE communication over CAN between zone controller and local ECU nodes
- Alternatively, implement a CAN transport for Zenoh (zenoh-pico supports custom transports)

## References

- Infineon/Flex FlexZoneX Zone Controller Development Kit — https://www.infineon.com/market-news/2025/infatv202601-038
- Infineon Rust Support for AURIX (HighTec compiler) — https://www.infineon.com/design-resources/development-tools/sdk/rust
- Infineon AURIX TC4Dx for Zone Controllers — https://www.infineon.com/products/microcontroller/32-bit-tricore/aurix-tc4x/tc4dx
- AURIX TC375 Lite Kit — https://www.infineon.com/evaluation-board/KIT-A2G-TC375-LITE
- Renesas RH850/U2A Zone/Domain MCU — https://www.renesas.com/en/products/rh850-u2a
- NXP S32Z/E Development Boards — https://www.nxp.com/design/design-center/development-boards-and-designs
- AGL SoDeV Reference Platform — https://www.automotivelinux.org/announcements/sodev/
- Tesla Zonal Architecture — https://www.laitimes.com/en/article/3n8cl_43xcy.html
- Making ROS 2 Automotive Ready — https://www.electronicdesign.com/markets/automotive/video/21240334/making-ros-2-automotive-ready
- AUTOSAR and ROS 2 for SDV (Apex.AI) — https://www.apex.ai/post/autosar-and-ros-2-for-software-defined-vehicle
- Micro XRCE-DDS / micro-ROS — https://micro.ros.org/docs/concepts/middleware/Micro_XRCE-DDS/
- Zonal Architecture Overview (Promwad) — https://promwad.com/news/zonal-architecture-vehicle-electronics
- Zonal vs Domain Architecture (EE Times) — https://www.eetimes.com/automotive-architectures-domain-zonal-and-the-rise-of-central/
- Zone Architecture and Ethernet (EE Times) — https://www.eetimes.com/zone-architecture-ethernet-drive-vehicle-of-the-future/
- Zonal Architecture 101 (onsemi) — https://www.onsemi.com/company/news-media/blog/automotive/en-us/zonal-architecture-101-reducing-vehicle-system-development-complexity
- Microchip Remote Control Protocol and Zonal Architecture — https://www.microchip.com/en-us/about/media-center/blog/2026/rcp-and-concurrent-paradigm-shift-to-zonal-architecture
