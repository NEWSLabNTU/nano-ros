# Design Overview

This section explains *why* nano-ros has the shape it does. It is not a tutorial (see [Getting Started](../getting-started/installation.md)) and not an API reference (see the [Reference](../reference/rust-api.md) chapters). It is the rationale a contributor needs to evaluate proposed changes, and the context a porter needs to make judgment calls when the [Porting](../porting/overview.md) guides don't cover their case.

Three design choices shape almost everything else:

## 1. The RMW layer was rewritten, not adopted

ROS 2's `rmw.h` assumes a libc heap, an OS scheduler with preemptable threads, dynamic loaders, and middleware-owned background dispatch. None of those hold on a Cortex-M3 with 64 KB of RAM. nano-ros defines its own RMW abstraction (`nros-rmw`) that pushes I/O buffers to the caller, replaces wait sets with explicit `drive_io()`, drops dynamic graph discovery, and selects the backend at compile time.

→ [RMW API Design](rmw.md)

## 2. Future/Promise unifies blocking and async, with no internal spin

rclcpp owns the spin loop (`rclcpp::spin(node)` blocks the thread it was called on); rclc has no future/promise type at all. Neither model works on a single-threaded MCU. nano-ros uses a single `Promise<T>` value that supports three patterns -- callback-driven polling, blocking-with-timeout, and `.await` -- and *never* owns the spin loop. Every blocking helper takes the executor as an argument and drives it internally.

→ [Client Library Model](client-library.md)

## 3. Platform traits are independent and contract-first

zenoh-pico needs a heap, threading, networking, and a clock. XRCE-DDS only needs a clock and four C function pointers. Bare-metal has no scheduler. Each capability is a separate trait the platform may stub if unsupported. Every method has an explicit behavior contract: blocking? may-fail? unsupported-fallback?

→ [Platform API Design](platform-api.md)

## How to read this section

If you are evaluating a *new* feature: start with the design page closest to the change (RMW for transport-level changes, Client Library for executor or callback-shape changes, Platform API for OS-primitive changes). Each page lists the constraints the current shape satisfies; a new design must satisfy them or argue for relaxing one.

If you are *porting* nano-ros to new hardware or a new transport: the [Porting](../porting/overview.md) chapters tell you *what* to implement. Read the matching design page to understand *why* each trait exists, which is what you need when the porting guide leaves a judgment call to you.
