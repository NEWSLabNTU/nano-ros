# QEMU and Physical Device Compatibility Analysis

This document analyzes approaches from reference projects for achieving compatibility
between QEMU-simulated devices and physical hardware, and how nano-ros can adopt these patterns.

## Reference Projects

Three reference projects were studied (cloned to `external/`):

### 1. qemu-stm32-docker

**Repository:** [amamory-embedded/qemu-stm32-docker](https://github.com/amamory-embedded/qemu-stm32-docker)

**Approach:**
- Uses a **forked QEMU** (beckus/qemu_stm32) with custom STM32 peripheral models
- Targets `stm32-p103` machine (STM32F103 - Cortex-M3)
- Includes FreeRTOS examples

**Pros:**
- Accurate STM32 peripheral emulation
- Good STM32F1/F2 support

**Cons:**
- Based on older QEMU (~2.x), outdated
- Requires custom QEMU build
- Not compatible with mainline QEMU

### 2. baremetal-super-minimal

**Repository:** [spanou/baremetal-super-minimal](https://github.com/spanou/baremetal-super-minimal)

**Approach:**
- Uses **official QEMU** with `netduinoplus2` machine (STM32F405 - Cortex-M4)
- Docker container (`spanou/qemu-m4`) with QEMU 7.1.50
- **Supports both virtual (QEMU) and physical (SAM4 XPlained Pro) boards**
- Uses `PLATFORM` define for conditional compilation
- CSV-based register definitions generate both C and ASM headers

**Key Pattern:**
```makefile
# Makefile supports both targets
ifeq ($(BOARD), qemu)
    CPPFLAGS= -DPLATFORM=0
else ifeq ($(BOARD), sam4)
    CPPFLAGS= -DPLATFORM=1
endif
```

**Pros:**
- Uses mainline QEMU
- Clean abstraction pattern for multi-platform
- Same codebase for QEMU and physical hardware

**Cons:**
- Limited to boards QEMU supports

### 3. baremetal-c

**Repository:** [meriac/baremetal-c](https://github.com/meriac/baremetal-c)

**Approach:**
- Docker container (`meriac/arm-gcc`) with just the toolchain
- Targets nRF52 Development Kit (physical board only)
- Uses CMSIS headers for hardware abstraction
- Educational, minimal approach

**Pros:**
- Clean, minimal examples
- CMSIS-based HAL

**Cons:**
- No QEMU support (nRF52 not emulated in QEMU)
- Physical hardware required

## QEMU ARM Machine Support

### Mainline QEMU (Debian Bookworm: v7.2)

| Machine | CPU | Physical Equivalent |
|---------|-----|---------------------|
| `mps2-an385` | Cortex-M3 | ARM V2M MPS2 (FPGA board) |
| `mps2-an386` | Cortex-M4 | ARM V2M MPS2 (FPGA board) |
| `mps2-an500` | Cortex-M7 | ARM V2M MPS2 (FPGA board) |
| `mps2-an505` | Cortex-M33 | ARM V2M MPS2+ (FPGA board) |
| `netduinoplus2` | Cortex-M4 (STM32F405) | Netduino Plus 2 |
| `netduino2` | Cortex-M3 (STM32F205) | Netduino 2 |
| `stm32vldiscovery` | Cortex-M3 (STM32F100) | STM32VLDISCOVERY |
| `olimex-stm32-h405` | Cortex-M4 (STM32F405) | Olimex STM32-H405 |
| `microbit` | Cortex-M0 (nRF51) | BBC micro:bit v1 |

### xPack QEMU Arm (Extended Support)

[xPack QEMU Arm](https://xpack-dev-tools.github.io/qemu-arm-xpack/) adds more boards:
- BluePill (STM32F103C8T6)
- Maple
- NUCLEO-F072RB, NUCLEO-F103RB, NUCLEO-F411RE
- STM32F0-Discovery, STM32F051-Discovery
- Various Olimex boards

### Not Supported

- **nRF52 series** - Not emulated in any QEMU variant
- **Most STM32L/H/G/U series** - Limited support
- **ESP32** - Not ARM-based, separate emulator (QEMU-xtensa)

## MPS2-AN385 vs Physical Hardware

Our current nano-ros QEMU examples use `mps2-an385`:

| Aspect | MPS2-AN385 (QEMU) | Physical Boards |
|--------|-------------------|-----------------|
| CPU | Cortex-M3 | Varies (M0-M7) |
| Flash | 4MB | 64KB-2MB typical |
| RAM | 4MB | 16KB-512KB typical |
| Ethernet | LAN9118 | Varies (W5500, ENC28J60, internal) |
| Debug | Semihosting | UART/SWD |

## Compatibility Strategy for nano-ros

### Option A: Platform Abstraction Layer (Recommended)

Follow baremetal-super-minimal pattern with platform-specific implementations:

```
examples/
├── baremetal-common/          # Shared code
│   ├── src/
│   │   ├── lib.rs            # Platform trait definitions
│   │   ├── zenoh_bridge.rs   # Platform-agnostic zenoh code
│   │   └── clock.rs          # Clock abstraction
│   └── Cargo.toml
├── baremetal-qemu/            # QEMU mps2-an385 platform
│   ├── src/
│   │   ├── platform.rs       # LAN9118 + smoltcp
│   │   └── debug.rs          # Semihosting
│   └── Cargo.toml
├── baremetal-stm32/           # STM32 with W5500 platform
│   ├── src/
│   │   ├── platform.rs       # W5500 SPI + smoltcp
│   │   └── debug.rs          # UART/RTT
│   └── Cargo.toml
└── talker/                    # Application (platform-agnostic)
    ├── src/main.rs
    └── Cargo.toml             # Feature flags select platform
```

### Option B: Docker-based Development

Create a Docker container with:
- QEMU 7.2+ (from Debian bookworm)
- ARM toolchain (arm-none-eabi-gcc)
- Rust with thumbv7m-none-eabi target
- zenoh-pico pre-built libraries

```dockerfile
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    qemu-system-arm \
    gcc-arm-none-eabi \
    libnewlib-arm-none-eabi \
    curl \
    build-essential \
    cmake \
    && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup target add thumbv7m-none-eabi thumbv7em-none-eabihf

WORKDIR /work
```

### Option C: Multiple QEMU Machines

Support multiple QEMU machines that map better to physical boards:

| QEMU Machine | Best Physical Match |
|--------------|---------------------|
| `mps2-an385` | Generic Cortex-M3 development |
| `netduinoplus2` | STM32F4 boards (Nucleo-F4xx, Discovery) |
| `stm32vldiscovery` | STM32F1 boards (BluePill, Nucleo-F1xx) |

## Recommended Next Steps

1. **Create Docker container** with Debian bookworm (QEMU 7.2)
2. **Test existing examples** with netduinoplus2 machine
3. **Design platform trait** for hardware abstraction
4. **Add STM32 platform** using embassy-stm32 or stm32-hal

## Physical Board Recommendations

For developers wanting to test on real hardware:

| Board | MCU | Ethernet | Price | Notes |
|-------|-----|----------|-------|-------|
| Nucleo-F429ZI | STM32F429 (M4) | Built-in | ~$25 | LAN8742A PHY |
| Nucleo-H743ZI | STM32H743 (M7) | Built-in | ~$35 | Best performance |
| STM32F4-Discovery + W5500 | STM32F407 (M4) | SPI module | ~$20 | Requires wiring |
| WeAct BlackPill + W5500 | STM32F411 (M4) | SPI module | ~$10 | Budget option |

## References

- [QEMU ARM Documentation](https://www.qemu.org/docs/master/system/target-arm.html)
- [QEMU STM32 Boards](https://www.qemu.org/docs/master/system/arm/stm32.html)
- [xPack QEMU Arm](https://xpack-dev-tools.github.io/qemu-arm-xpack/)
- [Zephyr MPS2-AN385](https://docs.zephyrproject.org/latest/boards/arm/mps2/doc/mps2_armv7m.html)
