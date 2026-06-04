# PX4 + Pure-RTOS AI (No Linux Companion) — Feasibility Research

Builds on `tmp/ai-drone-sw-stack.md`. Caveman prose. URLs after each claim. `[unclear]` = sparse.

Core q: PX4 on NuttX/Cortex-M alone — can run inference? What works today, what's research-only?

Headline finding upfront: **PX4 mainline already merged a TFLM-based neural-control module (PR #24366, `mc_nn_control`)**. So "pure-RTOS AI on PX4" is not hypothetical — it's shipping. But scope is narrow: tiny MLPs replacing the inner-loop controller, **not** primary perception ([PX4 docs: mc_nn_control](https://docs.px4.io/main/en/neural_networks/mc_neural_network_control), [arxiv 2505.00432](https://arxiv.org/html/2505.00432v1)).

---

## 1. Pixhawk-class MCU Compute Envelope

All current "serious" PX4 boards are STM32H7 (Cortex-M7 @ 400–480 MHz, double-prec FPU, ARMv7-M DSP ext). **No MVE/Helium yet** — that's Cortex-M55/M85 territory, none shipping as FC. **No on-die NPU** on any shipping autopilot.

| Board | MCU | Clock | Flash | RAM | FPU | SIMD | NPU |
|---|---|---|---|---|---|---|---|
| Pixhawk 6X / 6X Pro | STM32H753 | 480 MHz | 2 MB | 1 MB | double-prec | DSP ext (no MVE) | — |
| Pixhawk 6C | STM32H743 | 480 MHz | 2 MB | 1 MB | double-prec | DSP ext | — |
| Cube Orange+ | STM32H757 (M7+M4) | 480/200 MHz | 2 MB | 1 MB | double-prec | DSP ext | — |
| Holybro Durandal | STM32H743 | 480 MHz | 2 MB | 1 MB | double-prec | DSP ext | — |
| Pixhawk 5X | STM32F765 | 216 MHz | 2 MB | 512 KB | single-prec | DSP ext | — |
| mRo Pixracer Pro | STM32H743 | 460 MHz | 2 MB | 1 MB | double-prec | DSP ext | — |

Sources: [Pixhawk 6X](https://docs.px4.io/main/en/flight_controller/pixhawk6x), [Pixhawk 6X Pro](https://www.pixhawk.store/docs/flight-controller/pixhawk-6x-pro/pixhawk-6x-pro-technical-specification-1818), [Cube Orange+](https://ardupilot.org/copter/docs/common-thecubeorange-overview.html), [Durandal](https://docs.px4.io/main/en/flight_controller/durandal.html), [Pixhawk 5X](https://cdn.sparkfun.com/assets/e/f/1/2/c/Holybro_Pixhawk5X_Spec_Overview.pdf), [arxiv 2505.00432](https://arxiv.org/html/2505.00432v1).

Key envelope numbers for AI budget:
- Compute headroom for inference once PX4 modules consume their share: **~50 KB RAM** ([arxiv 2505.00432](https://arxiv.org/html/2505.00432v1) — explicit budget the neural-control paper used).
- Flash budget for model weights: ~hundreds of KB before crowding out flight modules (existing config near capacity — adding `mc_nn_control` requires removing FW/rover/VTOL modules to fit on some boards) ([PX4 mc_nn_control](https://docs.px4.io/main/en/neural_networks/mc_neural_network_control)).
- Cortex-M7 DSP ext = SMLAD-class 16×16+32 MACs, no 8-bit dot-product, no 128-bit vectors. CMSIS-NN int8 kernels work but no Helium speedup.

Sibling MCUs on the AI horizon (NOT yet in any PX4 board):
- **STM32N6** — Cortex-M55 @ 800 MHz + Neural-ART NPU @ 1 GHz, 600 GOPS, 4.2 MB embedded RAM, 3 TOPS/W ([STM32N6 blog](https://blog.st.com/stm32n6/), [Hackster](https://www.hackster.io/news/stmicroelectronics-stm32n6-brings-its-in-house-neural-art-npu-to-bear-on-tinyml-computer-vision-0be055f0bdc5)).
- **Alif Ensemble E3/E5/E7** — dual Cortex-M55 + dual Ethos-U55, 250+ GOPS ([CNX](https://www.cnx-software.com/2022/12/11/alif-ensemble-cortex-a32-cortex-m55-chips-feature-ethos-u55-ai-accelerator/), [Alif Ensemble](https://alifsemi.com/products/ensemble/)).
- **NXP i.MX RT1170** — Cortex-M7 @ 1 GHz + Cortex-M4 @ 400 MHz, CMSIS-NN class, no NPU on this part ([NXP RT1170](https://www.nxp.com/products/i.MX-RT1170)).
- **Renesas RA8** — Cortex-M85 + optional Ethos-U55 via external bus ([Renesas Ethos-U on RA8](https://www.renesas.com/en/document/apn/using-ethos-u-npu-ra8-mcus)).

PX4 board with any of these = none surveyed, fully roadmap territory `[unclear]`.

---

## 2. TFLite Micro on NuttX

**Status: officially merged into PX4 mainline** (May 2025, PR #24366).

- Submodule pin: `tensorflow/tflite-micro` main branch at `src/lib/tflm/tflite_micro/` ([PX4 TFLM docs](https://docs.px4.io/main/en/neural_networks/tflm)).
- Build path: model → `xxd -i model.tflite > model_data.cc` → C array linked into firmware (no filesystem dep on MCU) ([PX4 TFLM docs](https://docs.px4.io/main/en/neural_networks/tflm)).
- Uses `MicroMutableOpResolver` — op-by-op opt-in; only ops actually used are pulled in (saves flash) ([PX4 TFLM docs](https://docs.px4.io/main/en/neural_networks/tflm)).
- Timing layer abstracted across NuttX vs SITL; the same module builds for sim + flight ([PX4 mc_nn_control](https://docs.px4.io/main/en/neural_networks/mc_neural_network_control)).
- **CMSIS-NN: not enabled in mainline integration** (paper does not mention it; PR does not mention it) ([arxiv 2505.00432](https://arxiv.org/html/2505.00432v1)). Optimization left on the table.
- Community ports / GitHub: NTNU ARL group is the canonical upstream contributor (`SindreMHegre` PR + `ntnu-arl/px4-nns`) ([PX4 PR #24366](https://github.com/PX4/PX4-Autopilot/pull/24366), [NTNU project page](https://ntnu-arl.github.io/px4-nns/)).

Other real PX4 modules using TFLM beyond control: **none surveyed yet** `[unclear]`. The `RAPTOR Adaptive RL NN Module` listed in PX4 nav appears to be another control variant (RL-based adaptive). No TFLM-based vision/anomaly modules in mainline.

---

## 3. CMSIS-NN + Helium on M55/M85

CMSIS-NN speedup on Cortex-M55/M85 vs baseline ARMv7-M DSP path: **~5× on conv hot loops, 4.6× throughput / 4.9× energy across kernels** ([TF blog: TFLM+CMSIS-NN](https://blog.tensorflow.org/2021/02/accelerated-inference-on-arm-microcontrollers-with-tensorflow-lite.html), [CMSIS-NN README](https://github.com/ARM-software/CMSIS-NN), [arxiv 1801.06601](https://arxiv.org/pdf/1801.06601)).

Helium = 128-bit vector instructions, primarily 8-bit quantized (int8) kernels. M55 is the first part to ship it; M85 the second ([Renesas M85+Helium](https://www.renesas.com/en/blogs/leveraging-helium-and-arm-cortex-m85-unprecedented-dsp-and-ai-performance-mcu-core), [Alif M55 optimization](https://alifsemi.com/whitepaper/cortex-m55-optimization-and-tools/)).

Adoption in shipping flight controllers: **zero surveyed** `[unclear]`. Every current Pixhawk-class FC is STM32H7-class M7 — no MVE.

Cortex-M55-based FC announcements: **none I could find in 2025-2026** `[unclear]`. Alif Ensemble has the hardware profile but no public drone-FC SKU. STM32N6 board released for dev but no flight-controller integrator yet.

---

## 4. ARM Ethos-U55 / U65 + NuttX

Ethos-U55 = microNPU companion to Cortex-M, claims **480× ML perf boost** over baseline M-class ([Arm Ethos-U55](https://www.arm.com/products/silicon-ip-cpu/ethos/ethos-u55)). Pairs with M33/M55/M85 host.

PX4 board with Ethos-U: **none surveyed** `[unclear]`.

STM32 NPU pipeline: STM32N6 is the only ST part with an NPU — it's **ST's in-house Neural-ART**, not Ethos-U. ST not on the Ethos-U bandwagon ([STM32N6 NPU](https://blog.st.com/stm32n6/)).

NXP i.MX RT1170 + Ethos-U: **no — RT1170 has no Ethos-U integration** (it's a pure M7+M4 part, ML acceleration is software via CMSIS-NN) ([RT1170 NXP page](https://www.nxp.com/products/i.MX-RT1170)).

NuttX driver for Ethos-U: **not surveyed**, no upstream NuttX board configs reference Ethos-U `[unclear]`. The Ethos-U driver shipped by Arm is bare-metal C plus a thin RTOS wedge (FreeRTOS reference). Porting effort to NuttX = mechanical (POSIX-y RTOS), but no known port today.

---

## 5. Other Runtimes on NuttX (microTVM, Edge Impulse, NXP eIQ, STM32CubeAI)

| Runtime | NuttX-supported today? | PX4 hook? | Notes |
|---|---|---|---|
| TFLite Micro | **yes — mainline PX4** | `mc_nn_control` | the production path |
| CMSIS-NN | yes (compiles for any Cortex-M, RTOS-agnostic) | indirect via TFLM kernels | not wired into PX4 TFLM build |
| Edge Impulse SDK | RTOS-agnostic C++ runtime; supports STM32/NXP/Nordic targets but **no documented NuttX port** `[unclear]` | none in PX4 | could be ported (uses TFLM + CMSIS-NN under the hood) |
| microTVM | RTOS-agnostic C runtime, bare-metal + Zephyr/FreeRTOS references; **no NuttX template** `[unclear]` | none | research-grade tooling |
| NXP eIQ | FreeRTOS + bare-metal, MCUXpresso ecosystem; **not NuttX** | none | i.MX RT focus |
| STM32CubeAI / X-CUBE-AI | "runs independently of the RTOS" per ST; ST tooling targets CubeMX projects so **no NuttX out-of-box** but generated C is portable | none | ST claims faster + smaller than TFLM |

Sources: [ST X-CUBE-AI community](https://community.st.com/t5/edge-ai/we-are-try-to-run-ros-in-stm32-cortex-m-is-that-possible-to-run/td-p/92470), [Shawn Hymel state-of-embedded-ML](https://shawnhymel.com/2994/deep-learning-on-microcontrollers-the-state-of-embedded-ml-in-2025/), [Edge Impulse arxiv 2212.03332](https://arxiv.org/pdf/2212.03332).

Perf numbers (representative, STM32H7 / M7 class):
- TFLM person-detect (MobileNet v1 0.25, 96×96): 36 ms, **28 fps**, 214 KB flash, 40 KB RAM ([ST people-presence](https://www.st.com/content/st_com/en/st-edge-ai-suite/case-studies/people-presence-detection-visual-wake-word.html)).
- X-CUBE-AI YOLO derivative on STM32H747: real-time multi-person detect demoed at trade shows ([STM32 YOLO H7](https://community.st.com/t5/stm32-mcus-machine-learning-ai/how-to-implement-yolo-on-stm32-h7/td-p/78513)).
- YOLOv5n (1.9M params) compressed → fits in STM32H743 2 MB flash / 512 KB RAM ([arxiv 2507.16155](https://arxiv.org/html/2507.16155v1)).
- TinyVLM: 26 FPS on STM32H7 ([arxiv 2603.00136](https://arxiv.org/html/2603.00136)).
- Keyword spotting: 12-15 ms inference, 240 KB RAM on Cortex-M7 @ 216 MHz ([arxiv 2208.02765](https://arxiv.org/abs/2208.02765)).

**Caveat:** all those numbers are stand-alone demos on dev boards. On a Pixhawk running PX4, you have **~50 KB RAM / hundreds of KB flash** left after the autopilot consumes its share — most demo numbers above assume the MCU is dedicated to inference.

---

## 6. What CAN Actually Run Pure-RTOS Today (per workload)

Budget: STM32H7-class, ~50 KB RAM + few hundred KB flash for AI **after** PX4 reserves the rest ([arxiv 2505.00432](https://arxiv.org/html/2505.00432v1), [PX4 mc_nn_control](https://docs.px4.io/main/en/neural_networks/mc_neural_network_control)).

| Workload | Verdict on Pixhawk H7 | Why |
|---|---|---|
| Inner-loop control NN (MLP, 64+32 neurons) | **shipping** | 50 KB RAM, 93 μs, 650 Hz on Pixracer Pro ([arxiv 2505.00432](https://arxiv.org/html/2505.00432v1)) |
| IMU gesture / motor anomaly (MLP on IMU features) | **realistic** | 97% MLP on Cortex-M4F shown; M7 has more headroom ([Springer 10.1007/s43926-025-00142-4](https://link.springer.com/article/10.1007/s43926-025-00142-4)) |
| Keyword spotting / acoustic event | **realistic but RAM-tight** | 240 KB RAM standalone — would dominate PX4's free RAM, may need a dedicated H7 board variant ([arxiv 2208.02765](https://arxiv.org/abs/2208.02765)) |
| Person detection (VWW, MobileNet v1 0.25, 96×96) | **borderline** | 40 KB RAM activation fits; 214 KB flash for weights doesn't — needs board-config trim to free flash ([ST VWW case](https://www.st.com/content/st_com/en/st-edge-ai-suite/case-studies/people-presence-detection-visual-wake-word.html)) |
| ToF + tiny classifier | **realistic** | sub-KB input, MLP head, easy fit |
| Low-res object presence (binary) | **realistic** | similar profile to VWW, lighter |
| Tiny YOLO (YOLOv5n / Tinyissimo / YOLOv8n) | **research-only, not in PX4** | YOLOv5n needs ~2 MB flash + most of 512 KB RAM standalone — leaves nothing for PX4 ([arxiv 2507.16155](https://arxiv.org/html/2507.16155v1), [Tinyissimo](https://www.emergentmind.com/topics/tinyissimo-yolo)) |
| SLAM / VIO / depth estimation | **no — companion-computer-only** | PX4 docs explicitly route this to companion computer ([PX4 computer-vision](https://docs.px4.io/main/en/advanced/computer_vision.html)) |
| Primary obstacle-avoidance perception | **no — companion-computer-only** | same |

Pattern: **classifier-heads + tiny control nets fit; convnets fit if you crowd out other PX4 modules; nothing perception-grade fits alongside a full PX4 build**.

---

## 7. PX4 Architecture Hooks

uORB pub/sub is the integration contract — same surface as `commander`, `mavlink`, EKF2 ([PX4 uORB explained](https://px4.io/px4-uorb-explained-part-1/), [uORB middleware](https://docs.px4.io/main/en/middleware/uorb.html)).

How `mc_nn_control` wires itself in ([PX4 mc_nn_control](https://docs.px4.io/main/en/neural_networks/mc_neural_network_control)):

```
state-est topics → PopulateInputTensor() → TFLM Invoke() → PublishOutput() → ActuatorMotors
```

It **replaces the entire controller cascade** by subscribing where the classical controller subscribes and publishing where the allocator publishes. The reference doc explicitly says: "by switching which topics the module is subscribed to and publishes, all parts of the standard PX4 control cascade can be replaced."

Implication for a generic `tflm_*` module living alongside `mavlink`/`commander`: **yes, trivial.** Pattern is:
1. Define module like any PX4 work-queue module.
2. Subscribe to source uORB topics (sensors, EKF outputs, vision-bridge).
3. Run `Invoke()` on inference budget.
4. Publish to a new uORB topic (custom message) for downstream consumers.

`commander` (state machine) can subscribe to that new topic to gate mode transitions on inference output (e.g., "person detected → loiter"). No special hook needed — uORB is the entire contract.

Build-time board flag enables/disables the module via `default.px4board` ([PX4 mc_nn_control](https://docs.px4.io/main/en/neural_networks/mc_neural_network_control)).

---

## 8. Commercial Pixhawk-class Drones Running On-FC AI Today

Surveyed: **the academic / open-source case is real (NTNU paper above), the commercial case is essentially zero.** Every "AI drone" SKU surveyed offloads to a companion (Jetson, Qualcomm, Hailo) — see prior `tmp/ai-drone-sw-stack.md`.

Adjacent / partial matches:
- Auterion Skynode: PX4 on NuttX (FC) + AuterionOS Linux (mission). Could in principle run TFLM on the FC side; no public claim it does.
- Gumstix CM4 + Pixhawk FMUv6U: "PX4-compatible board with additional embedded AI" — but the AI is on the CM4 Linux side, not the STM32 ([Pixhawk Series](https://docs.px4.io/main/en/flight_controller/pixhawk_series)).
- ArduPilot: no documented neural-network inference module in the autopilot itself `[unclear]`.

Research-only:
- NTNU ARL "Learning-based Micro Flyer" — Pixracer Pro running RL-trained PPO controller via TFLM, 650 Hz, 93 μs inference. **Pure-RTOS, no companion** ([NTNU px4-nns](https://ntnu-arl.github.io/px4-nns/), [arxiv 2505.00432](https://arxiv.org/html/2505.00432v1)).
- Edge fault-detection: LSTM autoencoder + Isolation Forest, but on Raspberry Pi 4 / Jetson Nano — not the FC ([PMC PMC12390472](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC12390472/)).
- Hackster coverage: "PX4 Gets a Neural Upgrade" — first time mainline PX4 has on-FC inference ([Hackster](https://www.hackster.io/news/px4-gets-a-neural-upgrade-c3f5b31f81b8)).

---

## 9. Roadmap

Visible on the public horizon:
- **PX4 mainline TFLM is freshly merged (May 2025).** Expect follow-on modules (vision-bridge, anomaly, mode classifier) over 2025-2026. Nothing announced yet `[unclear]`.
- `RAPTOR Adaptive RL NN Module` listed in PX4 nav alongside `mc_nn_control` → second TFLM-based control variant.
- No public PX4 board project for Cortex-M55 / M85 / Ethos-U / STM32N6 surveyed `[unclear]`. STM32N6 (Neural-ART, 600 GOPS) is the most likely future PX4 base — would change the calculus on perception-grade inference on the FC itself.
- Alif Ensemble dev boards (M55 + dual Ethos-U55) are the most natural near-term "AI-grade FC" hardware base, but no autopilot vendor has picked them up `[unclear]`.
- NuttX itself: no Ethos-U driver upstream surveyed `[unclear]`.
- ArduPilot (ChibiOS) — no neural-control module in shipping firmware `[unclear]`. PX4 ahead.

Forum threads:
- PX4 PR #24366 (merged) ([github](https://github.com/PX4/PX4-Autopilot/pull/24366))
- NTNU project page ([px4-nns](https://ntnu-arl.github.io/px4-nns/))
- Aerial Gym Sim2Real guide ([NTNU sim2real](https://ntnu-arl.github.io/aerial_gym_simulator/9_sim2real/))

---

## 10. Honest Verdict

**"PX4 + pure-RTOS AI" = realistic for:**
- inner-loop control NNs (MLPs replacing classical PID cascade — proven, shipping mainline)
- IMU-based gesture / motor-anomaly classifiers (MLP heads on filtered sensor features)
- acoustic / ToF / single-axis sensor classifiers
- keyword spotting (RAM-tight but doable on a dedicated H7 board variant)
- binary-presence convnets (visual wake word class)

**Not realistic for:**
- primary perception (object detection, depth estimation, SLAM, VIO)
- multi-class object detection at useful resolution
- semantic segmentation
- anything needing > ~50 KB activation RAM or > few hundred KB weights *alongside* full PX4

**Why the perception wall is hard:**
- STM32H7 has no MVE/Helium → CMSIS-NN gains are real but bounded (4-5×), not 100×.
- No on-die NPU on any shipping Pixhawk MCU.
- PX4 consumes most of the H7's RAM budget for EKF2, mavlink, uORB queues, NuttX heap — leaving ~50 KB for AI.
- Camera-pipe (DCMI + ISP) absent from autopilot boards; vision data has to come over a sensor bus, not directly into the MCU's NPU memory.

**Path to breaking the wall:**
- A new "AI Pixhawk" base built on STM32N6 (Neural-ART, 600 GOPS, 4.2 MB RAM) or Alif Ensemble (M55 + Ethos-U55). Either would put real perception inside the FC envelope.
- Neither exists as a PX4 board today. **This is the gap to watch.**

So as of mid-2026 the answer is: **inference-on-FC is a real, shipping PX4 feature for the control axis — but perception still needs a companion computer.** The hardware to change that is on the market (STM32N6, Alif Ensemble); the integration into a Pixhawk-class board is the missing piece.
