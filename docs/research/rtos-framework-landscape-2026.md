# RTOS + middleware landscape — automotive, aerial, adjacent (2026 snapshot)

**Scope.** Real-world RTOS + framework choices in production-shipping
products, not academic toy stacks. Drives nano-ros's adoption / port
prioritisation: which RTOS + which middleware do real users already
have, and what does a nano-ros story to them look like?

**Method.** Public sources (vendor docs, OEM press, conference talks,
open-source repos) as of Jan 2026 cutoff. Numbers are order-of-magnitude;
exact units / EOL dates move per-program.

## 1. Automotive

### 1.1. Production ECUs (deeply embedded, safety-critical)

| Domain | Dominant RTOS | Dominant framework | Notes |
|---|---|---|---|
| Powertrain / chassis MCUs | OSEK/VDX → AUTOSAR Classic | AUTOSAR Classic (C, ASIL-B/D) | Vector MICROSAR, EB tresos. Fixed-rate cyclic tasks, no malloc. |
| Body / comfort (BCM, gateways) | AUTOSAR Classic / OSEK | AUTOSAR Classic | NXP S32K, Renesas RH850, Infineon TriCore / AURIX. |
| ADAS / domain controllers (SoCs, multi-core, Linux+RTOS hybrid) | AUTOSAR Adaptive on Linux/QNX | AUTOSAR Adaptive (C++17, POSIX-PSE51, ARA::Com over SOME/IP / DDS) | NVIDIA Orin, Renesas R-Car V4H, Qualcomm Snapdragon Ride. |
| Infotainment / cluster | Linux (AGL, Android Auto) / QNX | Qt + Wayland + GENIVI/AGL stacks | Not ROS-shaped. |
| Functional-safety MCUs (lockstep, supervising) | SafeRTOS, Hightec PXROS-HR, ETAS RTA-OS, μITRON | AUTOSAR Classic or bare metal | ISO 26262 ASIL-D. |
| Future-tier zonal compute | Adaptive AUTOSAR on QNX or Linux + dedicated safety MCU | Same as ADAS | Trend: collapse 50+ ECUs into 3–5 zonal compute nodes. |

**Major players (commercial):**
- **Tier-1 middleware:** Vector (MICROSAR Classic+Adaptive, market leader),
  Elektrobit (EB tresos, EB corbos, owned by Continental→spun out), ETAS
  (RTA-CAR), KPIT, EPAM Continuum.
- **OS vendors:** QNX (BlackBerry — IVI + ADAS, ~250M cars), Wind River
  VxWorks 7 + Helix (aerospace overlap), Green Hills INTEGRITY (Hyundai
  Genesis cluster, BMW iDrive), SUSE / Wind River Linux (instrument
  clusters), Aurix' own iLLD (bare metal).
- **OEM in-house stacks:** Toyota Arene (proprietary, on Linux + safety
  MCU), Mercedes MB.OS (Linux + QNX hybrid), Tesla (proprietary Linux +
  unknown safety MCU OS, rumoured custom-RTOS for FSD MCU), BMW iX OS
  (Linux + Adaptive AUTOSAR + Cerence), Stellantis STLA Brain (BlackBerry
  IVY + AWS).
- **Volume reality check:** AUTOSAR Classic still ships in 90 %+ of
  produced ICE/early-EV ECUs by unit volume in 2025; Adaptive AUTOSAR is
  growing in the ADAS / centralised-compute tier (~20 % of new platforms
  start with it).

### 1.2. Software-defined vehicle (SDV) / centralised compute push

| Stack | Position | Status 2026 |
|---|---|---|
| SOAFEE (ARM-led, Eclipse Foundation) | Cloud-native containerised on-vehicle compute on Arm | Reference architectures published; Bosch, Continental, Marelli, ZF onboard. Pre-production. |
| Eclipse SDV (Eclipse Foundation) | Open-source SDV middleware (Eclipse Kuksa, Eclipse Velocitas, Eclipse Leda Linux distro, Eclipse Zenoh) | Active. BMW, Bosch, Microsoft, Etas. Zenoh is the data-plane bet. |
| BMW Group VSomeIP / vsomeip | SOME/IP C++ stack (Genivi origin) | Production in BMW iX. Widely adopted as a SOME/IP reference. |
| COVESA / Vehicle Signal Specification (VSS) | OEM-agnostic vehicle data model | Schema, not runtime. Plugged into Kuksa. |
| Apex.AI Apex.OS / Apex.Grace | Certified ROS-2-derived middleware (ISO 26262 ASIL-D) | ZF Volkswagen partnerships. Closed-source distribution on top of ROS 2. |

