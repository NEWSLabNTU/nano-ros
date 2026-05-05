//! Thin `extern "C"` forwarders from zenoh-pico symbols to nros-platform.
//!
//! This crate is **platform-independent** — the same code works for all
//! platforms. It delegates to `nros_platform::ConcretePlatform`, which
//! resolves to the active platform backend at compile time.
//!
//! # Two independently-gated symbol groups
//!
//! - **System symbols** (clock, memory, sleep, random, time, task,
//!   mutex, mutex_rec, condvar) — gated on `feature = "active"`.
//!   Activated by zpico-sys when a platform feature is enabled and
//!   zenoh-pico's bare-metal system layer is in use. Disabled on
//!   platforms where zenoh-pico's own per-RTOS system.c file
//!   (`src/system/freertos/system.c`, etc.) provides the same
//!   symbols — there `active` stays off so we don't double-define.
//!
//! - **IVC helpers** (`_z_open_ivc`, `_z_close_ivc`, `_z_ivc_notify`,
//!   `_z_ivc_frame_size`, `_z_ivc_rx_get`, `_z_ivc_rx_release`,
//!   `_z_ivc_tx_get`, `_z_ivc_tx_commit`, `_z_ivc_tx_abandon`) —
//!   gated on `feature = "link-ivc"`. Forward to `<P as PlatformIvc>`
//!   regardless of whether the system shim is also active.
//!
//! # AGX Orin SPE (Phase 11.3.B)
//!
//! On orin-spe, `active` is OFF and `link-ivc` is ON. zenoh-pico's
//! own `system/freertos/system.c` provides every clock/mutex/condvar
//! symbol via the FSP V10.4.3 FreeRTOS API; the shim only contributes
//! the IVC link-layer forwarders.

#![no_std]

#[cfg(feature = "active")]
mod shim;

#[cfg(feature = "link-ivc")]
mod ivc_helpers;
