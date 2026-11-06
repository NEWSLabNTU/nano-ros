# Analyze timing

<!-- type: how-to · level: 3 -->

**Needs first:** [Pick a scheduling model](pick-a-scheduling-model.md)

Analyze timing before you trust a sched task's period: check the callbacks it
runs for the code that breaks a time bound. This page covers the static
checks for Rust node packages, the ones a CI job can run on every push.

A callback's worst case is what matters, not its average. These patterns
make it unbounded:

| Pattern | Problem | Check |
| --- | --- | --- |
| an unbounded loop | no upper bound on execution time | Clippy |
| recursion | unbounded stack depth | Clippy, `cargo-call-stack` |
| heap allocation | non-deterministic time, fragmentation | no allocator, `disallowed-methods` |
| blocking I/O, a missing timeout | an unbounded wait | review |
| a large stack frame | stack overflow | `cargo-call-stack` |

## Before you start

- A Rust node package that builds with `nros build`.
- `rustup` with a nightly toolchain, for the stack analysis.

## Steps

1. Run Clippy with the loop and recursion lints as errors, from the node
   package's directory:

   ```bash
   cargo clippy --all-targets -- \
       -D warnings \
       -D clippy::infinite_iter \
       -D clippy::while_immutable_condition \
       -D clippy::never_loop \
       -D clippy::empty_loop \
       -D unconditional_recursion \
       -W clippy::large_stack_arrays \
       -W clippy::large_types_passed_by_value
   ```

   `infinite_iter` finds iterator chains that never end,
   `while_immutable_condition` a loop condition nothing changes, and
   `empty_loop` a `loop {}` with no body (write
   `loop { core::hint::spin_loop() }`).

2. Set the size thresholds in a `clippy.toml` beside the package's
   `Cargo.toml`:

   ```toml
   array-size-threshold = 512         # bytes of stack array
   trivial-copy-size-limit = 16       # bytes passed by value
   cognitive-complexity-threshold = 15
   ```

3. Keep the heap out of the real-time path. A `no_std` package with no
   `#[global_allocator]` cannot call `Box`, `Vec` or `String` at all; use
   `heapless` collections with a fixed capacity:

   ```rust
   let v: heapless::Vec<u8, 256> = heapless::Vec::new();
   ```

   A package that needs `alloc` elsewhere can refuse it in one module with
   `#![deny(clippy::disallowed_methods)]` and a list in `clippy.toml`:

   ```toml
   disallowed-methods = [
       { path = "alloc::vec::Vec::push", reason = "Use heapless::Vec in RT code" },
       { path = "alloc::boxed::Box::new", reason = "No heap in RT code" },
       { path = "alloc::string::String::new", reason = "Use heapless::String" },
   ]
   ```

4. Measure the stack. `cargo-call-stack` needs nightly and fat LTO
   (`lto = "fat"` in the profile), and works best on a `no_std` binary:

   ```bash
   cargo +nightly install cargo-call-stack
   RUSTFLAGS="-Z emit-stack-sizes" cargo +nightly build --release
   cargo +nightly call-stack --release > call_graph.dot
   ```

   The graph gives each function's frame size and the deepest path. Calls
   through function pointers or trait objects may be missing from it.

5. Look for recursion in the same graph. A cycle is recursion:

   ```bash
   cargo +nightly call-stack --release 2>&1 | grep -i cycle
   ```

   Replace it with a loop over a bounded stack, such as a
   `heapless::Vec<&Node, 32>`, and handle the full case.

6. Review what Clippy cannot prove. It cannot show that a loop ends or that
   a call has a timeout. Check that every `loop` has a reachable `break` or
   `return`, that a loop in a callback has an iteration limit or a deadline,
   and that every blocking call states a bound:

   - nano-ros: `spin_once(timeout)`; in C++, `wait_for_service(10000)` and
     `wait_for_action_server(10000)`. The budget is required; there is no
     wait-forever form.
   - `std` on a host: `connect_timeout`, a socket read timeout,
     `recv_timeout`.

   ```rust
   // Bounded: stop after a deadline, not when the queue happens to empty.
   let deadline = Instant::now() + Duration::from_millis(10);
   while Instant::now() < deadline {
       match queue.pop() {
           Some(msg) => process(msg),
           None => break,
       }
   }
   ```

7. Run it in CI. The job below runs the lints, the stack analysis and Miri,
   for a project with its firmware in `firmware/` and two host-testable
   crates, `my_core` and `my_msgs`:

   ```yaml
   name: Real-time static analysis
   on: [push, pull_request]

   jobs:
     clippy-realtime:
       runs-on: ubuntu-latest
       steps:
         - uses: actions/checkout@v4
         - uses: dtolnay/rust-toolchain@stable
           with:
             components: clippy
         - run: |
             cargo clippy --all-targets -- -D warnings \
               -D clippy::infinite_iter -D clippy::while_immutable_condition \
               -D unconditional_recursion -W clippy::large_stack_arrays

     stack-analysis:
       runs-on: ubuntu-latest
       defaults:
         run:
           working-directory: firmware
       steps:
         - uses: actions/checkout@v4
         - uses: dtolnay/rust-toolchain@nightly
         - run: cargo +nightly install cargo-call-stack
         - run: |
             RUSTFLAGS="-Z emit-stack-sizes" cargo +nightly build --release
             cargo +nightly call-stack --release > stack_analysis.txt
             if grep -q cycle stack_analysis.txt; then
               echo "recursion in the call graph"; exit 1
             fi

     miri:
       runs-on: ubuntu-latest
       steps:
         - uses: actions/checkout@v4
         - uses: dtolnay/rust-toolchain@nightly
           with:
             components: miri
         - run: cargo +nightly miri test -p my_core -p my_msgs
   ```

   Miri runs host tests, so point it at the crates whose logic does not
   depend on the target: serialization, state machines, protocol code.

## Example

The one command to paste for a strict local check:

```bash
cargo clippy -- \
    -D warnings -D clippy::all \
    -D clippy::infinite_iter -D clippy::while_immutable_condition \
    -D clippy::never_loop -D clippy::empty_loop \
    -D unconditional_recursion \
    -W clippy::large_stack_arrays -W clippy::large_types_passed_by_value \
    -W clippy::cognitive_complexity
```

## If it doesn't work

- **`cargo-call-stack` reports nothing useful.** The build was not fat-LTO,
  or the package links `std`. Analyze the `no_std` firmware crate.
- **Timing measured in QEMU is meaningless.** QEMU's Cortex-M3 does not
  implement the DWT cycle counter, so every read is zero, and wall-clock
  percentiles track the host's load. Measure execution times on the real
  board.
- **A lint you need does not exist.** Clippy cannot see a blocking call in a
  callback or an allocation in an interrupt handler. A Dylint library of your
  own can; see the Dylint documentation below.

## See also

- [Pick a scheduling model](pick-a-scheduling-model.md): the periods these
  checks protect.
- [Deploy to production](../deploy/deploy-to-production.md#check-readiness-for-production):
  what to measure on your silicon before shipping.
- [Clippy lints](https://rust-lang.github.io/rust-clippy/master/index.html),
  [cargo-call-stack](https://github.com/japaric/cargo-call-stack),
  [Dylint](https://github.com/trailofbits/dylint),
  [heapless](https://docs.rs/heapless).