### 1.3. ROS-shaped automotive software (research / pre-production)

- **Tier IV Autoware (Universe, Core)** — open source, ROS 2 Humble +
  CycloneDDS. Tier IV's commercial offering is the production
  curation; Foxglove + Open Robotics partner. Ships on JPN robotaxi,
  ANA cargo trucks. **Not** safety-island certified — the safety
  island runs separately on a dedicated MCU.
- **Tier IV / NEWSLabNTU Sentinel** (the project nano-ros came out of)
  — 7 ported safety nodes (mrm_*, vehicle_cmd_gate, …) on a Cortex-M
  safety island, FreeRTOS / Zephyr / NuttX targets. Rust rewrite,
  not C++.
- **Autoware Foundation** members — Tier IV (lead), Linux Foundation
  Edge, Foxglove, eSync Alliance, Apex.AI, Arm, AWS, Cyngn, Embotech,
  Intel, LeapMotor, LG, Macnica, Microsoft, NVIDIA, Renesas, SOAFEE,
  Tier4, …
- **OpenADx (Eclipse)** — Bosch-led; less momentum than SOAFEE/Eclipse SDV.

### 1.4. Where nano-ros fits in the automotive picture

- The **safety MCU tier** is the natural insertion — Cortex-M /
  Cortex-R class, deterministic, no_std, deterministic latency. Today
  this slot runs SafeRTOS / AUTOSAR Classic / bare metal; ROS-shaped
  is rare because rclcpp + DDS doesn't fit.
- The **interop story** is what gets us in the door: a safety-island
  binary on FreeRTOS / Zephyr that talks the same Zenoh / Cyclone
  topics as the main Linux + Autoware compute, without bringing in a
  full DDS stack on the MCU. That's the Sentinel pitch + the nano-ros
  pitch.
- **Direct competition:** Apex.OS (paid + certified) and Wind River
  Helix Virtualization (paid + heavyweight). Differentiator: nano-ros
  is open, Rust-led, RTOS-portable, free.

## 2. Aerial

### 2.1. Drone / UAV / eVTOL

| Class | RTOS / OS | Framework | Use |
|---|---|---|---|
| Hobby + light commercial multirotors | NuttX (PX4 default) | PX4 Autopilot (uORB internal, MAVLink external) | DJI dominates closed; PX4 + Ardupilot share the open. ~3M autopilot units shipped on the PX4 stack (Auterion+others). |
| Custom industrial UAVs | NuttX (PX4) / Linux companion | PX4 + ROS 2 via micro-ROS or PX4-ROS 2 bridge | Skydio R3 (proprietary), Auterion Skynode, Modal AI |
| Larger eVTOL / certified UAS | Wind River VxWorks 653 / DO-178C, Lynx LynxOS-178, Green Hills INTEGRITY-178 | ARINC 653 partitioning, DDS-RTPS (RTI Connext DDS pro / Connext Cert) | Joby, Archer, Volocopter, Wisk. Several piloting AUTOSAR-Adaptive-style C++ on certified RTOS. |
| Mil / DoD | Wind River VxWorks, Green Hills INTEGRITY, LynxOS-178 | DDS (RTI), MAVLink derivatives, proprietary | F-35, MQ-9, Northrop Grumman platforms. |

**Major players (aerial):**
- **PX4** ← Dronecode Foundation (Linux Foundation), Auterion (lead
  commercial). PX4 v1.16+ ships uXRCE-DDS as the bridge to ROS 2; the
  legacy MAVROS bridge is the alternative.
- **ArduPilot** ← Linux Foundation, ArduPilot Foundation. ChibiOS,
  Linux, NuttX. Less ROS-coupled than PX4; their ROS 2 bridge is via
  micro-ROS / DDS.
- **micro-ROS** ← eProsima + Bosch + Fraunhofer + iRobot. The
  reference embedded-side ROS 2 client. NuttX, Zephyr, FreeRTOS,
  ThreadX. Has been "the" answer for ROS-on-MCU since 2020.
