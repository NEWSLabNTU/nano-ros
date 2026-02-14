# Safety-Critical Platform Study: Vehicles, Trains, and Drones

> Date: 2026-02-14
> Purpose: Study real-time safety-critical platform architectures across industries
> Focus: Real-timeness and safety (not power saving or computation constraints)

## Table of Contents

1. [Cross-Domain Safety Standards](#1-cross-domain-safety-standards)
2. [Automotive Platforms](#2-automotive-platforms)
3. [Railway Platforms](#3-railway-platforms)
4. [Aviation & Drone Platforms](#4-aviation--drone-platforms)
5. [Cross-Domain Architectural Patterns](#5-cross-domain-architectural-patterns)
6. [Certified RTOS Landscape](#6-certified-rtos-landscape)
7. [Hardware Safety Mechanisms](#7-hardware-safety-mechanisms)
8. [Deterministic Communication](#8-deterministic-communication)
9. [Implications for nano-ros & Autoware](#9-implications-for-nano-ros--autoware)

---

## 1. Cross-Domain Safety Standards

All safety-critical industries derive from **IEC 61508** (functional safety of E/E systems), which defines Safety Integrity Levels (SIL 1-4). Domain-specific standards add industry context:

| Domain     | Standard       | Integrity Levels | Failure Rate (per hour) |
|------------|----------------|------------------|-------------------------|
| Generic    | IEC 61508      | SIL 1-4          | 10^-5 to 10^-9          |
| Automotive | ISO 26262      | ASIL A-D         | QM to 10^-8             |
| Railway    | EN 50128/50129 | SIL 1-4          | 10^-5 to 10^-9          |
| Aviation   | DO-178C/DO-254 | DAL A-E          | 10^-3 to 10^-9          |

### Mapping Between Standards

| IEC 61508 | ISO 26262 | EN 50128 | DO-178C | Failure Rate/hr  |
|-----------|-----------|----------|---------|------------------|
| SIL 4     | --        | SIL 4    | DAL A   | <= 10^-9         |
| SIL 3     | ASIL D    | SIL 3    | DAL B   | <= 10^-8 / 10^-7 |
| SIL 2     | ASIL B/C  | SIL 2    | DAL C   | <= 10^-7 / 10^-5 |
| SIL 1     | ASIL A    | SIL 1    | DAL D   | <= 10^-6 / 10^-5 |
| --        | QM        | --       | DAL E   | No requirement   |

Note: ISO 26262 does not define ASIL equivalent to SIL 4. The highest automotive level (ASIL D) maps approximately to SIL 3. Railway interlocking (SIL 4) and primary flight controls (DAL A) are the most demanding safety requirements in practice.

### Common Verification Requirements at Highest Levels

All three domains require at highest integrity levels:
- **Modified Condition/Decision Coverage** (MC/DC) for structural test coverage
- **Formal methods** (highly recommended or required)
- **Independent verification** (reviewer != developer)
- **Requirements traceability** (every requirement -> design -> code -> test)
- **Tool qualification** (compilers, test tools must be assessed)
- **Configuration management** of all artifacts

---

## 2. Automotive Platforms

### 2.1 Safety MCU Architectures

Automotive safety MCUs provide hardware-level fault detection through lockstep cores, ECC memory, and integrated safety management:

#### Infineon AURIX TC3xx (TriCore)

The dominant safety MCU for automotive powertrain and chassis:

- Up to 6 TriCore 1.6.2 cores at 300 MHz
- 4 cores can form 2 dual-core lockstep (DCLS) pairs
- Hardware Safety Management Unit (SMU) aggregates all fault signals
- ASIL D certified by TUV SUD
- Integrated ADAS interface: RADAR/CAN/FlexRay/Ethernet
- Extensive ECC on all memory (SRAM, flash, cache)
- Hardware BIST (Built-In Self-Test) at startup and runtime

```
+------------------------------------------+
|           AURIX TC3xx SoC                |
|                                          |
|  +--------+  +--------+  +--------+     |
|  |Core 0  |  |Core 2  |  |Core 4  |     |
|  |Lockstep|  |Lockstep|  |(free)  |     |
|  |w/Core 1|  |w/Core 3|  |        |     |
|  +---+----+  +---+----+  +--------+     |
|      |           |                       |
|  +---v-----------v---+                   |
|  |  Safety Mgmt Unit |                   |
|  |  (SMU) - all HW   |                   |
|  |  fault aggregation |                   |
|  +--------------------+                  |
|                                          |
|  ECC SRAM | ECC Flash | HW BIST         |
+------------------------------------------+
```

#### TI TDA4VM / Hercules TMS570

- **TDA4VM**: SoC with Cortex-A72 (performance) + Cortex-R5F (safety) + DSP + ML accelerator
  - Cortex-R5F runs in dual-core lockstep for ASIL D safety functions
  - Safety island architecture: R5F monitors A72 domain
  - Used in ADAS domain controllers
- **TMS570LC4357**: Pure safety MCU, dual Cortex-R5F lockstep at 300 MHz
  - IEC 61508 SIL 3 certified by TUV SUD
  - Comprehensive HW BIST, ECC, voltage/clock monitoring

#### NXP S32K3 / S32G

- **S32K3**: Cortex-M7 based safety MCU, ASIL D certified
  - Integrated Safety Element out of Assurance (SEooA)
  - Hardware lockstep on Cortex-M7 core pairs
- **S32G**: Vehicle Network Processor for service-oriented gateways
  - Cortex-A53 (performance) + Cortex-M7 lockstep (safety)
  - Hardware Security Engine (HSE)
  - Ethernet TSN + CAN FD + LIN

#### Renesas RH850/U2A

- Multi-core RH850 architecture with lockstep
- ASIL D certified, widely deployed in Japanese OEMs
- Integrated AUTOSAR MCAL (Microcontroller Abstraction Layer)
- ICU-S (Intelligent Cryptographic Unit) for secure boot

### 2.2 AUTOSAR (AUTomotive Open System ARchitecture)

#### AUTOSAR Classic (MCU-based, static)

Designed for deeply embedded safety-critical ECUs:

- **Static task configuration**: All tasks, priorities, and timing defined at design time
- **Memory partitioning**: OS-Applications with MPU-enforced isolation
- **Timing protection**: Budget monitoring per task, execution time limits
- **Scalability classes**: SC1 (basic) through SC4 (full timing + memory protection)
- **Safety OS**: SC3/SC4 qualified to ASIL D

```
+-----------------------------------------------------+
|  AUTOSAR Classic Architecture                        |
|                                                      |
|  +----------+ +----------+ +----------+              |
|  | SWC 1    | | SWC 2    | | SWC 3    |  Application |
|  |(ASIL D)  | |(ASIL B)  | |(QM)      |  Layer       |
|  +----+-----+ +----+-----+ +----+-----+              |
|       |             |            |                    |
|  =====|=============|============|==== RTE ========== |
|       |             |            |   (Runtime Env)    |
|  +----v-----+ +-----v----+ +----v-----+              |
|  |COM       | |OS        | |Safety    |  BSW         |
|  |DET       | |WdgM      | |Dem       |  Layer       |
|  |Diagnostic| |Memory    | |FiM       |              |
|  +----------+ +----------+ +----------+              |
|                                                      |
|  +--------------------------------------------------+|
|  |  MCAL (Microcontroller Abstraction Layer)         ||
|  +--------------------------------------------------+|
+-----------------------------------------------------+
```

Key safety services:
- **WdgM** (Watchdog Manager): Supervises execution flow via checkpoints (alive, deadline, logical)
- **Dem** (Diagnostic Event Manager): Records faults with debouncing
- **FiM** (Function Inhibition Manager): Disables functions based on fault state

#### AUTOSAR Adaptive (POSIX-based, dynamic)

For high-performance ECUs (ADAS domain controllers):

- Runs on POSIX OS (QNX, PikeOS, Linux with PREEMPT_RT)
- Service-oriented communication (SOME/IP, DDS)
- Dynamic process lifecycle
- Health monitoring via platform health management
- ASIL B/D with QNX Hypervisor or PikeOS partitioning

### 2.3 Automotive Hypervisors

Hypervisors enable mixed-criticality on a single SoC:

| Hypervisor               | Certification          | Type   | Key Feature                         |
|--------------------------|------------------------|--------|-------------------------------------|
| **QNX Hypervisor**       | ASIL D (TUV SUD)       | Type 1 | Microkernel, IPC-based isolation    |
| **PikeOS**               | ASIL D + SIL 4 + DAL A | Type 1 | Broadest cross-domain certification |
| **COQOS Hypervisor SDK** | ASIL D capable         | Type 1 | OpenSynergy, VirtIO-based           |
| **Xen / ARINC 653**      | Research / military    | Type 1 | Open source, ARINC 653 scheduler    |

```
+-----------------------------------------------------------+
|  Automotive Hypervisor Architecture                        |
|                                                            |
|  +----------+  +-----------+  +----------+  +-----------+ |
|  | ASIL D   |  | ASIL B    |  | QM Linux |  | QM Android| |
|  | AUTOSAR  |  | AUTOSAR   |  | ADAS     |  | IVI       | |
|  | Classic  |  | Adaptive  |  | Stack    |  |           | |
|  +----+-----+  +-----+-----+  +----+-----+  +-----+-----+ |
|       |              |              |              |        |
|  =====|==============|==============|==============|======  |
|  Spatial + Temporal Partitioning (MMU + Scheduler)         |
|  =====|==============|==============|==============|======  |
|       |              |              |              |        |
|  +----v--------------v--------------v--------------v-----+ |
|  |              Certified Hypervisor Kernel               | |
|  |          (ASIL D / SIL 4 certified)                    | |
|  +--------------------------------------------------------+ |
|                                                            |
|  Hardware: ARM Cortex-A + Cortex-R (lockstep)             |
+-----------------------------------------------------------+
```

### 2.4 Watchdog Architectures

#### Internal Watchdogs (MCU-integrated)

- **IWDG** (Independent Watchdog): Free-running, separate clock domain, simple timeout
- **WWDG** (Window Watchdog): Must be refreshed within a time window (not too early, not too late)

#### External Safety Watchdogs

- **NXP FS26**: ASIL D certified companion IC
  - Challenge-response protocol (prevents accidental feeding)
  - Monitors Vdd, FCCU output, reset pin
  - Independently triggers safe state if MCU fails
- **Infineon TLF35584**: Pre-qualified ASIL D PMIC with watchdog
  - SPI-configurable timeout and window
  - Integrated voltage regulator + watchdog

#### AUTOSAR Watchdog Manager (WdgM)

Software-level multi-checkpoint supervision:

```
Task A:  [CP1] -----> [CP2] -----> [CP3]
                                     |
          Alive supervision (periodic)
          Deadline supervision (CP1 -> CP3 within T)
          Logical supervision (CP1 must precede CP2 must precede CP3)
```

Three supervision modes:
1. **Alive**: Periodic check — task must reach checkpoint N times per supervision cycle
2. **Deadline**: Time between two checkpoints must be within [min, max]
3. **Logical**: Execution sequence must follow defined graph (catches control flow errors)

---

## 3. Railway Platforms

### 3.1 Railway Safety Standards (CENELEC EN 5012x)

The European railway safety standards form a layered framework:

- **EN 50126** (IEC 62278): System-level RAMS (Reliability, Availability, Maintainability, Safety) lifecycle
- **EN 50128**: Software development for railway control and protection systems
- **EN 50129**: Safety-related electronic systems for signaling — defines architectural requirements
- **EN 50159**: Safety-related communication — defines "black channel" approach for safe communication over untrusted networks

### SIL Requirements by Function

| Function                                    | SIL         | Failure Rate/hr |
|---------------------------------------------|-------------|-----------------|
| Interlocking (route locking, point control) | **SIL 4**   | <= 10^-9        |
| ATP (Automatic Train Protection)            | **SIL 4**   | <= 10^-9        |
| ETCS Onboard (European Vital Computer)      | **SIL 4**   | <= 10^-9        |
| ETCS Trackside (Radio Block Centre)         | **SIL 4**   | <= 10^-9        |
| Level crossing protection                   | **SIL 3-4** | <= 10^-8        |
| ATO (Automatic Train Operation)             | **SIL 2**   | <= 10^-7        |
| ATS (Automatic Train Supervision)           | **SIL 2**   | <= 10^-7        |
| Passenger information                       | **SIL 0-1** | --              |

### 3.2 Vital Computer Architectures

Railway signaling uses the most conservative safety architectures in any industry. Key platforms:

#### Siemens SIMIS W — 2-out-of-3 Hardware Voting

```
+----------------------------------------------+
|           SIMIS W Vital Platform              |
|                                               |
|  +----------+  +----------+  +----------+    |
|  |Channel A |  |Channel B |  |Channel C |    |
|  | (CPU)    |  | (CPU)    |  | (CPU)    |    |
|  | Same SW  |  | Same SW  |  | Same SW  |    |
|  +----+-----+  +----+-----+  +----+-----+    |
|       |              |              |         |
|       +------+-------+------+------+         |
|              |              |                 |
|       +------v--------------v------+          |
|       |  Hardware Voter (2oo3)     |          |
|       |  Majority decision logic   |          |
|       +------------+---------------+          |
|                    |                          |
|            Safe Outputs                       |
+----------------------------------------------+
```

- Three identical channels running identical software
- Hardware-voted majority decision
- Single channel failure: detected and tolerated (system continues on 2 agreeing channels)
- Certified SIL 4, response time < 150 ms
- Hundreds of installations worldwide

#### Alstom Smartlock 400 — 2oo3 with Diverse CPUs

- Each Processing Module has 3 interconnected PowerPC processors
- 2 cores run interlocking logic with cross-checking; 3rd runs diagnostics
- 30-40 Virtual Interlockings per system
- SIL 4 certified

#### Bombardier/Alstom EBI Lock 950 — Hot Standby + 2oo2

- Primary + hot standby computers, each internally 2oo2 (dual CPU cross-check)
- Switchover < 150 ms on primary failure, no route data loss
- Over 600 systems delivered to 50+ customers in 30+ countries
- SIL 4 certified

#### Hitachi MicroLok II — Diverse Dual Software on Single Processor

- Single Motorola 68332 CPU
- Two independently developed software versions cross-checking on same processor
- SIL 4 certified
- Most widely deployed wayside controller globally

#### CLEARSY Safety Platform — Formally Verified

- Software proven correct using **B formal method** (Atelier B)
- Mathematical proof eliminates need for unit/integration testing
- Dual CPU with hardware comparator
- SIL 4 certified by CERTIFER

### 3.3 Coded Processor Technique

Achieves SIL 4 safety on a single hardware channel using arithmetic safety codes:

```
Standard variable:   X
Coded variable:      X_coded = A * X + signature
                     A = large prime (arithmetic code base)

Every operation:
  result_f = operation(X_f)          -- functional result
  result_c = code_operation(X_c)     -- coded result

Verification:
  Check: result_c == expected_code(result_f)
  Mismatch -> hardware error detected -> fail-safe shutdown
```

Detects: operation errors, operand corruption, data staleness, sequencing errors. Pioneered by CSEE Transport (now Alstom) with DIGISAFE in the 1980s. Key advantage: safety argument is hardware-independent.

### 3.4 Railway Communication: ETCS and Black Channel

#### ETCS Architecture

```
ONBOARD:                      TRACKSIDE:
+------------------+          +----------------+
| EVC (SIL 4)     |<-------->| RBC (SIL 4)   |
| - Speed supervise|  Radio   | - MA calc      |
| - MA management  |  (GSM-R/ | - Route mgmt   |
| - Braking curves |  future  |                |
| - Emergency brake|  FRMCS)  +----------------+
+--------+---------+
         |                    +----------------+
+--------v---------+          | Eurobalise     |
| BTM (Balise      |<------->| (Passive       |
|  Transmission)   |  27MHz   |  transponder)  |
+------------------+          +----------------+
```

#### Euroradio Safety Protocol

- **3DES MAC** with 192-bit key (3 distinct keys)
- Sequence numbers + timestamps for replay/delay detection
- Keys managed by centralized Key Management Centre (KMC)

#### EN 50159 Black Channel

The "black channel" principle treats any underlying transport as untrusted. The safety layer adds:

| Threat       | Defense                          |
|--------------|----------------------------------|
| Corruption   | CRC                              |
| Repetition   | Sequence number                  |
| Deletion     | Sequence number + timeout        |
| Insertion    | Sequence number + authentication |
| Resequencing | Sequence number                  |
| Delay        | Timestamp / timeout              |
| Masquerade   | Authentication code              |

This is directly relevant to nano-ros: zenoh is the "black channel," and a safety protocol layer on top provides the safety guarantees.

### 3.5 N-Version Programming in Rail

Used extensively for SIL 4 software:

- Same specification, different teams, different languages, different compilers
- Examples: one version in Ada, another in C; different formal methods (B method vs SCADE)
- EN 50128 requires T3 tool diversity (different compilers) at higher SIL levels

### 3.6 Safety Bag Pattern

An independent safety monitor that can only block (never command):

```
Main System (complex, SIL 2-3)
    |
    v proposed output
Safety Bag (simple, SIL 4, formally verified)
    |
    Pass: execute    Fail: safe state (all signals to danger)
```

Rules are simple invariants: "no conflicting routes," "no signal cleared without route locked," etc. The safety bag is auditable due to minimal code. This pattern maps directly to the Autoware safety island concept.

---

## 4. Aviation & Drone Platforms

### 4.1 DO-178C Design Assurance Levels

| DAL | Failure Condition | Rate/flight-hr | MC/DC Required     | Independent Verification |
|-----|-------------------|----------------|--------------------|--------------------------|
| A   | Catastrophic      | <= 10^-9       | Yes                | 30 objectives            |
| B   | Hazardous         | <= 10^-7       | No (DC sufficient) | 18 objectives            |
| C   | Major             | <= 10^-5       | No                 | 5 objectives             |
| D   | Minor             | > 10^-5        | No                 | 2 objectives             |
| E   | No safety effect  | N/A            | No                 | 0 objectives             |

DO-178C supplements: DO-331 (model-based), DO-332 (OOP), DO-333 (formal methods).

### 4.2 Flight Control Computer Architectures

#### Boeing 777 PFC — Triple-Triple Dissimilar Redundancy

The gold standard for fly-by-wire safety architecture:

```
PFC Left:     Lane 1: AMD 29050 (RISC)  |
              Lane 2: Intel 80486 (CISC) |-> Intra-channel vote
              Lane 3: Motorola 68040     |

PFC Center:   Lane 1: AMD 29050         |
              Lane 2: Intel 80486       |-> Intra-channel vote
              Lane 3: Motorola 68040     |

PFC Right:    Lane 1: AMD 29050         |
              Lane 2: Intel 80486       |-> Intra-channel vote
              Lane 3: Motorola 68040     |

              Inter-channel vote -> Actuator commands
```

- 3 channels x 3 lanes = 9 processors total
- **Dissimilar processors** per lane (RISC + 2 different CISC)
- **Dissimilar software** per lane (different implementations, potentially different languages)
- Survives any single processor failure, any single channel failure, and any common-mode failure affecting one processor family
- Communication via three independent ARINC 629 buses

#### Airbus A320 — Dissimilar Computer Types

```
ELAC 1,2 (Motorola 68010): Elevator + Aileron (primary)
SEC 1,2,3 (Intel 80186):   Spoiler + Elevator (standby)
FAC 1,2:                   Flight envelope, yaw damper
```

- Different processor families between computer types (not within)
- Different manufacturers: Thomson-CSF (ELAC) vs SFENA (SEC)
- A bug in ELAC software cannot propagate to SEC

#### Airbus A380 — Triple-Dual Architecture

```
3x PRIM (PowerPC 755):  Full flight control laws, each dual-lane (command+monitor)
3x SEC (SHARC DSP):     Simplified direct laws (dissimilar from PRIMs)
1x BCM:                 Backup purely electrical, three-axis
Network:                AFDX (first aircraft to use switched Ethernet)
```

#### Command-Monitor Pattern

Used within individual flight control computers:

```
Sensor Inputs ----+----> Command Lane ----> Actuator Commands
                  |                              |
                  +----> Monitor Lane -------> Compare
                                                 |
                                    Match: valid    Mismatch: disconnect
```

Both lanes compute independently. Monitor checks command lane's output. On mismatch, channel declares itself faulty and disconnects. Used in all Airbus PRIMs and SECs.

#### FADEC (Full Authority Digital Engine Control)

- Dual-channel (active/standby), each with own sensors and processor
- No mechanical reversion — FADEC has complete authority
- DAL A for single-engine aircraft; DAL B for multi-engine (redundancy mitigation)

### 4.3 Integrated Modular Avionics (IMA)

Evolution from federated (one box per function) to shared computing:

```
Federated:  [FCS box] [Nav box] [Engine box] [Display box]
                point-to-point wiring

IMA:        [Partition A] [Partition B] [Partition C] [Partition D]
            |  FCS (DAL A) | Engine (B)  | Nav (C)    | Display (D) |
            +------- Shared Hardware, ARINC 653 RTOS ---------------+
            +------- AFDX Network (deterministic Ethernet) ---------+
```

Benefits: reduced weight, power, wiring; easier upgrades; lower cost. Requires robust partitioning (ARINC 653).

### 4.4 ARINC 653 Partitioning (Detailed)

Two-level hierarchical scheduling:

**Level 1 (Inter-partition)**: Static cyclic schedule
```
|<======== Major Time Frame (e.g., 40ms) ========>|
| Part A | Part B |  Part C  | Part A | Part D    |
| 5ms    | 8ms    | 10ms     | 5ms    | 7ms       |
```

- Partition windows are fixed at integration time (not runtime)
- Non-preemptible between partitions
- Each partition gets guaranteed CPU time regardless of other partitions

**Level 2 (Intra-partition)**: Priority-based preemptive scheduling within each partition's time window

**Inter-Partition Communication**:
- **Sampling ports**: Latest-value (overwrite), no queuing — like nano-ros subscription buffer
- **Queuing ports**: FIFO with bounded depth — like nano-ros service queue

**Health Monitoring** (3 levels):
- Process level: deadline miss, stack overflow, illegal instruction
- Partition level: initialization failure, partition deadline overrun
- Module level: power failure, hardware fault

### 4.5 Drone/UAS Safety Architectures

#### Certification Approaches

| Approach          | Standard          | Target                          |
|-------------------|-------------------|---------------------------------|
| Type Certificate  | DO-178C + DO-254  | Large UAS, eVTOL (Joby, Archer) |
| Special Condition | FAA Part 21.17(b) | Novel categories (powered-lift) |
| SORA (Specific)   | JARUS SORA v2.5   | Medium UAS, BVLOS operations    |
| Open Category     | EU 2019/945       | Small UAS, VLOS, low risk       |

#### JARUS SORA Risk Assessment

Maps UAS operations to SAIL (Specific Assurance and Integrity Level I-VI):
- Ground Risk Class (GRC 1-10) based on kinetic energy + environment
- Air Risk Class (ARC-a to ARC-d) based on airspace encounter probability
- SAIL determines 24 Operational Safety Objectives (OSO) robustness requirements

#### Run-Time Assurance (ASTM F3269)

Enables ML/AI in safety-critical UAS without certifying the neural network:

```
+------------------+   +--------------------+
| Complex Function |   | Certified Safety   |
| (ML/AI, not      |   | Controller         |
|  certifiable)    |   | (DAL A/B)          |
+--------+---------+   +---------+----------+
         |                        |
         +----------+-------------+
                    v
           +--------+--------+
           | RTA Monitor     |
           | (certifiable    |
           |  switching)     |
           +---------+-------+
                     v
              Actuator Commands
```

If the complex function exceeds safe boundaries, RTA switches to the certified fallback. This is analogous to the safety island / safety bag pattern.

#### PX4/ArduPilot vs Certified Systems

| Aspect             | PX4/ArduPilot       | Certified FCS                    |
|--------------------|---------------------|----------------------------------|
| Software assurance | None                | DO-178C DAL A                    |
| Hardware           | COTS STM32F4/H7     | DO-254 certified boards          |
| Redundancy         | Single FCU typical  | Triple-dissimilar                |
| RTOS               | NuttX (uncertified) | VxWorks 653, INTEGRITY-178       |
| WCET analysis      | None                | Formal WCET + runtime monitoring |

#### eVTOL Safety Patterns

- Distributed Electric Propulsion (6-18+ motors): inherent propulsion redundancy
- Triple-dissimilar flight control systems
- Battery management: multiple independent packs with isolation
- Novel challenge: hover-to-cruise mode transition safety

### 4.6 Multi-Core Certification (CAST-32A / AMC 20-193)

The fundamental challenge: shared resources (cache, memory bus, interconnect) create **interference channels** where one core's activity unpredictably affects another's timing.

Mitigation strategies:
1. **Core partitioning**: Assign cores to functions, disable shared cache
2. **Bandwidth budgeting**: Allocate memory bandwidth per core
3. **Interference stress testing**: Synthetic worst-case traffic to bound WCET under contention
4. **Runtime monitoring**: Hardware performance counters detect exceeded budgets

INTEGRITY-178 tuMP was first RTOS to achieve multicore DAL A with CAST-32A compliance.

---

## 5. Cross-Domain Architectural Patterns

### 5.1 Redundancy Architectures

| Architecture                 | Safety    | Availability | Domains                        | Example                  |
|------------------------------|-----------|--------------|--------------------------------|--------------------------|
| 1oo1D (single + diagnostics) | Moderate  | Low          | Auto (ASIL B)                  | Single MCU with watchdog |
| 1oo2 (dual, both must agree) | High      | Low          | Auto (ASIL D), Rail (SIL 3)    | Lockstep MCU             |
| 2oo2 (dual cross-check)      | High      | Low          | Rail (SIL 4), Aviation (DAL A) | Command-monitor          |
| 2oo3 (triple majority vote)  | Very High | High         | Rail (SIL 4), Aviation (DAL A) | SIMIS W, B777 PFC        |
| N-version (diverse SW)       | Very High | Varies       | Rail (SIL 4), Aviation (DAL A) | MicroLok II, B777 lanes  |
| Hot standby                  | High      | Very High    | Rail (SIL 4)                   | EBI Lock 950             |

### 5.2 Safety Monitor / Safety Bag / RTA

All three domains use the same pattern under different names:

| Domain     | Name                     | Role                                           |
|------------|--------------------------|------------------------------------------------|
| Automotive | Safety Island            | Independent MCU monitors main system           |
| Railway    | Safety Bag               | Simple invariant checker blocks unsafe outputs |
| Aviation   | Run-Time Assurance (RTA) | Certified monitor switches to safe controller  |

Common properties:
- **Simpler than the main system** (auditable, formally verifiable)
- **Can only block, never command** (except RTA which can switch to backup)
- **Independent hardware** (separate failure domain)
- **Higher integrity level** than the main system it monitors

This is exactly the Autoware safety island architecture: main stack runs at QM, safety island runs at ASIL D.

### 5.3 Partitioning (Spatial + Temporal)

All domains converge on the same partitioning concept:

| Domain     | Standard        | Mechanism                                     |
|------------|-----------------|-----------------------------------------------|
| Automotive | AUTOSAR SC3/SC4 | MPU-based OS-Applications + timing protection |
| Railway    | PikeOS SIL 4    | Hypervisor partition + cyclic schedule        |
| Aviation   | ARINC 653       | MMU-based partitions + fixed time windows     |

Key guarantee: a non-safety partition **cannot** affect a safety partition, neither spatially (memory corruption) nor temporally (CPU starvation).

### 5.4 Graceful Degradation

All domains follow a degradation hierarchy:

```
Level 0: Full automatic (all systems nominal)
Level 1: Degraded automatic (non-critical failure, system continues)
Level 2: Supervised operation (safety backup active, reduced capability)
Level 3: Restricted operation (basic safety only)
Level 4: Manual/safe state (all automation failed, human control or stop)
```

Automotive: ASIL D -> degraded ADAS -> driver takeover -> emergency stop
Railway: Full ATP -> Level 2 -> Level 1 fallback -> staff responsible
Aviation: Normal law -> alternate law -> direct law -> mechanical backup

### 5.5 Black Channel Communication

All domains independently arrived at the "black channel" concept:

- **Railway** (EN 50159): Explicit black channel standard, CRC + sequence + timestamp + auth
- **Automotive** (AUTOSAR E2E): End-to-end protection profiles (P01-P07) over any bus
- **Aviation** (ARINC 653 ports): Sampling/queuing ports with bounded latency + network integrity

The underlying transport (Ethernet, CAN, zenoh) is treated as untrusted. Safety properties are ensured by the protocol layer above.

### 5.6 Formal Methods Adoption

| Domain     | Method                  | Tool              | Usage                                                 |
|------------|-------------------------|-------------------|-------------------------------------------------------|
| Railway    | B formal method         | Atelier B         | CLEARSY SIL 4 interlocking (proof eliminates testing) |
| Railway    | SCADE                   | Esterel/Ansaldo   | Control algorithms, state machines                    |
| Aviation   | DO-333 supplement       | Various           | Formal methods as DO-178C credit                      |
| Aviation   | Model checking          | CBMC, SPIN        | Exhaustive state exploration                          |
| Automotive | Abstract interpretation | Polyspace, Astree | Prove absence of runtime errors                       |
| All        | SPARK/Ada               | AdaCore           | Safe language with built-in contracts                 |

The CLEARSY Safety Platform is the strongest example: mathematical proof completely replaces unit and integration testing for SIL 4 interlocking software.

---

## 6. Certified RTOS Landscape

### Comparison Matrix

| RTOS              | Vendor           | Auto        | Rail      | Aviation | Architecture                      |
|-------------------|------------------|-------------|-----------|----------|-----------------------------------|
| **PikeOS**        | SYSGO            | ASIL D      | **SIL 4** | DAL A    | Separation kernel + hypervisor    |
| **VxWorks 653**   | Wind River       | --          | --        | DAL A    | ARINC 653 partitioning            |
| **INTEGRITY-178** | Green Hills      | --          | --        | DAL A    | Separation kernel, EAL6+ security |
| **LynxOS-178**    | Lynx Software    | --          | --        | DAL A    | Native POSIX, FAA RSC             |
| **Deos**          | DDC-I            | --          | --        | DAL A    | ARINC 653, DAL A crypto           |
| **QNX**           | BlackBerry       | ASIL D      | --        | --       | Microkernel + hypervisor          |
| **SafeRTOS**      | WITTENSTEIN      | ASIL D      | SIL 3     | DAL C    | Minimal certifiable FreeRTOS      |
| **Zephyr**        | Linux Foundation | In progress | --        | --       | RTOS with IEC 61508 cert effort   |

### PikeOS: The Cross-Domain Champion

PikeOS uniquely spans all three domains with a single kernel:

- **Railway SIL 4** (EN 50128): First RTOS + hypervisor certified SIL 4
- **Automotive ASIL D** (ISO 26262): Certified by TUV SUD
- **Aviation DAL A** (DO-178C): Full certification kit
- **Space** (ECSS): Used in ESA IMA4Space project
- **Security**: Common Criteria EAL 5+

Architecture: separation microkernel hosting multiple guest OS personalities (Linux, POSIX, AUTOSAR, ARINC 653) in isolated partitions. Each partition has independently certified integrity level.

This makes PikeOS the most relevant RTOS for a nano-ros safety platform that needs to operate across domains.

---

## 7. Hardware Safety Mechanisms

### 7.1 Lockstep Cores

All domains use lockstep for hardware fault detection:

```
+------------------+    +------------------+
|  Primary Core    |    |  Checker Core    |
|  (executes)      |    |  (delayed N cyc) |
+--------+---------+    +--------+---------+
         |                       |
         +----------+------------+
                    v
             +-----------+
             | Comparator|  Every clock cycle
             +-----------+
                    |
         Match: continue    Mismatch: fault -> reset/safe state
```

- **DCLS** (Dual-Core Lock-Step): Standard in automotive (AURIX, TMS570, S32K3)
- **TCLS** (Triple-Core Lock-Step): ARM evolution of DCLS, adding third core for higher dependability

The checker core runs with a configurable cycle delay (typically 2 cycles), providing protection against transient faults that could hit both cores simultaneously.

### 7.2 ECC Memory

| Type   | Capability                                | Domain                     |
|--------|-------------------------------------------|----------------------------|
| SECDED | Single-Error Correct, Double-Error Detect | All (minimum for SIL 3+)   |
| DECTED | Double-Error Correct, Triple-Error Detect | Rail SIL 4, Aviation DAL A |

Applied to: SRAM, Flash, cache (L1/L2), register files, bus interconnects.

### 7.3 Safety Management Units

Dedicated hardware that aggregates all fault signals:

- **Infineon SMU** (Safety Management Unit): AURIX TC3xx
- **NXP FCCU** (Fault Collection and Control Unit): S32K3, S32G
- **TI ECM** (Error Correcting Module): TMS570

These units can autonomously trigger safe state (reset, output disable) even if the CPU is hung.

---

## 8. Deterministic Communication

### 8.1 Protocol Comparison

| Protocol                | Domain          | Determinism            | Speed         | Safety Level      |
|-------------------------|-----------------|------------------------|---------------|-------------------|
| **AFDX** (ARINC 664)    | Aviation        | Virtual Links with BAG | 100Mbps-1Gbps | DAL A systems     |
| **TTEthernet** (AS6802) | Aviation/Space  | Sub-us sync            | 1 Gbps        | NASA Orion        |
| **CAN FD**              | Automotive      | Priority-based         | 8 Mbps        | ASIL D            |
| **FlexRay**             | Automotive      | TDMA static segment    | 10 Mbps       | ASIL D            |
| **Ethernet TSN**        | Auto/Industrial | Scheduled traffic      | 1-10 Gbps     | ASIL D (emerging) |
| **Euroradio**           | Railway         | Timestamped + MAC      | GSM-R speed   | SIL 4             |
| **SAFEbus**             | Aviation (B777) | Static TDMA backplane  | ~30 Mbps      | DAL A             |

### 8.2 Time-Triggered vs Event-Triggered

**Time-triggered** (TTA, SAFEbus, FlexRay static):
- All communication at predefined times from global clock
- Deterministic, no arbitration, no collisions
- Fault containment: failed node cannot disrupt others
- Higher overhead (unused slots wasted)

**Event-triggered** (CAN, Ethernet):
- Communication on demand
- Priority-based arbitration
- Better bandwidth utilization
- Less deterministic (priority inversion possible)

**Hybrid** (FlexRay, TTEthernet):
- Static segment for safety-critical (time-triggered)
- Dynamic segment for best-effort (event-triggered)
- Best of both worlds

### 8.3 AFDX Virtual Links

Used in Airbus A380/A350, Boeing 787:

```
Virtual Link = source -> one or more destinations
BAG (Bandwidth Allocation Gap) = minimum frame interval
    Values: 1, 2, 4, 8, 16, 32, 64, 128 ms
Lmax = maximum frame size (64-1518 bytes)
Bandwidth = Lmax / BAG

Dual redundant networks (A + B) for fault tolerance
Traffic policing at ingress enforces BAG and Lmax per VL
```

Guaranteed bounded end-to-end latency for each VL.

---

## 9. Implications for nano-ros & Autoware

### 9.1 Architecture Validation

The safety island architecture proposed in `autoware-safety-island-architecture.md` aligns with cross-domain best practices:

| Pattern              | Industry Practice        | nano-ros Implementation                           |
|----------------------|--------------------------|---------------------------------------------------|
| Safety monitor/bag   | Rail SIL 4, Aviation RTA | Safety island with watchdog + command gate        |
| Physical isolation   | All domains              | Separate MCU (STM32F4)                            |
| Temporal determinism | ARINC 653, AUTOSAR SC4   | RTIC compile-time scheduling                      |
| Black channel        | EN 50159, AUTOSAR E2E    | zenoh as untrusted transport                      |
| Formal verification  | CLEARSY B method, DO-333 | Verus + Kani proofs                               |
| Graceful degradation | All domains              | MRM hierarchy: comfortable stop -> emergency stop |

### 9.2 What nano-ros Should Adopt

Based on this study, the following patterns are most relevant:

#### High Priority

1. **End-to-End Safety Protocol** (EN 50159 / AUTOSAR E2E inspiration)
   - Add CRC, sequence number, timestamp to safety-critical messages over zenoh
   - Detect: corruption, repetition, loss, delay, masquerade
   - zenoh is the "black channel" — safety must not depend on it

2. **Watchdog Supervision** (AUTOSAR WdgM pattern)
   - Alive supervision: periodic checkpoint in main loop
   - Deadline supervision: bounded execution time for safety tasks
   - Logical supervision: execution flow must follow expected sequence
   - External watchdog IC (e.g., challenge-response) for MCU-level monitoring

3. **Safety Bag Invariants** (Railway pattern)
   - Define simple, formally verifiable safety rules for command gate:
     - Acceleration within bounds
     - Steering rate within bounds
     - Velocity within bounds
     - No command if heartbeat expired
   - These invariants are Verus-provable

#### Medium Priority

4. **Deterministic Scheduling** (ARINC 653 inspiration)
   - RTIC already provides static priority scheduling
   - Consider adding partition-like time budgets for different safety functions
   - Monitor actual execution time vs budgeted (health monitoring)

5. **Command-Monitor Architecture** (Aviation pattern)
   - If two safety islands are used, one computes (command) and other verifies (monitor)
   - Mismatch triggers safe state
   - Already proposed in the dual-island architecture

6. **Diverse Redundancy** (B777 / Railway N-version)
   - If two islands used, consider different MCU families (e.g., STM32F4 + RP2040)
   - Different compilers or compiler versions per island
   - Same specification, different implementation

#### Lower Priority (Future)

7. **Coded Processor Technique** (Railway VCP)
   - Arithmetic safety codes on safety-critical data
   - Detects hardware faults at data level without redundant hardware
   - Complex to implement but powerful for single-MCU safety

8. **Formal Safety Case** (GSN / UL 4600)
   - Structured argument linking hazards -> requirements -> evidence
   - Required for any real certification effort

### 9.3 Platform Recommendations for Autoware Safety Island

Based on the cross-domain survey:

#### MCU Selection

| MCU                      | Pros                                         | Cons                                         | Best For                 |
|--------------------------|----------------------------------------------|----------------------------------------------|--------------------------|
| **TI TMS570**            | IEC 61508 SIL 3 certified, lockstep          | Older core (Cortex-R4/R5), smaller ecosystem | Highest safety assurance |
| **Infineon AURIX TC3xx** | ASIL D certified, 6 cores, rich peripherals  | Complex, TriCore (not ARM), expensive        | Automotive production    |
| **STM32H7**              | Powerful (480 MHz Cortex-M7), FPU, large RAM | No lockstep, no safety certification         | Development/prototype    |
| **STM32F4**              | Proven, nano-ros BSP exists, low cost        | No lockstep, limited RAM                     | Current development      |
| **NXP S32K3**            | ASIL D, lockstep Cortex-M7, AUTOSAR support  | Newer, smaller community                     | Production path          |

For development: continue with STM32F4 (proven, BSP exists).
For production: TMS570 (pre-certified SIL 3) or S32K3 (ASIL D, ARM ecosystem).

#### RTOS Selection

| RTOS         | Certification                      | nano-ros Compatibility | Recommendation          |
|--------------|------------------------------------|------------------------|-------------------------|
| **RTIC**     | None (but compile-time guarantees) | Excellent (current)    | Development + prototype |
| **Zephyr**   | IEC 61508 in progress              | Good (BSP exists)      | Near-term production    |
| **PikeOS**   | SIL 4 + ASIL D + DAL A             | Unknown (proprietary)  | Cross-domain production |
| **SafeRTOS** | ASIL D + SIL 3                     | Feasible (simple API)  | Automotive production   |

### 9.4 Safety Protocol Design Sketch

Based on EN 50159 + AUTOSAR E2E, a minimal safety protocol for nano-ros:

```rust
/// Safety-wrapped message with E2E protection
struct SafeMessage<M> {
    /// Monotonic sequence counter (detects loss, repetition, resequencing)
    sequence: u32,
    /// Sender timestamp in milliseconds (detects delay)
    timestamp_ms: u32,
    /// Source identifier (detects masquerade)
    source_id: u16,
    /// The actual payload
    payload: M,
    /// CRC-32 over all above fields (detects corruption)
    crc: u32,
}

/// Receiver-side validation
fn validate<M>(msg: &SafeMessage<M>, expected_seq: u32, max_age_ms: u32) -> Result<&M, SafetyError> {
    // 1. CRC check (corruption)
    verify_crc(msg)?;
    // 2. Sequence check (loss, repetition, resequencing)
    if msg.sequence != expected_seq { return Err(SafetyError::SequenceMismatch); }
    // 3. Freshness check (delay)
    if age(msg.timestamp_ms) > max_age_ms { return Err(SafetyError::MessageTooOld); }
    // 4. Source check (masquerade)
    verify_source(msg.source_id)?;
    Ok(&msg.payload)
}
```

This is implementable in `no_std` and could be a nano-ros-safety crate.

---

## References

### Standards
- IEC 61508: Functional safety of E/E/PE safety-related systems
- ISO 26262: Road vehicles — Functional safety
- EN 50126/50128/50129: Railway applications — CENELEC safety standards
- EN 50159: Railway safety-related communication
- DO-178C: Software Considerations in Airborne Systems
- DO-254: Design Assurance for Airborne Electronic Hardware
- DO-333: Formal Methods Supplement to DO-178C
- ARP 4754A: System Development Process
- ARP 4761: Safety Assessment Process
- ARINC 653: Avionics Application Standard Software Interface
- ARINC 664 Part 7: AFDX (Avionics Full-Duplex Switched Ethernet)
- ASTM F3269: Standard Practice for Methods to Safely Bound Flight Behavior
- JARUS SORA v2.5: Specific Operations Risk Assessment

### Platforms & Products
- Infineon AURIX TC3xx: https://www.infineon.com/aurix
- TI TMS570: https://www.ti.com/microcontrollers-mcus-processors/safety-mcus
- NXP S32K3: https://www.nxp.com/s32k
- Siemens SIMIS W: https://www.siemens.com/mobility
- Alstom Smartlock: https://www.alstom.com/smartlock-range
- CLEARSY Safety Platform: https://www.clearsy.com/en/tools/clearsy-safety-platform/
- PikeOS: https://www.sysgo.com/products/pikeos
- VxWorks 653: https://www.windriver.com/products/vxworks
- INTEGRITY-178: https://www.ghs.com/products/safety_critical/integrity_178_tump.html
- LynxOS-178: https://www.lynx.com/products/lynxos-178
- Deos: https://www.ddci.com/solutions/products/deos/
- QNX Hypervisor: https://blackberry.qnx.com/en/products/hypervisor
