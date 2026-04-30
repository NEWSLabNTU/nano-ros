# nros-platform-posix

POSIX platform implementation for nano-ros. **Canonical reference port** —
every other platform crate follows the same trait-implementation pattern,
so when writing a new port, read this crate first.

## Role

Implements the trait family in
[`nros-platform-api`](../nros-platform-api) on top of standard libc:
`clock_gettime`, `malloc`/`free`, pthreads, BSD sockets,
`/dev/urandom`. Targets Linux and macOS host development; not for
embedded.

## Source layout

| File | Role |
|------|------|
| `src/lib.rs` | `PosixPlatform` zero-sized type + trait impls. |
| `src/clock.rs` | `clock_gettime(CLOCK_MONOTONIC)` for `clock_ms` / `clock_us`. |
| `src/alloc.rs` | libc `malloc` / `realloc` / `free` shims. |
| `src/thread.rs` | pthreads-backed `task_*` / `mutex_*` / `condvar_*`. |
| `src/net.rs` | TCP / UDP / multicast over BSD sockets, fully Rust-side. |
| `src/random.rs` | `getrandom`(2) entropy. |

## When to use

- Local development + CI on Linux/macOS.
- `cargo test` integration suites that exercise the same Rust trait
  surface as the embedded ports.

## See also

- [Custom Platform porting guide](../../../book/src/porting/custom-platform.md)
- [Platform API Design](../../../book/src/design/platform-api.md)
- Source on GitHub: <https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/core/nros-platform-posix>