- **eProsima Fast DDS** — micro-ROS's transport; also the ROS 2
  Humble/Iron default RMW until rmw_zenoh became viable.
- **RTI Connext DDS** ← Real-Time Innovations. Aerospace + defence
  + ADAS DDS leader. Connext Cert is DO-178C / DO-254 / ISO 26262
  variants. NOT free.
- **Wind River**, **Green Hills**, **LynxOS** ← certified RTOS for
  manned + heavy UAS.

### 2.2. nano-ros vs micro-ROS on aerial

This is the head-to-head. Both pitch the same shape: ROS 2 client on
an MCU, talks the same topics as the Linux flight stack. Differences
matter:

| Axis | micro-ROS | nano-ros |
|---|---|---|
| Language | C (the rclc + uxr stack) | Rust no_std (rclrs-mirror) + thin C/C++ surfaces |
| RMW transport | XRCE-DDS only (gates through a host agent) | XRCE *or* zenoh-pico (peer-to-peer, no agent) *or* cyclonedds (peer DDS, embedded) |
| Static-only memory | Yes, by design | Yes, by design (no_std + arena allocator) |
| Discovery | Agent-mediated (no on-device DDS) | XRCE: same. Zenoh/cyclone: on-device peer discovery. |
| Verification story | Static-analysis + Fraunhofer code review | Kani (160 bounded) + Verus (102 unbounded) proofs in-tree |
| Ecosystem | Bigger (incumbent since 2020); ROS 2 community defaults to it | Smaller; differentiated by Rust + multi-RMW + verification |
| Cert path | None claimed; micro-ROS is "MISRA-trending" | None claimed yet; the verification harness is the early-stage cert hook |

**Reality:** micro-ROS is the default a 2026 ROS 2 aerial integrator
reaches for. nano-ros wins on Rust, on peer-discovery (no agent
required), and on multi-RMW flexibility. Story for adoption: PX4 user
who wants the safety MCU to run a Rust ROS client without a host agent
on the autopilot.

## 3. Robotics + industrial + medical

### 3.1. Industrial robotics (arm + AGV/AMR)

| Player | Stack |
|---|---|
| KUKA | KUKA Sunrise.OS (proprietary, VxWorks underneath) + ROS bridge |
| ABB | RobotWare (proprietary RTOS) + EGM (Externally Guided Motion) bridge to ROS |
| FANUC | proprietary; ROS bridges via ROS-Industrial Consortium |
| Universal Robots | URCap (Linux RT) + ROS / ROS 2 driver (community) |
| Kassow Robots | Linux RT + ROS 2 |
| Doosan, Yaskawa, Kawasaki | Each their own RTOS + ROS bridges |
| Mobile (AMR) — MiR, OTTO, 6 River | Linux + ROS 2; some safety MCU layer on RTOS |
| Boston Dynamics Spot | Linux + proprietary API; ROS 2 driver (community + BD-supported) |

**Trend:** ROS 2 is the lingua franca for AMR + research arms; the
proprietary controller talks to a ROS 2 layer running on a Linux
companion. Safety MCUs sit underneath, almost always closed RTOS.

### 3.2. ROS-Industrial / Open RMF

- ROS-Industrial Consortium (Americas, Europe, Asia-Pacific) —
  industrial-shaped ROS 2 packages, calibration, force-control.
- Open RMF (Robot Middleware Framework) — multi-vendor fleet
  coordination on ROS 2. Sponsored by Singapore's IHL + Open
  Robotics.

### 3.3. Medical robotics

- **Surgical robots (Intuitive da Vinci, CMR Versius, Medtronic
  Hugo, etc.)** — closed Linux + closed safety RTOS. ROS used in
  research / academic robots (Raven II, MARS), not production.
- **Imaging / IVD / point-of-care** — Linux dominant, Yocto +
  ros2-style topic models internal. IEC 62304 + FDA pre-cert paths.
- **Implantables / patient monitors** — bare-metal or proprietary
  RTOS (FreeRTOS, NuttX, μITRON). No ROS shape; CAN / Bluetooth LE
  Audio host-link.

### 3.4. Space + satellites

- **NASA cFS (core Flight System)** + cFE — the de facto open
  space-grade C framework. RTEMS underneath; runs on RAD750,
  LEON, etc.
