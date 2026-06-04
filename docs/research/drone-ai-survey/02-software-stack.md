# AI Drone Software Stack Research (2025-2026)

Caveman prose. Tables + code blocks normal. Builds on `tmp/ai-drone-fov-research.md`.
URL cites inline. `[unclear]` = vendor opaque or sparse.

---

## 1. OS Split — Two-Processor vs Single-SoC

Pattern dominant in serious drones: **two-processor split**. Hard-RT autopilot RTOS on MCU + Linux companion on apps SoC. Exception: ModalAI VOXL line collapses both onto one Qualcomm SoC w/ DSP partition (still two domains, one die).

### Flight controller RTOS
- **PX4 → Apache NuttX**. Default + canonical. Modules = tasks, shared address space, uORB pub/sub. ~"reactive, async, updates instantly when new data" ([PX4 architecture](https://docs.px4.io/main/en/concept/architecture)).
- **ArduPilot → ChibiOS**. Switched from NuttX at Copter 3.6 (~2018). 3.7+ ChibiOS-only. Result: smaller flash, faster loop, less jitter. Now all ArduPilot autopilots use it incl. small race boards ([ArduPilot ChibiOS port](https://discuss.ardupilot.org/t/ardupilot-porting-guide-for-chibios-stm-microcontrollers/26415)).
- **FreeRTOS**. Cheap drones, hobbyist FCs, custom builds. Less common on serious autonomy.
- **QNX**. Niche but real. Flying-Cam SARAH (Oscar-winning cinema UAS) on QNX Neutrino ([QNX Flying-Cam](http://www.qnx.com/news/pr_5831_1.html)). QNX OS for Safety 8.0 (Aug 2025) — ISO 26262 ASIL D + IEC 61508 SIL 3, aerospace/defense pitch ([QNX QOS 8.0](https://www.newswire.com/news/qnx-launches-qnx-os-for-safety-qos-8-0-to-accelerate-development-of-safety-and)). Adoption in drones small vs cars.
- **Zephyr**. Growing but research-grade for FCs. No major shipping autopilot uses it yet `[unclear]`.
- **VxWorks**. Aerospace heritage, classified programs `[unclear]` — not visible in shipping commercial/defense drones surveyed.
- **DJI / Autel proprietary**. Closed RTOS on custom ASICs (P1/S1/S2 "Pigeon" chipsets). Not publicly documented ([DJI Ocusync chipsets](https://fpvwiki.co.uk/dji-ocusync-p1-soc)).

### Companion / mission computer
- **Linux base**. Universal on Western autonomy stacks. Distro varies:
  - **L4T (Linux4Tegra)** — NVIDIA's Ubuntu-derived BSP. Standard on Jetson. JetPack ships CUDA + cuDNN + TensorRT + DeepStream + VPI ([JetPack](https://developer.nvidia.com/embedded/jetpack)).
  - **Yocto-derived custom** — Auterion AOS is hardened PX4 on FC + custom Linux mission OS on apps proc. Auterion doesn't explicitly say "Yocto" in docs but pattern matches ([Auterion Skynode](https://docs.auterion.com/hardware-integration/skynode)).
  - **Ubuntu (stock-ish)** — research stacks, ModalAI VOXL ships Ubuntu derivative.
  - **Android** — DJI's user-facing controllers; not flight side.
- **QNX companion** rare in drones; ADAS-side (WeRide etc.) is the QNX growth story.

### Single-SoC exception: ModalAI VOXL 2
QRB5165 hosts both. PX4 runs **partially on SLPI DSP** (IMU, baro, mag, GPS, ESC, state estimator) + partially on **ARM64 apps cluster** (MAVLink, logging, perception). DSP↔ARM via Qualcomm proprietary shared-memory ([VOXL 2 PX4 dev guide](https://docs.modalai.com/voxl-px4-dev-build-guide/)). One die, two domains — physically integrated but software-architecturally still a split.

| Stack | FC RTOS | Companion OS | Pattern |
|---|---|---|---|
| Skydio X10 / X10D | proprietary `[unclear]` | L4T (Jetson Orin) + Qualcomm side | dual-SoC |
| Anduril ALTIUS / Roadrunner | proprietary `[unclear]` | NVIDIA "self-driving-car-derived" units | dual-SoC, edge |
| Shield AI V-BAT / Nova | proprietary "Hivemind Edge" stack | open + modular, platform-agnostic | dual-SoC |
| ModalAI VOXL 2 / Starling | PX4 on Hexagon SLPI DSP | Ubuntu on QRB5165 ARM64 | single-SoC, partitioned |
| Auterion Skynode (PX4) | PX4 on NuttX (FMU) | AuterionOS Linux (mission comp) | dual-die in one device |
| Quantum Systems Vector AI | PX4 / NuttX `[unclear]` | L4T (dual Jetson Orin) | dual-SoC |
| ArduPilot OEMs | ChibiOS | varies: RPi / Jetson / x86 | dual-board |
| DJI Mavic 3 / Air 3S | DJI proprietary | DJI proprietary ARM SoC | dual-SoC, closed |

---

## 2. CUDA + Jetson Usage

Jetson = de facto Western autonomy companion. AGX Orin range = up to 275 TOPS, Orin NX up to 157 TOPS, Orin Nano up to 67 TOPS (Super refresh) ([NVIDIA Jetson Orin](https://www.nvidia.com/en-us/autonomous-machines/embedded-systems/jetson-orin/)).

| Platform | Jetson SKU | AI runtime |
|---|---|---|
| Skydio X10 / X10D | Orin (NX-class likely, X10+Dock claim 100 TOPS) `[SKU unclear]` | TensorRT (JetPack stack) ([Skydio X10](https://www.skydio.com/x10), [JetPack](https://developer.nvidia.com/embedded/jetpack)) |
| Quantum Systems Vector AI | **dual** Jetson Orin (SoM) `[NX vs AGX unclear]` | likely TensorRT + ROS 2 / Isaac ROS ([Vector AI](https://quantum-systems.com/us/vector-ai/)) |
| Anduril ALTIUS / Roadrunner | NVIDIA "self-driving-car" chip — likely Orin/Drive `[unclear]` | proprietary Lattice autonomy ([Anduril Wikipedia](https://en.wikipedia.org/wiki/Anduril_Industries)) |
| Holybro Pixhawk Jetson Baseboard | Orin NX 16 GB OR Orin Nano 4 GB | open: ROS 2 + Isaac ROS + TensorRT ([Holybro PX4 doc](https://docs.px4.io/main/en/companion_computer/holybro_pixhawk_jetson_baseboard)) |
| Neousys FLYC-300 mission comp | Orin NX (100 TOPS) | open / customer choice |
| Skydio 2+ (legacy) | Tegra TX2 | (pre-Orin) |

### AI runtime patterns
- **TensorRT** is the universal endpoint. Models flow `PyTorch → ONNX → TensorRT engine` w/ FP16 or INT8 calibration. YOLOv10n at 640 px hits ~50 Hz on Orin NX ([ONNX→TensorRT workflow](https://nvidia-jetson.piveral.com/jetson-orin-nano/using-tensorrt-with-onnx-models-in-jetson-inference/), [arXiv aerial-autonomy-stack](https://arxiv.org/html/2602.07264v1)).
- **ONNX Runtime** also available on Jetson w/ TensorRT execution provider ([ONNX Runtime Jetson](https://developer.nvidia.com/blog/announcing-onnx-runtime-for-jetson/)).
- **DeepStream** = NVIDIA's GStreamer-based video analytics SDK; used for multi-camera pipelines.
- **VPI** (Vision Programming Interface) = NVIDIA's CV primitives library; alternative to OpenCV-CUDA.
- **Holoscan** = newer streaming-sensor framework. Zero-copy GPU pipeline, fits drone perception. Holoscan 3.0 (GTC 2025) supports IGX Orin 500. Pitched explicitly for "security drone runs cheap motion detector → reroutes stream to heavy transformer on trigger" ([Holoscan 3.0 blog](https://developer.nvidia.com/blog/easily-build-edge-ai-apps-with-dynamic-flow-control-in-nvidia-holoscan-3-0/), [Holoscan primer](https://premioinc.com/blogs/blog/what-is-nvidia-holoscan-a-practical-guide-to-real-time-sensor-and-camera-pipelines-at-the-edge)). Adoption in shipping drones — sparse `[unclear]`; mostly medical / industrial robotics today.
- **Triton** server — server-side / fleet inference, not on drone itself.
- **cuDNN direct** — rare; teams call TensorRT or ORT.

---

## 3. Non-CUDA AI Accelerators

### Qualcomm QRB5165 (ModalAI VOXL line, Snapdragon Flight)
- Hexagon Tensor Accelerator + Adreno GPU + Hexagon DSP + NPU → 15+ TOPS combined ([VOXL 2](https://www.modalai.com/products/voxl-2)).
- **Two runtime paths** documented:
  1. **Qualcomm Neural Processing SDK (SNPE)** → `.dlc` format. Convert from TF/ONNX/Caffe.
  2. **voxl-tflite-server** → LiteRT (formerly TFLite) on CPU/GPU/DSP delegates ([ModalAI deep-learning docs](https://docs.modalai.com/voxl-tflite-server/)).
- AIMET (AI Model Efficiency Toolkit) = Qualcomm's quantization+compression toolchain. Companion to SNPE. Mentioned in vendor materials, not heavily called out in VOXL community.
- QNN (Qualcomm Neural Network SDK) = newer unified SDK superseding SNPE on newer Snapdragon. VOXL community mostly still SNPE-era `[unclear]`.

### Hailo-8 / Hailo-15
- 26 TOPS @ ~2.5 W, M.2 form factor ([Hailo-8](https://hailo.ai/products/ai-accelerators/hailo-8-ai-accelerator/)).
- Pitch explicitly drone-friendly: agricultural crop scan, real-time CNN vision ([Hailo drones](https://hailo.ai/resources/industries/drones/hailo-ai-processors-for-drones/)).
- Toolchain: Hailo Dataflow Compiler converts ONNX/TFLite → `.hef`.
- Drone-side deployment shipping: **sparse documented** — appears more in industrial cameras + Raspberry Pi 5 AI HAT than nameable drone SKUs `[unclear]`.

### Coral Edge TPU (Google)
- 4 TOPS INT8, USB-stick or SoM. 2–10 ms inference per typical detection model → 30+ FPS.
- Hobbyist + ArduPilot+DroneKit search-and-rescue projects ([Coral SAR drone](https://www.hackster.io/bandofpv/search-and-rescue-drone-with-google-coral-a485c7), [Coral Cognifly](https://github.com/OliDug/Coral_Cognifly)).
- Production drone SKUs using Coral: **none I found**. Pattern is Pi+Coral DIY, not commercial drone.

### Ambarella CV5 / CV5S
- AI vision SoC, 5 nm, < 2 W @ 8K30. CVflow AI engine + dual A76. SLAM + path planning + OA marketed ([Ambarella CV5](https://www.electronics-lab.com/ambarella-cv5-ai-vision-soc-for-low-power-computer-vision-applications/)).
- **Antigravity A1** drone (CES 2026) — first announced CV5-powered drone ([Ambarella+Antigravity](https://ambarella.gcs-web.com/news-releases/news-release-details/ambarella-accelerates-edge-ai-innovation-next-generation-drones)). Antigravity = DJI sister-brand, ironic given Western anti-DJI policy.
- CVflow toolchain: TF/PyTorch/ONNX → Ambarella amba_cnnflow. Closed-vendor.

### Custom ASICs (DJI, Autel)
- DJI P1/S1/S2 "Pigeon" custom ASICs primarily handle the **OcuSync RF link**, not the AI core ([DJI P1 history](https://www.suasnews.com/2022/05/the-dji-p1-and-s1-fpv-chipset-its-not-all-that/), [DJI Pigeon details](https://fpvwiki.co.uk/dji-ocusync-p1-soc)).
- DJI's actual AI compute SoC (APAS 5.0, ActiveTrack 360) is undisclosed ARM-class chip `[unclear]`.
- Autel "Autonomy Engine" — vendor undisclosed silicon `[unclear]`.

---

## 4. RTOS + AI Integration — Is It Real?

Short answer: yes for sensor-fusion + battery + minor classification; **no** for primary navigation perception. Primary perception lives on Linux + Jetson/QRB5165.

### Shipping / validated patterns

- **TFLite Micro + CMSIS-NN on Cortex-M**. Real and deployed. Tight kernel optimisations: 6–7× conv/FC speedup, INT8 quantized models, MAC-aligned data packs ([TFLM CMSIS-NN](https://blog.tensorflow.org/2021/02/accelerated-inference-on-arm-microcontrollers-with-tensorflow-lite.html), [TFLM cmsis_nn dir](https://github.com/tensorflow/tflite-micro/tree/main/tensorflow/lite/micro/kernels/cmsis_nn)).
- **ARM Ethos-U55 / U65 microNPU**. Pairs with Cortex-M55/M85. 256–512 MAC, low-mW NPU. TVM + Vela compiler path. Pitch: "100M MAC/inference at multi-Hz, vs sub-Hz w/o NPU" ([ARM ML blog](https://developer.arm.com/community/arm-community-blogs/b/ai-blog/posts/ml-based-embedded-computer-vision)). Drone deployments: **not yet visible in commercial drone SKUs** `[unclear]`. CubeSat TinyML papers reference it though ([arXiv CubeSat TinyML](https://arxiv.org/pdf/2603.20174)).
- **Edge Impulse + EON Compiler**. Real. UAV battery RUL on RP2040 Cortex-M0+ via Edge Impulse pipeline ([UAV battery TinyML](https://pmc.ncbi.nlm.nih.gov/articles/PMC12196908/)). OpenMV-on-DJI-Tello research uses Edge Impulse for low-end nav inference. Production-scale drone deployments: research-grade.
- **microTVM**. Generic TVM for MCUs / Ethos-U. Toolchain mature; drone-specific case studies thin.
- **Cadence HiFi / Tensilica DSPs**. Inside Qualcomm Hexagon, NXP i.MX RT, etc. — used as the underlying DSP target, not directly chosen by drone integrators.
- **Syntiant, Greenwaves GAP9**. Audio + ultra-low-power TinyML niche. Not visible in named drone products.
- **Bosch Sensortec BMI323/BMI270 + on-chip ML core**. IMU-side gesture / motion classification. Not a perception path.

### Reality check
Production drones do not run BEV transformer models on Cortex-M. They run **sensor fusion + filter + simple anomaly detection** on the MCU side, then everything heavier (object detection, VIO, SLAM, tracking) lives on the Linux companion.

| Compute tier | AI workload |
|---|---|
| FC MCU (NuttX/ChibiOS) | sensor fusion (EKF), failsafe logic, NO heavy NN |
| Hexagon DSP / Ethos-U / TFLM | optional: anomaly detection, simple classification, motion gating |
| Linux companion (Jetson/QRB5165) | YOLO-class object detection, VIO, depth, SLAM, tracking |

---

## 5. PX4 / ROS 2 Bridging

### uXRCE-DDS as the standard (PX4 v1.14+)
- v1.13 → Fast-RTPS bridge. **v1.14 retired it**, replaced by uXRCE-DDS ([PX4 uXRCE-DDS](https://docs.px4.io/main/en/middleware/uxrce_dds)).
- Architecture: PX4-side **client** (uXRCE-DDS client embedded in NuttX firmware) ↔ **micro-XRCE-DDS-Agent** on companion (Linux). Agent acts as DDS proxy; transports = serial or UDP.
- Message contract: PX4 main exports uORB messages to `PX4/px4_msgs` ROS 2 package. ROS 2 apps build against same version.
- Auterion ships this on Skynode/Skynode X by default ([Auterion uXRCE-DDS](https://docs.auterion.com/hardware-integration/flight-controller-customization/micro-xrce-dds), [Auterion DDS/ROS2 config](https://docs.auterion.com/app-development/app-framework/dds-ros2-configuration)).
- Adoption: **near-universal** in the open PX4 + ROS 2 ecosystem now. Tutorials, university courses, startup stacks all assume uXRCE-DDS. ArduPilot has its own ROS 2 / DDS bridge — separate effort.

### Auterion Skynode — ROS 2 in Docker (production-grade)
- AuterionOS on mission computer runs **ROS 2 apps in Docker containers** ([Auterion ROS 2 push](https://auterion.com/auterion-driving-ros-2-adoption-for-flying-robots/), [Skynode X](https://auterion.com/product/skynode-x/)).
- DDS middleware = **Fast DDS** (recommended by Auterion); supports shared-memory + zero-copy intra-host.
- Deployed across multiple US DoD swarming programs (Auterion Nov 2025 launch).

### ModalAI VOXL 2 — single-SoC bridging
- PX4 on Hexagon SLPI DSP + Linux on ARM64 same QRB5165 die.
- DSP↔ARM = Qualcomm shared-memory IPC (proprietary).
- For ROS 2: install `voxl-microdds-agent` on apps side; uXRCE-DDS client baked into PX4. ROS 2 talks to PX4 via the agent ([ModalAI ROS 2 install](https://docs.modalai.com/ros2-installation-voxl2/), [VOXL ROS 2 forum](https://forum.modalai.com/topic/4055/ros2-microdds-communication-with-px4-from-external-fc-fvc2)).
- Same uXRCE pattern as Auterion, collapsed onto one chip.

### Skynode X (compute spec for the curious)
- Mission computer: quad-core ARM Cortex-A53 @ 1.8 GHz, 4 GB RAM, 16 GB eMMC. Not a Jetson — for *heavier* perception, customers add Jetson via the Skynode X expansion ([Skynode S/X datasheet](https://auterion.com/wp-content/uploads/2025/03/M216-Auterion-Skynode-S.pdf)).

---

## 6. Inference Runtime + Framework Picks

| Stack | Training framework (host) | Edge runtime |
|---|---|---|
| Skydio X10 / X10D (Jetson Orin) | PyTorch (assumed) → ONNX | TensorRT |
| Quantum Systems Vector AI (dual Orin) | likely PyTorch/TF → ONNX | TensorRT, possibly Isaac ROS perception nodes |
| ModalAI VOXL 2 | TF / PyTorch → ONNX → DLC (SNPE) or TFLite | SNPE / QNN / voxl-tflite-server |
| Auterion Skynode + ROS 2 app | customer choice | customer choice (containerised) |
| ArduPilot + Pi/Jetson DIY | PyTorch | TFLite / TensorRT / ORT |
| Anduril Lattice-side | proprietary `[unclear]` | proprietary `[unclear]` |
| Shield AI Hivemind | proprietary `[unclear]` | proprietary, "platform-agnostic" HAL ([Hivemind SDK](https://shield.ai/from-concept-to-combat-how-hivemind-sdk-powers-next-gen-autonomy/)) |
| Cortex-M side (any) | TF → TFLM | TFLM + CMSIS-NN, sometimes Edge Impulse / TVM / Vela |
| OpenVINO / NCNN | rarely used on drones — OpenVINO is Intel, drones are mostly ARM | minor |

### Models being run
- **Drone perception ≠ car perception** today. Drones largely run **YOLO-class detectors** (v8/v10/v11n variants) for tracking + person/vehicle classification + obstacle bounding boxes ([YOLO drone survey](https://pmc.ncbi.nlm.nih.gov/articles/PMC12736610/)).
- **Multi-modal IR+RGB fusion** appearing in counter-UAS detection (EGD-YOLO etc. — research grade) ([EGD-YOLO arXiv](https://arxiv.org/pdf/2510.10765)).
- **VIO / VINS / ORB-SLAM3** for GPS-denied — classical CV + small NN front-ends.
- **BEV transformer / multi-camera fusion à la cars** — not common on drones yet. Drones don't have the same horizontal-plane prior. Skydio's 6× fisheye → 3D occupancy is closer to NeRF / multi-view geometry, vendor stack details closed `[unclear]`.

---

## 7. Real-Time Guarantees for AI Inference

### Hard-RT — flight control only
- Inner loop (attitude / rate) on NuttX/ChibiOS MCU. Hard-RT.
- AI perception **NOT in this loop**. Output of perception → set-point or hazard flag for FC.

### Soft-RT — perception
- PREEMPT_RT-patched L4T kernel **officially supported by NVIDIA** since ~Jan 2021. Available via Debian OTA. Set Preemption Model = "Fully Preemptible Kernel (RT)" + 1000 Hz timer ([Jetson RT kernel docs](https://orenbell.com/setting-up-realtime-kernel-on-jetson/), [NVIDIA Jetson kernel customisation](https://docs.nvidia.com/jetson/archives/r36.4.3/DeveloperGuide/SD/Kernel/KernelCustomization.html)).
- Adoption: research robotics + serious autonomy stacks. Anduril / Skydio / Shield AI internal kernel choices `[unclear]`. Open community runs PREEMPT_RT on Orin AGX for ROS 2 perception.
- TensorRT itself **not deterministic** — best-effort under PREEMPT_RT. Real-time bottleneck research on Orin AGX shows accuracy-vs-latency frontier non-trivial ([Real-time perception arXiv](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC12583037/)).

### Hardware fault-tolerance
- Pattern: **separate compute for safety-critical vs perception**.
  - FC MCU = safety. Failsafe + RTL + sensor-only fallback.
  - Companion = perception. If it dies, drone still flies (degraded).
- Auterion explicitly splits FMU vs mission computer over DDS for this reason.
- Skynode X expansion = optional Jetson alongside the always-on A53 mission computer.
- ModalAI VOXL2 = exception: failsafe needs careful design since DSP + apps share die. Qualcomm safety island design helps but no public ASIL claim `[unclear]`.

---

## 8. Open vs Closed Map

| Drone / stack | Flight stack | Companion stack | Comms | Net openness |
|---|---|---|---|---|
| PX4 + ROS 2 reference | open (PX4, NuttX, MAVLink, uXRCE-DDS) | open (Linux + ROS 2) | open | fully open |
| Auterion Skynode | open core (PX4) + hardened | AuterionOS = proprietary Linux + ROS 2 / Docker | MAVLink + DDS | mostly open |
| Skydio X10 / X10D | proprietary | proprietary core, MAVLink/RAS-A surface for 3rd-party C2 | MAVLink (control + telemetry only) ([Skydio X10D Control ICD](https://www.skydio.com/blog/unlocking-skydio-x10d-control-and-telemetry-for-developers)) | closed core, open edges |
| ModalAI VOXL 2 / Starling | open (PX4 on DSP) | open (Linux + ROS 2 + voxl-* services) | MAVLink + DDS | open |
| Holybro Pixhawk + Jetson | open | open | open | open |
| Anduril (ALTIUS / Roadrunner / Bolt) | proprietary | Lattice — proprietary; Lattice SDK exposes select APIs | proprietary mesh | closed, SDK-walled |
| Shield AI Hivemind / V-BAT | proprietary | Hivemind — platform-agnostic but closed runtime | proprietary | closed |
| DJI consumer + Enterprise | closed (proprietary RTOS, P1/S1/S2 ASIC) | closed | proprietary OcuSync | fully closed |
| Autel | closed (Autonomy Engine) | closed | proprietary | fully closed |
| ArduPilot OEMs | open (ChibiOS, ArduPilot) | open | MAVLink + AP-DDS | fully open |

Headline: open camp = **PX4 + ChibiOS-on-ArduPilot + Auterion + ModalAI + Holybro + Pixhawk hardware**, communicating via MAVLink + uXRCE-DDS → ROS 2. Closed camp = **DJI + Autel + Anduril + Shield AI + Skydio (core)**. Western defense stacks expose MAVLink/RAS-A as a thin compatibility surface but autonomy IP stays internal.

---

## Cross-Cutting Observations

1. **Single de facto bridge protocol**: uXRCE-DDS replaces the older PX4-Fast-RTPS bridge across the open stack. Anything new uses it. ROS 2 + Fast DDS / Cyclone DDS is the companion-side world.
2. **Jetson Orin is the autonomy GPU**. Orin NX (100–157 TOPS) is the sweet spot for sUAS; Orin AGX for heavier ISR; Orin Nano for budget/research; Xavier NX largely supplanted on new designs.
3. **Qualcomm QRB5165 is the only credible non-NVIDIA path** with shipping product (ModalAI). Hailo + Ambarella exist but the drone-side product evidence is thin.
4. **TinyML on the FC MCU is a side channel, not the main perception**. CMSIS-NN + TFLM + Ethos-U55 / Edge Impulse mostly research and battery/sensor-fusion work.
5. **PREEMPT_RT on Tegra is mature** and the standard answer for soft-RT perception scheduling. No vendor sells "hard-RT TensorRT" today.
6. **DJI's chip story is RF, not AI**. Pigeon ASICs handle OcuSync. The actual ARM SoC running APAS-5/ActiveTrack-360 is undisclosed.
7. **Sparse spots**:
   - Anduril / Shield AI / Skydio compute SKUs published `[unclear]`.
   - Vector AI Orin variant (NX vs AGX) `[unclear]`.
   - Hailo + Ambarella shipping-drone customer list `[unclear]` (Antigravity A1 is the first named one).
   - Adoption of NVIDIA Holoscan inside actual drones `[unclear]` — primarily medical/industrial today.
