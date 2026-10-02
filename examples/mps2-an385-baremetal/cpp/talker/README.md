# Bare-metal C++ talker (MPS2-AN385, no RTOS)

Publishes `std_msgs/String` `"Hello World: N"` on `/chatter` once a second from
an ordinary nano-ros **C++** application on a Cortex-M3 with **no RTOS at all** —
the same program as `examples/mps2-an385-freertos/cpp/talker/`.

Same shape as [`../../c/talker`](../../c/talker/README.md), whose README explains
why: the link root is Rust (`src/main.rs` boots the board and calls
`app_main()`), and `build.rs` compiles `src/talker.cpp` into it. Three things are
C++-specific:

* **The message code is half Rust.** `nros generate cpp` emits header-only C++
  types plus their CDR serializers as Rust (`*_types.rs` / `*_exports.rs`). On
  the cmake road a per-package staticlib wraps them; here `build.rs` writes one
  `include!` per file and `src/main.rs` compiles them into the binary, under the
  small prelude they expect (`nros_serdes`, `fixed_str`, `nros_cpp_publish_raw`).
  `nros generate cpp` gained its standalone (`package.xml`) front door for this
  leaf; it runs the same emitter the cmake args-file path does.
* **No C++ runtime.** `-ffreestanding -fno-exceptions -fno-rtti
  -fno-threadsafe-statics -fno-use-cxa-atexit`, and no `-lstdc++`. The
  freestanding flag is also what keeps `nros-cpp`'s hosted-STL surface out —
  each such include is behind `__STDC_HOSTED__` AND `__has_include`.
* **No namespace-scope C++ objects.** `cortex-m-rt` runs no `.init_array`, so a
  global with a constructor would be used unconstructed; the node, publisher
  and timer are locals of `nros_app_main`, which never returns.

`nros-codegen.toml` bounds `std_msgs/String.data` (256), as the FreeRTOS
sibling's does.

## Build

```sh
source ./activate.sh
nros sync examples/mps2-an385-baremetal/cpp/talker
cd examples/mps2-an385-baremetal/cpp/talker && cargo build --release
```

## Status

Build-only (issue 1512): it boots in QEMU as far as the session open
(`nros::init failed` after `ConnectionFailed`), and has no runtime lane until
the entry locator reaches a cargo-rooted image. `cpp/` has the talker only; the
other five roles are mechanical from here.
