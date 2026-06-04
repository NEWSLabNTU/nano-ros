# AI Drone FOV / Perception Research (2025-2026)

Caveman prose. Working systems only — shipping, deployed, or validated research stacks. No vaporware.
Sources cited inline. Where official spec sheets dodge a number, marked `[unclear]`.

---

## 1. Consumer / Prosumer Autonomy

### Skydio 2+ (2022, still sold as baseline reference)
- 6× fisheye color nav cams, each 200° FOV, 3-up / 3-down arrangement → full omnidirectional [src](https://medium.com/skydio/inside-the-mind-of-the-skydio-2-b1b78aa6dfa7)
- Compute: NVIDIA Jetson TX2 (predecessor pattern; not Orin)
- AI: subject tracking, GPS-denied VIO, obstacle dodge mid-tracking, return-to-home with obstacle nav
- Endurance: ~27 min. The canonical "6× fisheye omni" pattern originates here.
- src: [DPReview](https://www.dpreview.com/reviews/skydio-2-review-tracking-tech-wows-but-image-quality-disappoints)

### Skydio X10 (2023, shipping)
- 6× custom nav cams, **trinocular top + trinocular bottom**. Samsung 1/2.8" 32 MP CMOS, f/1.8, **200° diagonal FOV each**. Obstacle sensing 20 m, **true 360°** [src](https://www.skydio.com/x10/technical-specs)
- Modular payload: VT300-Z (64 MP narrow 50° + 190 mm tele 13° + 640×512 thermal 41°) or VT300-L (50 MP wide 93°). Mechanical gimbal; no LiDAR.
- Compute: **NVIDIA Jetson Orin** + Qualcomm Snapdragon 865. "Up to 100 TOPS" claimed for X10+Dock combo [src](https://www.skydio.com/blog/dock-for-x10-future-scalable-autonomous-flight-data-capture)
- AI: real-time subject tracking + reacquisition, on-board 2D/3D reconstruction, deep-learning at edge, autonomous scan planning
- 2.11 kg, 35 min hover / 40 min flight, max 16 m/s under standard OA
- src: [Skydio X10](https://www.skydio.com/x10), [UAV Coach](https://uavcoach.com/skydio-x10/)

### DJI Mavic 3 Pro (2023)
- **APAS 5.0**: 6 fisheye + 2 wide-angle vision sensors + bottom IR ToF [src](https://www.dji.com/mavic-3-pro/specs)
- Per-axis ranges (measurement / detection / FOV H×V):
  - Fwd/Back: 0.5–20 m / 0.5–200 m / 90°×103°
  - L/R: 0.5–16 m / – / 90°×103°
  - Up: 0.5–25 m / – / 90°×85°
  - Down: 0.3–18 m / – / 130°×160°
- Compute: undisclosed DJI ASIC + ARM SoC `[unclear]`
- AI: ActiveTrack 360, MasterShots, waypoint flight; no LiDAR

### DJI Air 3S (2024)
- **First DJI consumer with forward-facing LiDAR**: 0.5–25 m range, 60°×60° FOV [src](https://www.dji.com/air-3s/specs)
- 6 vision sensors (2 fwd, 2 back, 2 down) + downward IR ToF; omnidirectional + LiDAR-augmented night flight
- APAS 5.0; range/FOV per-axis comparable to Mavic 3 Pro
- src: [UAV Coach](https://uavcoach.com/air-3s/), [Engadget review](https://www.engadget.com/cameras/dji-air-3s-review-lidar-and-improved-image-quality-make-for-a-nearly-faultless-drone-130002876.html)

### Parrot Anafi AI (2021, still shipping)
- Unusual: **two cameras on gimbal** that rotate fwd/up/down, sweeping **311° vertical × 110° horizontal** [src](https://www.parrot.com/en/drones/anafi-ai/technical-documentation/technical-specifications)
- Marketed as 360° obstacle detection but is sweep-based, not 6-cam static — measured-vs-marketed gap
- 48 MP Quad Bayer main; 4G LTE link (first consumer)
- AI: PhotogrammetryKit on-board, Air SDK Python
- src: [IEEE Spectrum](https://spectrum.ieee.org/parrot-announces-anafi-ai-a-buginspired-4g-drone)

### Autel EVO Max 4T (2023, V2 2025)
- **Dual-fisheye vision + 60 GHz mmWave radar fusion**. Marketed "720° omnidirectional" (i.e. dual hemispheres) [src](https://www.autelrobotics.com/productdetail/evo-max-4t/)
- Ranges: Fwd/Back 0.3–50 m, sides 0.5–26 m, up 0.2–26 m, down 0.15–80 m (radar)
- FOV: fwd/back 120°×80°; up 180° (lateral)
- 4-sensor payload: wide + tele + thermal + laser rangefinder
- Compute: Autel "Autonomy Engine" — vendor doesn't disclose SoC `[unclear]`. Detects objects to 0.5 inch (marketing claim, not validated)

---

## 2. Defense / Military

### Anduril Bolt-M (2024, deployed)
- ML-guided strike drone. **Onboard vision + terminal guidance survives loss-of-link** [src](https://www.anduril.com/bolt)
- Camera array undisclosed at unit level (export-controlled) `[unclear]`
- Lattice mesh networking; multi-source sensor fusion off-board
- 5.4 kg, 45 min, 20 km range
- src: [TWZ Inside Bolt-M](https://www.twz.com/uncategorized/inside-andurils-bolt-m-kamikaze-drone-program), [Defense Post](https://thedefensepost.com/2024/10/11/anduril-bolt-attack-drone/)

### Anduril Anvil + Anvil-M (deployed, USMC + USNORTHCOM)
- Counter-UAS kinetic interceptor (quadrotor). Group 1/2 threats
- "Advanced onboard computing and sensors" for terminal guidance `[unclear]` — no public camera/FOV specs
- Cued by Lattice off-board (Heimdal trailer: thermal + radar)
- src: [Anduril Anvil](https://www.anduril.com/anvil), [Army Recognition](https://www.armyrecognition.com/news/army-news/2025/u-s-usnorthcom-tests-anduril-counter-drone-system-to-defend-u-s-bases-from-drone-threats)

### Anduril ALTIUS-600M / 700M (deployed)
- Tube-launched. **Modular nosecone**: ISR, SIGINT, EW, comms-relay, or warhead [src](https://www.anduril.com/hardware/altius/)
- Coordinated multi-asset autonomy via Lattice (one operator → many drones)
- 600M: ~160 km, ~2 h with warhead; 700M: heavier (≤35 lb warhead), up to 440 km ISR
- Sensor mix payload-dependent; no fixed cam suite
- src: [Army Recognition](https://www.armyrecognition.com/news/aerospace-news/2025/u-s-anduril-altius-600m-and-700m-loitering-munitions-unify-reconnaissance-and-strike-in-one-munition)

### Anduril Roadrunner-M (deployed 2024)
- Twin-turbojet **reusable VTOL CUAS interceptor**. High-subsonic
- "Onboard sensors + onboard processing" autonomously find target + plan intercept `[unclear]` — exact sensor stack classified
- Lattice C2; one operator supervises squadrons
- src: [Anduril](https://www.anduril.com/roadrunner), [DefenseScoop](https://defensescoop.com/2023/12/01/anduril-develops-new-roadrunner-drones-that-it-says-can-perform-air-defense-missions/)

### Shield AI V-BAT (Block upgrade 2025, MQ-35)
- VTOL fixed-wing, 3.8 m wingspan, 73 kg MTOW, 40 lb payload, **13+ h endurance** [src](https://shield.ai/v-bat/)
- Sensors: gyro-stabilized EO/IR gimbal w/ continuous zoom + GPS/INS; optional SAR; **integrated ViDAR camera array** (Sentient) for wide-area maritime/land
- Compute: undisclosed; runs **Hivemind** autonomy
- GPS-denied + comms-denied operation; SATCOM BLOS in Block
- src: [PR Newswire](https://www.prnewswire.com/news-releases/shield-ai-unveils-v-bat-block-upgrade-powered-by-hivemind-advanced-autonomy-satcom-and-heavy-fuel-engine-among-new-features-302421755.html)

### Shield AI Nova 2 (deployed, indoor/subterranean)
- Small quad. **3D detection/tracking/mapping in real time**, GPS-denied
- Sensor mix: stereo + ToF + IMU (Hivemind perception stack — exact cams not public) `[unclear]`
- Runs same Hivemind autopilot scaled down to V-BAT / F-16 / MQM-178
- src: [Shield AI Nova 2 blog](https://shield.ai/autonomy-for-the-world-indoor-exploration-with-nova-2/)

### Skydio X10D (2024, defense variant of X10)
- Same 6-cam, 200° FOV omni nav as X10. **AES-256 + secure boot + signed firmware**, NDAA compliant [src](https://www.skydio.com/x10d)
- 4K60P HDR cams (×6) + 16× digital zoom + 360° Superzoom; FLIR Boson 320 thermal on 180° gimbal
- 35 min, 10 km link
- AI: omni avoidance, real-time 3D mapping, object/scene recognition, autonomous motion planning
- src: [Army-technology](https://www.army-technology.com/projects/skydio-x2d-reconnaissance-drone/)

### AeroVironment Switchblade 300 Block 20 / Switchblade 600 (deployed; Ukraine combat-validated)
- 300 Block 20: 20+ min endurance, 30 km range. **EO/IR panning camera suite** + dual fwd/side EO + IR nose cam [src](https://www.avinc.com/solution/switchblade-300-block-20/)
- 600: 40 min, 40+ km, anti-armor warhead, "class-leading high-res EO/IR"
- AI: "Patrol mode" autonomous targeting (300 BL20); operator commit-and-PID
- src: [AvInc 600](https://www.avinc.com/lms/switchblade-600), [Wikipedia](https://en.wikipedia.org/wiki/AeroVironment_Switchblade)

### AeroVironment JUMP 20 (deployed, US Army FTUAS)
- Group 3 VTOL fixed-wing. 5.7 m wingspan, 97.5 kg MTOW, **13+ h endurance, 185 km range**, 17 kft alt [src](https://www.avinc.com/solution/jump-20/)
- 13.6 kg modular payload bay: EO/MWIR, HSOR, SIGINT, comms relay
- Onboard tracking + stabilization + GPS-denied launch/nav/landing (incl. moving deck recovery on JUMP 20-X)
- src: [Wikipedia T-20](https://en.wikipedia.org/wiki/AeroVironment_T-20), [Defense Post 20-X](https://thedefensepost.com/2025/02/19/aerovironment-jump-20x-uas/)

### Teal Drones Black Widow (Red Cat; 2024 US Army SRR pick, 2025 NATO NSPA catalog)
- <3 lb sUAS. **Teledyne FLIR Hadron 640R+** payload: 64 MP EO (67° H FOV) + 640×512 radiometric IR (32° FOV) [src](https://dronelife.com/2024/11/20/teledyne-flir-selected-for-red-cats-black-widow-us-army-srr-program/)
- 45+ min endurance, 8 km link
- Modular: third-party AI/CV apps for 3D map, target acquisition, decision support. Anti-jam
- Specific obstacle-avoidance cam suite **not publicly disclosed** `[unclear]` — Black Widow targets ISR, not collision-rich autonomous flight
- src: [Cratos NZ](https://www.cratos.co.nz/teal-black-widow/), [Calibre Defence](https://www.calibredefence.co.uk/dsei-2025-red-cats-black-widow-reconnaissance-drone/)

### Quantum Systems Vector AI (2025, US Army CL sUAS-DR2 selection)
- eVTOL fixed-wing, 2.8 m span, 9.5 kg MTOW
- **Dual NVIDIA Jetson Orin SoMs** for real-time object detection / classification / tracking [src](https://quantum-systems.com/us/vector-ai/)
- VIO for GPS-denied; Silvus MANET radio
- MOSA Ethernet payload bay → swappable ISR/SIGINT/EW
- Cams: gimbal + fixed array (specific models payload-dependent) `[unclear]`
- src: [QS press](https://quantum-systems.com/blog/2025/03/24/quantum-systems-unveils-vector-ai/)

### Edge Autonomy VXE30 Stalker (deployed, SOCOM)
- 22 kg VTOL. **8 h battery / 12+ h SOFC endurance**, 160 km link [src](https://edgeautonomy.io/uncrewed-systems/vxe30-stalker/)
- EO/IR payloads (Octopus ISR gimbals, HD→4K + WIR/LWIR)
- Field demo with **Sentient ViDAR** AI-enabled wide-area optical sensor → small-moving-target indication
- src: [Army Recognition Stalker](https://www.armyrecognition.com/news/aerospace-news/2025/uss-vxe30-stalker-vtol-drone-from-edge-autonomy-shown-at-landeuro-2025-elevates-tactical-recon-missions-worldwide), [Edge ViDAR demo](https://edgeautonomy.io/sentients-ai-enabled-vidar-optical-sensors-soar-on-edge-autonomys-vxe30-stalker-uas-in-successful-live-demonstrations/)

### Edge Autonomy Penguin C VTOL Mk2 (deployed)
- 4.1 m span, 42 kg MTOW, 4.5 kg payload, 13 kft, 70 kt max
- Same Octopus gimbal family as Stalker
- src: [EDR Magazine](https://www.edrmagazine.eu/defea-2025-edge-autonomy-penguin-c-vtol-adding-aerodynamics-to-vertical-operations-capability)

---

## 3. Industrial Inspection / Mapping

### DJI Matrice 350 RTK + Zenmuse L2 (2023, shipping)
- **6-directional binocular vision + IR**: fwd/back/L/R 0.7–40 m; up/down 0.6–30 m [src](https://enterprise.dji.com/matrice-350-rtk/specs)
- FOV: fwd/back/down 65°×50°; L/R/up 75°×60°
- **Optional CSM Radar**: 360° horizontal upward, 30 m
- **Zenmuse L2 LiDAR payload**: 250 m @10% reflectivity / 450 m @50%; max 500 m. Repetitive scan 70°×3°, non-repetitive 70°×75°. 240k pts/s, 5-return penetration, 5/4 cm H/V accuracy @150 m [src](https://enterprise.dji.com/zenmuse-l2/specs)
- M350 is the **canonical solid-state-LiDAR + nav-camera survey rig**

### Skydio Dock for X10 (2024, shipping)
- Persistent docked autonomy. **Visual fiducial precision landing** (driven by X10's six 200° nav cams)
- Up to 100 TOPS edge inference; remote-pilot from anywhere
- HVAC, wind to 160 mph survival / 28 mph operational, −20→50 °C [src](https://www.skydio.com/dock)
- Used for substation inspection, infra patrol, DFR (drone-as-first-responder)
- src: [UST coverage](https://www.unmannedsystemstechnology.com/2024/10/dock-released-for-autonomous-uav-flight-data-capture/)

### ModalAI Starling 2 / Starling 2 Max (2023→, NDAA, shipping)
- **VOXL 2 brain**: Qualcomm QRB5165, 8 cores, **15+ TOPS**, 7 concurrent cameras, integrated PX4 on DSP, 5G [src](https://www.modalai.com/products/voxl-2)
- Starling 2: 3× AR0144 fisheye stereo pairs + 1× IMX412 color + **PMD ToF** + optional FLIR Lepton
- Starling 2 Max: dual IMX412 + dual AR0144, NDAA + GPS-denied outdoor [src](https://www.modalai.com/products/starling-2-max)
- TDK ICM-42688 IMU + ICP-10111 baro on DSP — tight VIO loop
- Pattern: **ToF + RGB hybrid + multi-fisheye** + Hexagon NPU on Snapdragon-class SoC
- src: [BusinessWire VOXL 2 Starling](https://www.businesswire.com/news/home/20230719952178/en/ModalAI-Launches-Even-Smaller-Smarter-and-Safer-Development-Drone-VOXL-2-Starling)

---

## 4. Research / Autonomy Stacks

### MIT FlightGoggles (active, Unity3D + ROS HITL)
- Photorealistic exteroceptive sim → real drone in motion-capture cage, virtual obstacles. Lets agile-flight researchers skip aero / motor / battery modeling [src](https://flightgoggles.mit.edu/learn)
- Used to gate AlphaPilot drone-racing finalists
- Backbone for sensor-selection studies (event cam, RGB, depth)
- src: [arXiv 1905.11377](https://arxiv.org/pdf/1905.11377), [FlightGoggles MIT](https://flightgoggles.mit.edu/research)

### UZH RPG (Scaramuzza Lab) — Event Camera Drones
- **DAVIS sensor**: event pixels embedded in std RGB pixel array + synchronized IMU. <few-ms reaction → autonomous ball-dodge [src](https://spectrum.ieee.org/drone-with-event-camera-takes-first-autonomous-flight)
- 2023-2024: **Swift** event-aware system beat world-champion FPV pilots head-to-head in drone racing
- Domain: event cameras = the research-grade FOV pattern. Latency + motion-blur robustness > frame-based
- src: [RPG UZH](https://rpg.ifi.uzh.ch/), [Unmanned Airspace event cams](https://www.unmannedairspace.info/latest-news-and-information/zurich-research-group-use-event-cameras-to-provide-reliable-detect-and-avoid-capability/)

### PX4 + ROS 2 Stacks (open-source, broad deployment)
- PX4 firmware + ROS 2 micro-XRCE-DDS bridge → companion-computer perception. Defactor stack for university & startup autonomy
- Common shape: Pixhawk / Cube flight controller + Jetson/RPi/NVIDIA Orin Nano companion + RealSense or stereo cam
- src: [PX4 docs](https://docs.px4.io/main/en/companion_computer/auterion_skynode)

### Auterion Skynode / Skynode X (deployed via Auterion OS)
- **All-in-one**: flight controller + mission computer + video stream + 4G/5G + networking, single device [src](https://auterion.com/product/skynode-x/)
- Runs hardened PX4 + Linux mission OS + **ROS 2 in Docker**
- Auterion OS basis for several US DoD swarming programs (Auterion 2025 swarming tech launch)
- Camera/perception sensor selection is **vehicle-integrator's choice**; Skynode is the brain not the eyes
- src: [Auterion ROS 2](https://auterion.com/auterion-driving-ros-2-adoption-for-flying-robots/), [Tectonic Defense swarming](https://www.tectonicdefense.com/auterion-launches-new-drone-swarming-technology/)

### ModalAI VOXL 2 / VOXL 2 Mini (open-stack, fielded research)
- Same QRB5165 as Starling line. **Companion-computer-as-flight-controller** (PX4 on Hexagon DSP). 16 g, 15+ TOPS
- Reference platform for PX4 + ROS 2 + GPS-denied research; used in DARPA programs
- src: [PX4 VOXL 2 doc](https://docs.px4.io/main/en/flight_controller/modalai_voxl_2.html)

---

## Cross-Cutting FOV Patterns

| Pattern | Canonical example | Notes |
|---|---|---|
| **6× fisheye omnidirectional** (3-up / 3-down, ~200° each) | Skydio 2+, X10, X10D | True hemispherical-pair coverage; static; lets onboard VIO + neural avoidance run continuously |
| **Fwd+down stereo pairs + side fisheyes** | DJI Mavic 3 Pro / Air 3S / M350 (APAS) | 6-direction binocular; cheaper than Skydio omni, narrower per-cam FOV (90°×103° typ), IR ToF for landing |
| **ToF + RGB hybrid + fisheye stereo** | ModalAI Starling 2 (PMD ToF + AR0144 stereo + IMX412 RGB) | Research/GPS-denied; ToF closes near-field gap that vision-only misses in low texture |
| **Solid-state LiDAR + cameras** | DJI Matrice 350 + Zenmuse L2; DJI Air 3S (fwd LiDAR for nav) | Mapping-grade survey + night-flight obstacle nav; L2 = Livox-derived Risley-prism non-repetitive |
| **mmWave radar + dual fisheye** | Autel EVO Max 4T (60 GHz) | All-weather, rain/fog robust; dense fusion uncommon in consumer |
| **Sweep-gimbal stereo** | Parrot Anafi AI (2 cams sweep 311°×110°) | Vertically wide but temporally sampled — not a true static omni |
| **Event camera (DVS)** | UZH Swift / DAVIS-equipped quads | Microsecond-latency change events; research, not commercial yet |
| **Wide-area scanning EO (ViDAR)** | Edge Autonomy Stalker + Sentient ViDAR; Shield AI V-BAT integrated | Maritime / persistent ISR scanning; AI-driven small-target detection across very wide FOV |
| **Multi-source off-board fusion** | Anduril Lattice (Anvil, Bolt-M, Roadrunner, ALTIUS) | On-board sensors classified per-platform; fusion in software layer, not on the drone |

---

## Marketed-vs-Measured Gaps to Flag

- **Autel "720° omnidirectional"** = 360°×360° hemispherical marketing; physically two fisheye hemispheres + radar. Real coverage non-uniform.
- **Parrot Anafi AI "360° obstacle detection"** = swept gimbal, temporal not instantaneous; subject motion in blind sweep window goes unseen.
- **Skydio Dock+X10 "100 TOPS"** is the *combined* edge envelope, not Orin alone; X10 vehicle hosts Jetson Orin (specific SKU not disclosed) + Snapdragon 865.
- **Anduril Bolt/Roadrunner/Anvil sensor stacks** — no public spec; everything is "advanced onboard computing." Treat as `[unclear]`.
- **Shield AI Nova 2** sensor list also undisclosed — only that it does 3D real-time and runs Hivemind.
- **DJI Matrice "obstacle sensing FOV"** numbers (65°×50° fwd) are the **stereo binocular envelope**, narrower than the fisheye consumer line; M350 trades cam FOV for stereo depth precision at survey range.
- **Switchblade "AI targeting"** in Block 20 — operator-in-the-loop PID commit, not fully autonomous engagement. Marketed as autonomy; legally semi-autonomous.

---

## Compute Substrate Summary

| SoC | Used by |
|---|---|
| NVIDIA Jetson Orin | Skydio X10/X10D, Quantum Systems Vector AI (dual SoM), many PX4+ROS2 companion configs |
| NVIDIA Jetson TX2 / Xavier | Skydio 2+ (TX2), legacy research stacks |
| Qualcomm QRB5165 (Flight RB5) | ModalAI VOXL 2 / Starling 2 family; secondary on Skydio X10 |
| Qualcomm Snapdragon 865 | Skydio X10 (secondary) |
| Auterion Skynode (NXP i.MX 8 + STM32 FMU class) | Auterion-OS drones, US DoD swarming refs |
| Vendor undisclosed | DJI (custom ASIC), Autel, Anduril, Shield AI |

---

## Endurance / Payload Quick Reference

| Platform | Endurance | Payload / MTOW | Notes |
|---|---|---|---|
| Skydio X10 | 35–40 min | 2.11 kg AUW | Modular VT300 payloads |
| DJI Mavic 3 Pro | 46 min | ~960 g | Consumer |
| DJI Matrice 350 RTK | 55 min | 9.2 kg MTOW, 2.7 kg payload | + Zenmuse L2 |
| Parrot Anafi AI | 32 min | 898 g | LTE link |
| Autel EVO Max 4T | 42 min | ~1.6 kg | Radar |
| Shield AI V-BAT Block | 13+ h | 73 kg / 18 kg payload | SATCOM, heavy fuel |
| Shield AI Nova 2 | indoor mins | small quad | Hivemind |
| Anduril Bolt-M | 45 min | 5.4 kg | 20 km |
| Anduril ALTIUS-600M | ~2 h | ~12 kg class | Tube launch |
| AV Switchblade 300 BL20 | 20+ min | ~3 kg | 30 km |
| AV Switchblade 600 | 40 min | ~50 kg | Anti-armor |
| AV JUMP 20 | 13+ h | 97.5 kg / 13.6 kg payload | Group 3 VTOL |
| Quantum Vector AI | (extended, not stated) | 9.5 kg MTOW | Dual Orin |
| Edge VXE30 Stalker | 8 h batt / 12+ h SOFC | 22 kg | SOCOM use |
| Edge Penguin C VTOL | (not stated) | 42 kg / 4.5 kg payload | 13 kft |
| Teal Black Widow | 45+ min | <1.4 kg | SRR pick |
| ModalAI Starling 2 / Max | ~20 min | dev drone | 15+ TOPS |
