# Troubleshooting {#troubleshooting}

See @subpage error_codes for the full return-code reference.

## Message Too Large / Truncated

Messages pass through several buffer layers. A message must fit every
layer to be delivered intact:

| Layer | Env var | Posix default |
|-------|---------|---------------|
| Defragmentation | `ZPICO_FRAG_MAX_SIZE` | 65536 |
| Batch size | `ZPICO_BATCH_UNICAST_SIZE` | 65536 |
| Shim buffer | `ZPICO_SUBSCRIBER_BUFFER_SIZE` | 1024 |

For large messages, increase the transport limits before building:

```bash
ZPICO_FRAG_MAX_SIZE=131072 cmake --build build
```

After changing any buffer variable, clean the build cache:

```bash
cargo clean -p zpico-sys
rm CMakeCache.txt
```

## zenoh Version Mismatch

zenoh-pico and zenohd must be the same version. Symptoms:
`z_publisher_put failed: -100` (`_Z_ERR_TRANSPORT_TX_FAILED`) followed
by `-73` (`_Z_ERR_SESSION_CLOSED`).

Build zenohd from the pinned submodule (`just build-zenohd`) or install
the matching version.

## Build Issues

- **Submodule not found** — run `git submodule update --init --recursive`
- **CMake cache stale** — delete `CMakeCache.txt` and rebuild. For
  Cargo-based builds, run `cargo clean -p zpico-sys` then rebuild.
- **`NROS_PUBLISHER_SIZE` undefined** — the build did not run the
  `nros-sizes-build` probe. Re-run `cargo build -p nros-cpp` and verify
  `nros_cpp_config_generated.h` is regenerated.

## Move and Lifetime

`nros::Publisher<M>`, `nros::Subscription<M>`, etc. are non-copyable
but movable. The Rust-side handle is relocated through a dedicated FFI
call (`nros_cpp_*_relocate`) — moves are O(1) memcpy + an FFI hop.

```cpp
nros::Publisher<MyMsg> pub;
node.create_publisher(pub, "/topic");

auto pub2 = std::move(pub);   // OK — pub2 owns the handle now
// pub.publish(...) is now invalid (use is_valid() to check)
```

Storing a publisher inside a container that re-allocates (e.g.,
`std::vector` without `reserve()`) causes implicit moves; that is fine.
**Holding raw pointers** to a publisher across a move is **not** —
re-locate via the FFI re-binds the storage but external pointers stay
stale.

## Callback ABI in Mixed C++/C

Callbacks passed to the `nros::Subscription`, `nros::Timer`, and
`nros::Service` constructors are `void(*)(...)` C function pointers,
not `std::function`. Lambdas without captures decay implicitly; a
capturing lambda needs to be split into a context struct + free
function:

```cpp
struct Ctx { int count; };
static void on_tick(void* ctx_ptr) {
    auto* c = static_cast<Ctx*>(ctx_ptr);
    c->count++;
}

Ctx ctx{};
nros::Timer t;
node.create_timer(t, 1000, on_tick, &ctx);
```

`std::function` overloads are available under `NROS_CPP_STD`.

## FFI Crash on Subscription Callback

The C++ subscription invokes the user callback **on the executor
thread**. If you use a `std::function` and the function holds a
heap-allocated capture, the allocator must be safe to call from that
thread. On `no_std` / RTOS targets, prefer the freestanding C-pointer
form to keep the callback path allocator-free.

## Common Result Codes

| `nros::Code` | Meaning |
|---|---|
| `NotInitialized` | `nros::init()` was never called or returned an error |
| `InvalidArgument` | Null or empty topic/service name |
| `Timeout` | Service / action wait deadline elapsed |
| `Full` | Static pool exhausted (raise `NROS_*_BUFFER_SIZE`) |

See @ref error_codes for the complete table.
