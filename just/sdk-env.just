# Default paths for external SDKs. Keep these repo-relative so copied
# checkouts remain position-independent; local overrides belong in `.env`.
export FREERTOS_DIR := env("FREERTOS_DIR", justfile_directory() / "third-party/freertos/kernel")
export FREERTOS_PORT := env("FREERTOS_PORT", "GCC/ARM_CM3")
export LWIP_DIR := env("LWIP_DIR", justfile_directory() / "third-party/freertos/lwip")
export FREERTOS_CONFIG_DIR := env("FREERTOS_CONFIG_DIR", justfile_directory() / "packages/boards/nros-board-mps2-an385-freertos/config")
# First-party C the board build.rs compiles (Phase 191/192.3 — was a build.rs
# `../../../packages` walk-up; centralized here so build.rs reads an env var).
export NROS_PLATFORM_CFFI_INCLUDE := env("NROS_PLATFORM_CFFI_INCLUDE", justfile_directory() / "packages/core/nros-platform-api/include")
export NROS_PLATFORM_FREERTOS_SRC := env("NROS_PLATFORM_FREERTOS_SRC", justfile_directory() / "packages/core/nros-platform-freertos/src")
export NROS_PLATFORM_POSIX_SRC := env("NROS_PLATFORM_POSIX_SRC", justfile_directory() / "packages/core/nros-platform-posix/src")
export NROS_PLATFORM_THREADX_SRC := env("NROS_PLATFORM_THREADX_SRC", justfile_directory() / "packages/core/nros-platform-threadx/src")
export NROS_LAN9118_LWIP_DIR := env("NROS_LAN9118_LWIP_DIR", justfile_directory() / "packages/drivers/lan9118-lwip")
export NROS_VIRTIO_NET_NETX_DIR := env("NROS_VIRTIO_NET_NETX_DIR", justfile_directory() / "packages/drivers/virtio-net-netx")
export NROS_C_INCLUDE := env("NROS_C_INCLUDE", justfile_directory() / "packages/core/nros-c/include")
export NROS_CPP_INCLUDE := env("NROS_CPP_INCLUDE", justfile_directory() / "packages/core/nros-cpp/include")
# Tonbandgeraet trace lib (third-party SDK; only used under NROS_TRACE=1).
export TBAND_DIR := env("TBAND_DIR", justfile_directory() / "third-party/tracing/Tonbandgeraet/tband")
export NUTTX_DIR := env("NUTTX_DIR", justfile_directory() / "third-party/nuttx/nuttx")
export NUTTX_APPS_DIR := env("NUTTX_APPS_DIR", justfile_directory() / "third-party/nuttx/nuttx-apps")
export THREADX_DIR := env("THREADX_DIR", justfile_directory() / "third-party/threadx/kernel")
export THREADX_CONFIG_DIR := env("THREADX_CONFIG_DIR", justfile_directory() / "packages/boards/nros-board-threadx-linux/config")
export NETX_DIR := env("NETX_DIR", justfile_directory() / "third-party/threadx/netxduo")
export NETX_CONFIG_DIR := env("NETX_CONFIG_DIR", justfile_directory() / "packages/boards/nros-board-threadx-linux/config")
export PX4_AUTOPILOT_DIR := env("PX4_AUTOPILOT_DIR", justfile_directory() / "third-party/px4/PX4-Autopilot")
export NROS_ESP_IDF_WORKSPACE := env("NROS_ESP_IDF_WORKSPACE", justfile_directory() / "esp-idf-workspace/esp-idf")
export NROS_ESP_IDF_ENV_SHIM := env("NROS_ESP_IDF_ENV_SHIM", justfile_directory() / "esp-idf-workspace/env.sh")
export IDF_PATH := env("IDF_PATH", NROS_ESP_IDF_WORKSPACE)
