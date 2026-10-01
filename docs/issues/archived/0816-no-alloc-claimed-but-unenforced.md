---
id: 816
title: "The book promises no-alloc integrations and nothing checks the linked
  image, so it is a claim rather than a property"
status: resolved
type: test-gap
area: tooling
related: [issue-0817, phase-391]
---

## Problem

The tree is genuinely built for heap-free operation. `alloc` is a Cargo feature
and every core crate gates on it:

```rust
#[cfg(feature = "alloc")]
extern crate alloc;
```

(`nros-core`, `nros-rmw`, `nros-node`, `nros-serdes`, `nros` — all the same
shape.) The book then makes the property a promise:

- `book/src/user-guide/embassy-integration.md:81` — *"The no-alloc contract."*
- `book/src/user-guide/embassy-integration.md:336` — *"fully no-alloc."*
- `book/src/internals/dispatch-strategy.md:171` — *"the no-alloc +
  framework-task-routed"* constraint
- `book/src/concepts/no-std.md:146` — the parameter API *"works without alloc"*

**Nothing verifies any of it.** No lane `nm`s a built image and asserts the
absence of allocation symbols. The feature gates are necessary and are not
sufficient: a vendored C dependency, a weak-symbol fallback, or one
`extern crate alloc` added under a default-on feature reintroduces the heap
with no diagnostic, and the book keeps promising otherwise.

## Evidence that this class of drift is real

[Issue 0817](0817-platform-funnel-bypassed-in-zephyr-port.md) found
sixteen allocation sites in the Zephyr platform port that bypassed
`nros_platform_alloc` and called `k_malloc` directly. They compiled, linked,
ran, and passed every lane for as long as they existed. A source grep is what
eventually found them, and a source grep cannot see vendored C — zenoh-pico
alone reaches the allocator from 42 call sites.

## Shape of the fix

A link-time symbol gate, not a source check: `nm` the image and deny
`malloc`/`calloc`/`realloc`/`free`/`k_malloc`/`k_free`/`pvPortMalloc`/... with
a per-tier strictness:

| tier | rule |
| --- | --- |
| `heap-free` | deny every allocation symbol, no exceptions |
| `unified` | allow them only inside the `nros_platform_*` backend objects |

That turns both the book's promise and RFC-0034 D6's single-funnel rule into
things a build can fail on. Owned by
[phase 391](../../roadmap/archived/phase-391-allocation-unification-and-tier-model.md).

## What landed: `scripts/check-no-alloc-image.py`

The instrument exists. It reads a built artifact's symbol table and denies four
allocation families — `c-heap`, `rust-alloc-shim`, `cxx-operator-new`,
`rtos-heap` — each justified in the script's docstring. `--selftest` proves it
fails in every direction (each family alone is enough to fail; the near-miss
symbols this tree really contains — `freeaddrinfo`, `z_free`,
`_z_string_preallocate`, `nros_platform_alloc` — must not fire; and each of the
four vacuous shapes exits 2 rather than green).

The two tiers read different inputs, and that is not a shortcut:

- **`heap-free`** reads the linked ELF and denies everything.
- **`unified`** is a rule about *which object* reaches the allocator, and a
  linked image has already discarded that — after `ld` there is one `U malloc`
  for the whole program. So it reads `--objects` (per-object undefined
  symbols), and it *refuses* an ELF rather than returning a green it cannot
  justify.

Two findings from running it that this issue's text did not anticipate:

1. **Symbol tables need `U`, and the sibling tool deliberately drops them.**
   `scripts/nros-mem-report.py` passes `--size-sort`, which omits sizeless
   symbols — and a call into libc's `malloc` is exactly a sizeless `U malloc`.
   A gate built on that reader would pass every dynamically linked image in the
   tree. This tool reads the full table, both directions.
2. **Whole-name matching missed the entire Rust family on a real binary.**
   Current rustc emits the allocator ABI inside a synthetic `__rustc` crate:
   `__rustc::__rust_alloc` demangled, `_RNvCs..._7___rustc12___rust_alloc`
   mangled, and the shim is now `__rust_no_alloc_shim_is_unstable_v2`. The
   first run reported five `c-heap` symbols and zero Rust ones from a binary
   containing ten. That family is now matched as a substring — the one
   documented exception to the whole-name rule.

## What this issue got wrong: there is no image to point it at

The issue says nothing verifies the no-alloc promise. The stronger fact is that
**no image in the tree is configured no-alloc at all**, so there is currently
nothing for the `heap-free` tier to pass. Every bare-metal Rust leaf —
including every RTIC one, which is where the dispatch-strategy claim lives —
enables the `alloc` feature:

```
$ for f in examples/mps2-an385-baremetal/rust/*/Cargo.toml; do grep -c '"alloc"' $f; done
# 1 for all 13, incl. talker-rtic, listener-rtic, {service,action}-{server,client}-rtic
```