- **ESA OPSAT / OPSAIRS** — cFS-derived.
- **CubeSat / small-sat** — FreeRTOS, NuttX, Zephyr (growing).
  cFS works here too. No ROS adoption to speak of.
- **Modern New Space (SpaceX Starlink, Planet, Iceye, Capella,
  Maxar)** — proprietary stacks (often Linux + custom userspace).
  Tesla heritage at SpaceX shows.

### 3.5. Industrial PLC / control

- **Codesys**, **B&R Automation**, **Beckhoff TwinCAT**, **Rockwell
  Studio 5000** — closed PLC ecosystems. RTOS underneath
  (proprietary or VxWorks).
- **OPC UA** for vendor-interop; ROS 2 ⟷ OPC UA bridges exist.
- ROS doesn't compete here; it integrates above.

## 4. RTOS-level landscape (free / paid)

| RTOS | Source | Typical use | Cert | Where ROS-shaped clients matter |
|---|---|---|---|---|
| **FreeRTOS** | Amazon (free, MIT) | AWS IoT, fleet of Cortex-M MCUs, hobby + commercial low-end | SafeRTOS variant DO-178C, IEC 61508, ISO 26262 | huge: nano-ros + micro-ROS both target it |
| **Zephyr** | Linux Foundation (Apache 2) | New industry baseline 2022+; Nordic, NXP, Intel, STM. | LTS line being safety-audited (PRECISE, Zephyr Safety Working Group) | growing fast |
| **NuttX** | Apache (Apache 2) | PX4 default; many MCU vendors; POSIX-shape | none claimed | PX4-coupled |
| **ThreadX** (Eclipse ThreadX since 2023) | Microsoft → Eclipse (MIT) | Microsoft Azure RTOS / FileX/NetX, EOL'd at MS, lives in Eclipse | IEC 61508 / 62304 / ISO 26262 (kept under Eclipse) | yes; nano-ros ships it |
| **Mbed OS** | Arm | Cortex-M generic | none mainstream | EOL'd 2026 (Arm announced retirement). |
| **RIOT** | community (LGPL) | IoT mesh | none | small |
| **Apache NuttX** | (above) | | | |
| **Wind River VxWorks** | proprietary | aerospace, defence, industrial, IVI | DO-178C, IEC 61508, ASIL-D | ROS 2 supported via Wind River Studio Linux Distribution + DDS; embedded ROS via ROSi pilots |
| **Green Hills INTEGRITY / INTEGRITY-178** | proprietary | aerospace, automotive cluster | DO-178C, ASIL-D | uses native middleware |
| **BlackBerry QNX** | proprietary | IVI + ADAS + medical | ISO 26262 ASIL-D | hosts ROS via QNX Foundry / community ports |
| **SafeRTOS** | WHIS (paid) | functional-safety MCUs | IEC 61508 SIL-3 / ISO 26262 ASIL-D | freertos-source-compatible API |
| **Sysgo PikeOS** | proprietary | rail + avionics + automotive Adaptive AUTOSAR host | DO-178C, ASIL-D | hypervisor + AUTOSAR Adaptive |
| **Lynx LynxOS-178 / LynxSecure** | proprietary | mil avionics | DO-178C, MILS | DDS via RTI |
| **eSOL eMCOS** | proprietary | EU/JP automotive Adaptive AUTOSAR | ASIL-D | OEM-deal-based |
| **Microsoft Azure Sphere** | proprietary | IoT (EOL'd 2027) | n/a | retiring |
| **NXP MCUXpresso SDK / Renesas e²studio** | vendor BSP, RTOS-agnostic | | | |
| **RTEMS** | OAR (free, dual GPL/RTEMS) | space (cFS) | DO-178B + FACE | cFS ecosystem |

## 5. Middleware / framework matrix

### 5.1. DDS / pub-sub layer

| Implementation | Source | Position 2026 |
|---|---|---|
| **eProsima Fast DDS** | open (Apache 2) | ROS 2 Humble default RMW until Iron; still widely deployed |
| **Eclipse Cyclone DDS** | open (EPL 2) | ROS 2 Iron/Jazzy default RMW; AWS-backed; lightest in stable production |
| **RTI Connext DDS** | proprietary | aerospace + defence + ADAS DDS leader; certified variants |
| **OpenSplice DDS** | originally PrismTech → ADLINK | declining |
| **Twin Oaks CoreDX DDS** | proprietary | mil + small footprint niche |
| **GurumDDS** | proprietary (KR) | regional |
| **Zenoh** | Eclipse (EPL 2.0) | ROS 2 alt RMW (rmw_zenoh); Eclipse SDV bet; geographically-distributed |
| **MQTT / MQTT-SN / NanoMQ** | open | IoT, not ROS-shaped but interop bridges exist |
| **uORB** (PX4 internal) | open | PX4-only, in-process |
| **iceoryx2 (Eclipse iceoryx)** | open (EPL 2) | shared-memory zero-copy; DDS-adjacent |

### 5.2. ROS-shaped middleware

| Layer | Where it lives |
|---|---|
| rclcpp / rclpy / rclrs (Ros 2 Humble/Iron/Jazzy) | hosted Linux / macOS / Windows |
| micro-ROS rclc + uxr | RTOS MCU client; agent-mediated DDS |
| nano-ros | RTOS MCU client; peer or agent; Rust-led |
| Apex.OS | ROS 2 certified subset; commercial |
| Eclipse Cyclone DDS C++ binding (DDSi-RTPS) | native DDS users skipping rclcpp |
| RTI Connext Cert | DDS for certified targets |
| ARA::Com (Adaptive AUTOSAR) | service-oriented C++ over SOME/IP / DDS |

### 5.3. Trend lines through 2026

- **Zenoh ↑.** Eclipse SDV's bet; rmw_zenoh shipping in Jazzy; rapidly
  becoming the "what comes after fastdds" answer for new ROS 2 stacks.
- **DDS ↘ for ROS 2 default,** ↗ for safety-certified aerospace.
  Cyclone holds the open-source ROS 2 ground; Connext keeps the
  cert+pro tier.
- **AUTOSAR Adaptive ↑.** From 20 % of new ADAS programs to ~40 % by
  2026 per IHS Markit forecasts. Hostile to ROS-shape but fertile
  ground for DDS-aware MCU clients.
- **Rust on RTOS ↑.** Embassy, RTIC, defmt growing; Zephyr's
  zephyr-lang-rust LTS. Differentiator for nano-ros + Tock for
  isolation, even if rare in production cars.
- **MISRA / certified-Rust** ↗ Ferrous Systems Ferrocene 2024 → Critical
  Section in 2025 → various ISO-26262-trending certifications. Opens
  the door for Rust safety-island.

## 6. Major-player matrix — who picks what

| Company | RTOS / OS | Middleware | Notes |
|---|---|---|---|
| Toyota Arene | Linux + safety MCU OS undisclosed | undisclosed; pulling SDV in-house | Arene targets 2025–26 |
| Mercedes MB.OS | Linux + QNX | AUTOSAR Adaptive + Mercedes proprietary | Yarrow + Microsoft Azure cloud-side |
| BMW Group | Linux + Adaptive (BMW VsomeIP) | Eclipse SDV early adopter; Zenoh PoCs | iX / iDrive 9 |
| VW CARIAD | Linux + Adaptive | AUTOSAR Adaptive + ASG / Cariad's own | VW PPE platform |
| Stellantis STLA Brain | Linux + Adaptive | + BlackBerry IVY (data only) + AWS | Maserati GranTurismo Folgore launch |
| Tesla | Linux + custom safety MCU OS | proprietary | FSD HW3/HW4 |
| Hyundai/Kia | QNX (cluster) + Linux (IVI) + Adaptive on Tier 1s | mixed | Genesis luxury cars |
| GM Ultifi | Linux + safety MCU | early SDV pivot; Foxglove partner | Cadillac Lyriq |
| Ford Power-Up | Linux + AUTOSAR Classic + Adaptive Tier-1 | google-partnered IVI | F-150 Lightning |
| Bosch (Tier-1) | promotes Eclipse SDV + Zenoh + Linux | full middleware portfolio | major Eclipse SDV backer |
| Continental | EB tresos / EB corbos | AUTOSAR Classic + Adaptive | strong on Cyclone DDS + Zenoh |
| ZF | own Linux dist + Apex.OS partnership | ProAI compute | Apex.OS for safety-island |
| NVIDIA DRIVE | DRIVE OS (Linux + QNX) + DriveWorks | DDS + custom NV middleware | Mercedes, Hyundai, Lucid |
| Qualcomm | QNX + Linux on Snapdragon Ride | Adaptive AUTOSAR | BMW, Stellantis, Honda |
| Renesas | RH850 (Classic AUTOSAR) + R-Car (Adaptive) | combo | Toyota + others |
| NXP | S32 (Classic) + S32G (Adaptive) | combo | broad |
| Infineon | AURIX (Classic AUTOSAR / SafeRTOS) | safety MCU dominator | broad |
| ESA / NASA | RTEMS + cFS / cFE | spaceflight pubsub | cFS is the open standard |
| Skydio | Linux + custom flight code | not PX4 (forked early) | military + enterprise |
| Auterion | PX4 on NuttX + Linux companion | uXRCE-DDS + MAVLink | PX4 commercial lead |
| Boston Dynamics | Linux + proprietary | ROS 2 driver community + BD | Spot / Stretch |
| Tier IV / Autoware | Linux (ROS 2 Humble → Iron) + Sentinel-style safety MCU | Cyclone DDS (Autoware), Zenoh PoCs | Mainline open Autoware |
| Apex.AI | safety-certified ROS 2 derivative | DDS, partner with Cyclone | ZF, Volkswagen, Continental |

## 7. Implications for nano-ros

1. **Sweet spot: safety MCU + Linux compute** (automotive Sentinel
   pattern; aerial PX4 safety-companion). Today's slot belongs to
   AUTOSAR Classic / SafeRTOS (closed) or micro-ROS (C, agent-mediated).
   nano-ros's pitch: open Rust, peer-discovery, multi-RMW.
2. **RMW that aligns with the market:** rmw_zenoh is the rising star
   on the Linux compute side (Eclipse SDV + rmw_zenoh in Jazzy);
   nano-ros already ships zenoh-pico (the natural MCU sibling). XRCE
   keeps us PX4-compatible; Cyclone keeps us
   Autoware-/ROS-2-compatible.
3. **Certification path matters even as a future promise.** The Verus
   + Kani harness is a real differentiator vs micro-ROS's
   static-analysis-only story. Ferrocene + zero-stdlib panic Rust as
   the cert language adds a credible long arc.
4. **Don't compete with DDS-on-Linux frameworks** (rclcpp, Cyclone,
   Connext) — interoperate. Topic-name + type-hash wire compatibility
   is the moat.
