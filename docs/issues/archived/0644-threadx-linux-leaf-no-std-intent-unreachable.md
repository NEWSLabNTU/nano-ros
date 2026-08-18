---
id: 644
title: The threadx-linux talker was declared `no_std` but had no allocator and no
  panic handler — it linked only because `env` granted `std`
status: resolved
type: bug
area: build
related: [0594, 0475]
---

## What happened

phase-359 W10 made this leaf say it is `no_std`:

```toml
# phase-359 W10 — was `["std", …]`; ThreadX is a no_std board (it just happens
# to run as a Linux process here) …
nros = { version = "*", default-features = false, features = ["alloc", "rmw-cffi", "macros"] }
```

The image kept building, so the change looked complete. It was not. `nros`'s
`env` capability listed `"std"`, something in the graph pulled `env`, and std
arrived with it — supplying BOTH lang items the leaf needs:

* `#[global_allocator]`, and
* `#[panic_handler]`.

So the manifest said `no_std` while the image was a std one. Removing the grant
(ARCHITECTURE §2 clause (a) — a capability REQUIRES the heap, it does not grant
it) made the truth visible:

```
error: no global memory allocator found but one is required;
       link to std or add `#[global_allocator]` …
error: `#[panic_handler]` function required, but not found
```

## Why the obvious fix does not work, which is the actual finding

The allocator half is easy and follows the tree's pattern — the BOARD wires it,
as `nros-board-threadx-qemu-riscv64` does:

```toml
features = ["platform-threadx", "global-allocator"]
```

The panic handler is where it stops. Adding a `#[panic_handler]` to
`nros-board-threadx-linux` (mirroring the riscv64 sibling, forwarding to the
phase-366 `nros_platform_panic` ABI) compiles the BOARD fine and still leaves:

```
   Compiling nros-board-threadx-linux v0.4.0
   Compiling threadx_linux_rs_talker v0.1.0
error: `#[panic_handler]` function required, but not found
```

Because of how the leaf is built:

```toml
[lib]
crate-type = ["rlib", "staticlib"]
```

A `staticlib` is a final artifact, so rustc demands the lang items while
compiling the LIB — and `src/lib.rs` never references
`nros_board_threadx_linux`. Only `src/main.rs` does, through `nros::main!()`.
An unreferenced rlib is not loaded, so the board's handler is invisible to
exactly the target that needs it.

That is this tree's documented DCE class, the one CLAUDE.md records for
backends: "a pure-Rust image needs the REAL backend dep — and a direct
reference, or rustc's staticlib DCE drops the dep's `#[no_mangle]` export".
Here it applies to a lang item rather than a symbol.

## Current state

The leaf names `std` again. That restores EXACTLY what the image linked before
— it was a std image throughout — rather than inventing a different one. The
comment in the manifest says so and points here.

W10's intent is recorded, not abandoned.

## Direction

Whoever owns phase-359 W10 should pick the shape; each has a real cost and none
is a manifest edit:

1. **Force the reference.** Have the leaf's `lib.rs` name the board crate so
   the rlib is loaded. Cheapest, and consistent with how backends are
   force-linked here — but it puts board knowledge in a Node package, which the
   layering deliberately keeps out.
2. **Move the handler to a crate the lib already references.** `nros` or
   `nros-platform` is loaded by definition. But the lang item belongs to the
   IMAGE (RFC-0077), and a handler in a widely-linked crate is what phase-366
   was moving AWAY from with `nros_platform_panic`.
3. **Drop `staticlib` from the leaf's crate-type** if nothing consumes it. Then
   the rlib needs no lang items and the bin gets them from the board it already
   references. Verify the C/C++ side does not link that `.a` first.

Whichever lands, the check is not "it compiles" — that is what hid this for the
length of W10. It is that the leaf builds with `nros`'s `std` feature OFF.

## Update 2026-08-16 — phase-366 W5 supplies the mechanism, not the fix

W5 landed `panic_to_platform!` in `nros` and moved the lang item onto the IMAGE
for freertos, nuttx and threadx-qemu-riscv64: "the image declares its ending;
the board stops". That is the right shape and it is what a fix here should use.

It does not close this by itself. `nros::main!()` — which is where an image
would invoke it — lives in this leaf's `src/main.rs`, and the target that fails
is the LIB: `crate-type` includes `staticlib`, so rustc demands the lang items
while compiling `src/lib.rs`, which invokes no macro and references no board.
The three shapes below are still the choice; option 3 (drop `staticlib` if
nothing consumes it) becomes the most attractive now that the bin has a
sanctioned way to declare its ending.

## Provenance

Found 2026-08-16 while removing `env`'s `std` grant to close a clause (a)
violation that had `just ci` red on main. Every other consumer that relied on
the grant needed only its manifest updated (27 of them); this one is the single
case where the dependency was on std's LANG ITEMS rather than its API, and it is
the only one that cannot be fixed by naming a feature.


## RESOLVED 2026-08-19 — the blocker was already gone, and the fix is smaller than any of the three shapes

All six threadx-linux Rust leaves now take `nros` with **`alloc`**, not `std`, and
build. That is this issue's own acceptance criterion, and `rust-rtos-link-check`
— the gate it names as failing first — passes.

### What actually unblocked it

None of the three directions. The blocker was `crate-type = ["rlib",
"staticlib"]`: a staticlib is a FINAL artifact, so rustc demanded
`#[global_allocator]` and `#[panic_handler]` while compiling `src/lib.rs`, which
is `#![no_std]` and references no board, so nothing could supply them.

phase-366 already dropped the staticlib from all six leaves — nothing consumed
it — which is direction 3, landed for its own reasons. The obligation went with
it. What remained was one word in six manifests.

### What NOT to add, both measured

The obvious completions are both wrong on THIS family, and each was tried:

* **`nros::panic_to_platform!()` in `src/main.rs` — E0152.** *"the lang item is
  first defined in crate `std` (which `talker` depends on)"*. On threadx-linux
  the Rust bin IS the Linux process entry, so `main.rs` links std for its
  runtime (crt0 + the `main` shim) and std owns the panic lang item already.
* **A board `image-runtime` feature** forwarding `nros-platform/global-allocator`,
  mirroring `nros-board-nuttx`. Builds — and is unnecessary: removing it, all six
  still build, because std's allocator serves. Installing one would reroute every
  Rust allocation through `tx_byte_allocate` in a hosted process, a runtime
  behaviour change with no defect behind it. Reverted.

The RISC-V sibling differs precisely here, which is why copying it fails: its
entry is `app_main.rs` inside a staticlib called from C, so it has no std
runtime and must declare both lang items. `qemu-riscv64-threadx/*/src/app_main.rs`
carries `nros::panic_to_platform!()` for that reason. threadx-linux is a hosted
process wearing an RTOS board's name.

So the residual `std` in these images is real and correct — it is the process
runtime, not a capability grant. What issue 0644 was actually about, `nros`'s
`env` capability GRANTING `std` (ARCHITECTURE §2 clause (a)), is closed: the
leaves no longer name it.

### Note

Building the six leaves by hand created `examples/threadx-linux/rust/*/target/`
(2.7 GiB), which `check-example-leaf-target-dirs` correctly flagged as "a writer
this gate cannot see" — it was me. Deleted. The gate works.
