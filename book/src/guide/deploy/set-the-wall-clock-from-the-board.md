# Set the wall clock from the board

<!-- type: how-to · level: 3 -->

**Needs first:** [Configure an image](../build/configure-an-image.md)

Take the wall clock from the board when it already knows the time, from an
RTC, a GNSS receiver or its own SNTP task, and messages should be stamped
with it.

An image has one wall clock. `Clock::system()`, `rclcpp::Clock::now()` and
the executor all read it, and `[knobs.clock] source` says where it comes
from. With `source = "board"`, nano-ros asks a function your board or
application defines, `nros_board_wall_clock_ns`, until it answers.

## Before you start

- An image on Linux (POSIX) or FreeRTOS, bare metal or `freertos-posix`.
  Zephyr, ThreadX and NuttX images refuse `board` when they build.
- Somewhere the board keeps the time: an RTC, a GNSS fix, or a task of
  its own that syncs it.

## Steps

### Select the board

Select the board as the image's wall-clock source in `system.toml`:

```toml
[image.robot.knobs.clock]
source = "board"
```

To fall back to the platform's own clock when the board never answers,
list both, in order:

```toml
[image.robot.knobs.clock]
source = "board,platform"
```

### Define the hook

Define the hook in a package the image launches. It returns `true` and
writes nanoseconds since the Unix epoch once the board's clock is set,
and `false` until then. nano-ros asks it every 10 ms, so it must return
at once: read a value your RTC driver or sync task keeps, and never wait
on the hardware or the network here.

<!-- tabs -->

#### Rust

```rust
// src/robot_board/src/lib.rs
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_board_wall_clock_ns(out_ns: *mut u64) -> bool {
    // `board_time_ns` is yours: the time your RTC or sync task holds,
    // or None until it is set.
    let Some(now) = board_time_ns() else { return false };
    if out_ns.is_null() {
        return false;
    }
    // SAFETY: non-null, and nano-ros passes a pointer valid for a u64.
    unsafe { out_ns.write(now) };
    true
}
```

nano-ros calls the hook from C, after your crate on the link line, so
the linker can leave out a Rust function nothing in Rust reaches. Name
it once in the component's `register` to keep it in:

```rust
core::hint::black_box(
    nros_board_wall_clock_ns as unsafe extern "C" fn(*mut u64) -> bool,
);
```

#### C

```c
/* src/robot_board/src/board_clock.c */
#include <stdbool.h>
#include <stdint.h>

bool nros_board_wall_clock_ns(uint64_t *out_ns) {
    uint64_t now;
    /* board_time_ns is yours: false until your RTC or sync task has a time. */
    if (out_ns == NULL || !board_time_ns(&now)) {
        return false;
    }
    *out_ns = now;
    return true;
}
```

#### C++

```cpp
// src/robot_board/src/BoardClock.cpp
#include <cstdint>

extern "C" bool nros_board_wall_clock_ns(std::uint64_t* out_ns) {
    std::uint64_t now = 0;
    // board_time_ns is yours: false until your RTC or sync task has a time.
    if (out_ns == nullptr || !board_time_ns(&now)) {
        return false;
    }
    *out_ns = now;
    return true;
}
```

<!-- /tabs -->

Put it in an object file of the image, or in an archive linked after
nano-ros. A board that ships it in a static archive under CMake adds that
archive to the image's `LINK_DEPENDS`.

### Build and run

Build and run the image. On its first wall-clock read it prints where the
clock came from:

```text
nros: wall clock from board
```

### Check the clock is set

Before publishing data whose stamp matters, check that the clock is set.
`wall_clock_state()` says whether it is synced and which source answered:

<!-- tabs -->

#### Rust

```rust
if let Some(st) = nros::wall_clock_state() {
    if st.synced {
        // publish stamped data
    }
}
```

#### C

```c
nros_wall_clock_state_t st = rclc_wall_clock_state();
if (st.synced) {
    /* publish stamped data */
}
```

#### C++

```cpp
const nros_wall_clock_state_t st = ::rclcpp::Clock::wall_clock_state();
if (st.synced) {
    // publish stamped data
}
```

<!-- /tabs -->

## When the board never answers

The source is resolved on the image's first wall-clock read, and that read
waits for it. The hook is asked every 10 ms for up to
`clock.fallback_settle_ms` (10 s by default). The first `true` sets the
clock; after that the hook is never called again.

If the window runs out, the next source in the list is tried. If none
answers, `clock.epoch_failure` decides:

| `epoch_failure` | The image prints | Then |
| --- | --- | --- |
| `unstamped` (default) | `nros: no wall-clock source answered (board, platform); stamps are unset (header.stamp reads 0)` | runs on |
| `halt` | `nros: no wall-clock source answered (board, platform); halting, as clock.epoch_failure = halt asks` | stops |

The list in parentheses is the sources the image tried, in order.

```toml
[image.robot.knobs.clock]
source = "board,platform"
fallback_settle_ms = 30000   # a GNSS fix can take longer than 10 s
epoch_failure = "halt"       # peers here refuse an unstamped sample
```

## Which stamps follow it

Choosing a source changes the clock nano-ros reads. A sample's source
timestamp is stamped by the RMW backend, and only zenoh asks nano-ros:

| RMW | A sample's source timestamp |
| --- | --- |
| zenoh | the image's wall clock, from the board |
| Cyclone DDS | the host's `CLOCK_REALTIME`, whatever the image selected |
| XRCE-DDS | 0: the client delivers no source timestamp |

A `header.stamp` your code writes from `Clock::system()` or
`rclcpp::Clock::now()` follows the board on every RMW.

## If it doesn't work

- **The link fails on `nros_board_wall_clock_ns`.** nano-ros has no default
  hook: an image that selects `board` must define it. Define it in a package
  the image launches, or, if it is in an archive, link that archive after
  nano-ros.
- **The clock reads 0 on FreeRTOS.** A read before the scheduler starts has
  no clock yet. Read the time from a task.
- **A C or C++ image refuses `board` at configure.** For now, an image whose
  components build their own executables cannot select it. Use a Rust image,
  or components that build no executable.
- **The image refuses `board` at build.** It is a Zephyr, ThreadX or NuttX
  image; only POSIX and FreeRTOS ports take a board clock for now.

## See also

- [Use simulated time](use-simulated-time.md): timers that follow `/clock`
  instead of the wall.
- [Configuration surface](../../reference/configuration-surface.md): every
  `clock.*` knob.
- [Platform API](../../reference/platform-api.md): the wall-clock calls a
  port provides.