5. **Real first targets to chase:**
   - **PX4 safety companion** — large open-source community,
     uXRCE-DDS is already there, micro-ROS is direct competition; the
     Rust angle differentiates.
   - **Tier IV Sentinel-style safety island** — nano-ros's birth
     project; immediately ports a real automotive control stack.
   - **A non-Tier-IV Autoware downstream** that wants a safety island
     they didn't have to build — same pitch as Sentinel, broader
     audience.
   - **Industrial AMR safety MCU** — easier-to-enter than automotive,
     same shape (Linux + ROS 2 + safety MCU).
6. **Strategic gap to fill:** no widely-adopted **open Rust ROS 2
   middleware** on Linux. rclrs (Open Robotics) exists but is
   light-staffed. If nano-ros's Rust idioms grow upward to a hosted
   profile, that's a real ecosystem position.

## 8. References (where the numbers come from)

- IHS Markit / S&P Global Mobility ECU + Adaptive AUTOSAR forecasts
- Eclipse Foundation 2024 + 2025 SDV reports
- AUTOSAR member list + roadmap publications
- Dronecode Foundation member surveys, PX4 release notes through v1.16
- ROS 2 community survey (Open Robotics, annual)
- ROS-Industrial Consortium 2025 quarterly review
- NASA cFS GitHub + adoption pages
- Vendor product pages (Vector, EB, ETAS, Wind River, Green Hills, QNX,
  RTI, Apex.AI) as of Jan 2026
- Conference talks: ROSCon 2023–25, Embedded World 2024–25, Autosar
  Open Conference 2024, Eclipse SDV Day 2024–25, IROS 2024–25.

(All figures are public-sources mid-2025 to early-2026; verify per
specific program / OEM before quoting in a customer-facing deck.)