And there is no Embassy example at all: no `examples/**/embassy*` directory and
no `fixtures.toml` row that builds one. `check-no-alloc-image.py --claims`
prints this roster with a per-claim reason; today it reads **0 of 4 backed**.

So the remaining half of this issue is not tooling. It is a fixture: one
bare-metal entry built with `default-features = false` and no `alloc`, given a
`fixtures.toml` row, which the gate then holds at `heap-free` forever. Until
that exists the tool can only be wired at the `unified` tier.

## What the `unified` tier already finds

Run against a built cyclonedds leaf's RMW objects it reports ten allocator
references that bypass `nros_platform_alloc` — `operator new`/`operator delete`
in `publisher.o`, `subscriber.o`, `session.o`, `service.o`, plus `calloc`/`free`
in `service.o`. Two caveats, both real:

- The `calloc`/`free` pair is in an object dated three weeks *before* the source
  that stopped calling them (`service.cpp` now uses `ddsrt_calloc`, with a
  comment saying so). A stale artifact yields a stale verdict — every report
  prints the artifact's mtime for exactly this reason.
- The `operator new`/`operator delete` references are current, and are the same
  class as issue 0817 one backend over.

The zenoh side is clean and demonstrates the funnel working end to end: 96
objects scanned across `libzenohpico.a` + the zpico shim, zero bypasses, because
zenoh-pico routes through `z_malloc`/`z_free` and
`libzpico_platform_aliases.a` forwards those to `nros_platform_alloc`.

## Still open

- No `just` recipe is wired yet (the lane belongs with the fixture, above).
- `--claims` is a listing, not a gate — wiring it as one would red-line every
  lane until the no-alloc fixture exists.

## Resolution (2026-10-01)

The fixture this issue named as its remaining half had already landed
(phase-391 W5-endgame): `heap-free-poc-mps2` builds `nros` without `alloc`, and
`just ci l3` runs `check-no-alloc-image.py --tier heap-free` on the linked ELF
(also the depth of `just ci matrix build`). What was still missing was the
other end: the book claims were not connected to it, `--claims` was a listing
that always exited 0 and still reported "0 of 4 backed", and it ran only in
`build-serial` (schedule-only) while its own comment said "fast line".

Done:

1. **The image covers more of what the book says.** `heap-free-poc-mps2` now
   also links `spin_some`/`spin_forever` and the core `ParameterServer`
   (`new_in`/`declare`/`set`/`get` over a caller-owned `ParameterStorage<4>`),
   printing the value read back so it cannot be optimised out. Measured:
   `--tier heap-free` OK, symbols read 552 (was ~460); QEMU mps2-an385 prints
   `HEAP-FREE-POC: parameter rate_hz = 42` then the open-refused marker.
2. **`--claims` is a gate.** It harvests every `book/src/**/*.md` line with
   no-alloc wording (`no-alloc`, `heap-free`, `without alloc`, `no heap`,
   `fully static allocation`, `allocation-free`, `zero allocations`); each must
   be a roster row, matched by TEXT (two of the old four line numbers had
   drifted). A row of kind `image` must name an image the L3 recipe actually
   runs through `--tier heap-free` — read from `just/ci.just`, so it cannot go
   stale toward OK. Other rows (`type-level`, `rationale`, `disclaimer`,
   `tool`) carry a written reason. There is no `unbacked` kind. The selftest
   proves each failure direction. Moved onto the fast line.
3. **The book now says what is checked.** Embassy's "no-alloc contract" and
   dispatch-strategy's "no-alloc + framework-task-routed contract" now name the
   checked image and what it does NOT cover (no Embassy image is built; every
   shipped transport allocates in C). `two-layer-api.md` dropped "zero
   allocations". Three XRCE sentences ("fully static allocation, no heap
   required" twice, "heap-free" in the porting guide) were FALSE — every XRCE
   session and entity is a `nros_xrce_calloc` through `nros_platform_alloc` —
   and now say it allocates at open/create, never per message.

Measured before/after: `--claims` before = "0 of 4 backed", exit 0 always;
after = "OK — 8 image promise(s) … 12 other line(s) classified", exit 0; with
L3's `--tier heap-free` line removed = 9 problems, exit 1; a book line
"We are fully no-alloc." added = UNROSTERED, exit 1.

Sweep: `grep -rniE 'no-alloc|without alloc\b|heap-free|\bno heap\b|fully
static (memory )?allocation|allocation-free|zero allocations' book/src`.

NOT done / not measured:

- The `unified` tier is still not wired into a lane (cyclone's `operator new`
  in four TUs, as recorded above) — unchanged scope, still needs a fix or an
  `--allow` with a tracked id.
- No Embassy or RTIC image is built heap-free; the book now says so rather
  than implying it.
- The phrase set is a declared list; wording outside it ("no allocator",
  e.g. the generated rmw-api-comparison page) is not harvested.
- `scripts/rmw-alloc-sites.py` reports no XRCE row at all because it does not
  see the `nros_xrce_calloc` helper — filed as [issue 1605](../1605-rmw-alloc-sites-blind-to-xrce-helper.md).
