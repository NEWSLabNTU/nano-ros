# Phase 369 — ThreadX-RV64 gets ONE entry point and ONE build path

**Status (2026-08-19). W1-W3 LANDED; W4 in flight; W5 CLOSED as not-applicable; W6 is a decision.** Direction chosen: the **Zephyr shape** —
both RMWs build through CMake, the lib-side entry is the only entry, and
`src/main.rs` goes away.

**Owns:** [issue 0666](../issues/0666-threadx-zenoh-and-cyclonedds-build-paths-diverge.md),
[issue 0668](../issues/0668-threadx-rv64-example-shape-differs-from-every-other-standalone.md).
**Unblocked by:** 0674 → 0678 → 0692, now closed end to end; 0692's fix removed
the panic collision that made this urgent, so this is consistency work with a
measured cost, not a firefight.

## Why

Six `examples/qemu-riscv64-threadx/rust/*` leaves are the only standalone
examples in the tree that own TWO entry points and TWO build paths:

```
src/main.rs      nros::main!()                        -> the bin  (cargo, zenoh)
src/app_main.rs  <board>::app_main!(crate::register)   -> the .a   (CMake, cyclone)
                 nros::panic_to_platform!()            <- the extra line
```

The RMW picks which path runs. Issue 0666 records the cost in one sentence, and
0688 paid it: **a bespoke build path silently misses machinery the shared one
applies, and the failure surfaces four crates away from its cause** — nothing
gave `nros-c` a panic policy, because the seam is not `nano_ros_entry()`.

## Why the Zephyr shape and not cargo-only

0668 says either single-entry shape is fine. The cargo-only shape needs
CycloneDDS reachable from a cargo build on a bare-metal riscv64 target.
`packages/rmw/cyclonedds/cyclonedds-sys/build.rs` does drive CMake and produce a
static `libddsc.a`, so it is not hypothetical — but it is wired only into
`nros-board-linux`, and making it cross-compile for bare metal is a project, not
a cleanup.

The Zephyr shape needs no new cross-compilation capability: the CMake path
already exists and already builds five of the six leaves' RMW today.

**Cost, stated up front:** the pure cargo build is the faster inner loop and the
one contributors reach for. This trades it for one shape. If that loop matters
more than the uniformity, the decision should be revisited BEFORE W3 — after
that the `[[bin]]` is gone.

## Waves

**W1 — make the seam RMW-neutral.** `nros_threadx_rv64_rust_cyclone_app()`
hardcodes `FEATURES rmw-cyclonedds alloc`. Take the feature from the RMW
selection instead and rename to `nros_threadx_rv64_rust_app()`. The leaf
CMakeLists already parameterise `NROS_RMW` as a cache variable, so this is the
only thing standing between them and a zenoh configure.
*Acceptance:* one leaf configures and links with `-DNROS_RMW=zenoh` through
CMake, producing the same ELF the cargo path produces today.

**W2 — move the six zenoh rows onto the CMake path.** `examples/fixtures.toml`'s
six `rmw=zenoh` rows for `qemu-riscv64-threadx/rust/*` change from cargo rows to
cmake rows; `just threadx_riscv64 build-examples` stops calling
`fixtures-build.sh threadx-riscv64 rust` for them.
*Acceptance:* `just threadx_riscv64 build-fixtures` builds all twelve rows
(6 zenoh + 6 cyclone) through CMake, from a WIPED tree — see the note below.

**W3 — delete the second entry.** Remove `src/main.rs` and the `[[bin]]` section
from all six `Cargo.toml`. `crate-type` stays `["staticlib", "rlib"]`.
*Acceptance:* `git grep -l "nros::main!" examples/qemu-riscv64-threadx` is empty,
and the leaves still build.

**W4 — follow the binaries.** The test-side resolvers move from the cargo target
dir to the CMake build dir (`nros_tests::fixtures::binaries::threadx_riscv64`).
*Acceptance:* the threadx-riscv64 runtime tests resolve and run, or skip for a
stated reason that is not "binary not found".

**W5 — drop the hand-written panic line. NOT APPLICABLE; premise refuted.**

The wave assumed 0668's prediction: "these six lose their last hand-written
panic line — the entry macro emits it like everywhere else." That holds for
`nros::main!`, which does emit one. It does NOT hold for the entry that
survives, because W3 deleted `main.rs` and the remaining entry is the board's
`app_main!`, which emits only the extern function:

```rust
macro_rules! app_main {
    ($register:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn app_main() -> ! { $crate::run_app_thread($register) }
    };
}
```

So `nros::panic_to_platform!()` is not redundant — after W3 it is THE provider
for these images. Removing it would leave the leaf staticlib, a FINAL artifact,
with no handler and reproduce `#[panic_handler] function required` exactly as
issues 0688/0692 did.

Established by reading the macro, before running anything: the phase doc's own
instruction to VERIFY rather than assume is what caught it.

**No change. The line stays, and the reason is now written down** — that is this
wave's whole output, and it is worth more than the deletion would have been.

**W6 — should `app_main!` emit the ending at all? (design, then decide)**

0668's underlying wish — no hand-written panic line — is reasonable and is NOT
delivered by W5. Delivering it means making `app_main!` emit the handler, which
is a different change with a wider question behind it.

*The question is not mechanical.* RFC-0077 separates three concerns:
implementation is the PLATFORM's, installation is the LINK ROOT's, and **policy
is the IMAGE's**. A hand-written `nros::panic_to_platform!()` in the image's own
entry file IS the image declaring its policy — which is what phase-366 spent a
whole phase pulling OUT of libraries. Making a board macro emit it implicitly
moves that choice back INTO a library.

| option | for | against |
| --- | --- | --- |
| **A. leave it** (today) | the image states its ending explicitly, which is RFC-0077's shape; zero machinery | six hand-written lines that can drift; differs from every other family's look |
| **B. `app_main!` emits unconditionally** | uniform with `nros::main!`; six lines gone | a library choosing a policy it cannot know — issue 0618's exact complaint; `panic = "own"` becomes unexpressible, and an image wanting its own handler must fight the macro |
| **C. `app_main!` takes the policy** — `app_main!(crate::register)` defaults to platform, `app_main!(crate::register, panic = own)` emits none | removes the separate line AND keeps the choice at the image; mirrors `nros::main!(panic = …)` | one more macro arm to maintain; the policy is still spelled in Rust rather than reaching from `nano_ros_entry`, because this seam has no entry call |

**Recommendation: C**, or A if nobody minds the six lines. B is the one to avoid;
it is the pattern issues 0618 and 0692 both punished.

*Blast radius, measured:* `app_main!` has exactly six consumers, all of them
these leaves. Whatever is chosen is atomic with removing (or keeping) their
`panic_to_platform!()` — the two cannot land separately without a window where
the image has two providers or none.

*Acceptance, whichever is chosen:* exactly one GLOBAL `rust_begin_unwind` in the
final image, checked with `nm`, as 0692 did — that issue showed two handlers can
coexist by symbol locality (`t` vs `T`), so the count is a link-level fact and
not inferable from the source.

## Do not

* **Do not test incrementally on this board.** `CMAKE_C_COMPILER` is sticky;
  issue 0678 spent a day looking unfixed because 22 stale `build-*` dirs kept
  resolving Debian's compiler. Wipe the build dirs before believing any result
  here, green or red.
* **Do not conflate the allocator.** `#[global_allocator]` has the same shape
  problem in the same files; 0616/0594 own it. This should not make it worse and
  should not try to fix it.
