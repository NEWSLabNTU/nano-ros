---
rfc: 0137
title: "Prebuilt per-board libraries, used only when their inputs match"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-578]
amends: []
supersedes: []
superseded-by: null
---

# RFC-0137 — Prebuilt per-board libraries, used only when their inputs match

**Status:** Draft (2026-11-18), for ruling. Written for phase-578 W8 ("a
shorter first build"). Nothing here is implemented; the questions at the end
are what a ruling settles.

## Problem

A user's first build compiles everything nano-ros needs for the board: the
RTOS kernel, lwIP, zenoh-pico, nano-ros's C and its Rust crates. Only the
user's own node is theirs. Measured on newslab-118 (48 cores, sccache off,
crates already downloaded), from an empty tool store, the tutorial's
`mps2-an385-freertos` talker:

| step | time |
| --- | --- |
| `nros setup mps2-an385-freertos --rmw zenoh` | 334 s (1.7 GB, mostly downloads) |
| first `nros build`, Rust talker | 64 s (124 crates; cargo 31 s) |
| first `nros build`, C talker | 84 s (181 crates; two cargo runs, 20 s and 22 s) |
| first `nros build`, C++ talker | 60 s (193 crates) |
| no-op rebuild | 3.5 to 7.6 s |

On a laptop with a quarter of those cores the build half scales towards four
to five minutes; the download half does not move. Two findings that need no
prebuilt anything:

- The C talker runs cargo twice: it builds both the `nros-c` and the
  `nros-cpp` image crates, and links the C++ one (also seen linking the TI
  image, phase-575 W3). One of the two runs is avoidable.
- `nros setup` time is downloads; a prebuilt library adds to them unless it
  replaces a source download, which this RFC's D4 makes it do.

## The hard part is trust, not caching

Copying a `.a` into the store is easy. The question is when the build may use
one instead of compiling, without ever linking an object built from inputs
the user's image does not have. The object code of every prebuilt candidate
depends on more than the board's name:

- **zenoh-pico**: its sources (the fork pin), its feature `-D`s, the pool
  sizes (`ZPICO_MAX_*`, derived per image from its declarations, RFC-0100),
  and the generated zenoh config header (issue 0135: a mismatch is a silent
  ABI break).
- **lwIP and the FreeRTOS kernel**: the board's `lwipopts.h` and
  `FreeRTOSConfig.h` (an image may raise the heap), the port, the CPU flags.
- **nano-ros's C and Rust**: the sizes header (`nros_config_generated.h`,
  per image), the cargo features, every knob in the knob registry that
  reaches a `cfg` or a `const`.

So a prebuilt piece is right for exactly one point in that input space, and
"same board, same nano-ros version" is not that point. The PX4 POSIX
archives (phase-541) are safe with a version match only because the PX4
module accepts exactly one fixed configuration (uORB-only, no networked
backend) and refuses every other; a board build has no such single point.

## Design

### D1 — A prebuilt piece is named by the digest of its inputs

Each prebuildable piece (`zenoh-pico`, `lwip`, `freertos-kernel`,
`nros-image-crate`) has a declared input set. Its **input digest** is a hash
over:

- the source trees, by CONTENT (the store's `nros-generated.toml`-style
  content stamp, RFC-0104 D5 — never an mtime and never a path);
- the toolchain's identity: the compiler's `--version` line and the index
  pin it resolved to (issue 1117's provenance line);
- the exact flags the build would pass, as the build computes them, sorted;
- the content of every generated or board header the piece includes (config
  headers, sizes headers, `lwipopts.h`, `FreeRTOSConfig.h`);
- for the Rust piece, the resolved feature set and the `Cargo.lock` slice.

The digest is computed by the same code that would drive the build, from the
same resolved inputs, so there is one derivation (CLAUDE.md "one formula is
not enough — the inputs are derived twice too", issue 1025).

### D2 — Use only on an exact match; otherwise build, and say why

At build time the CLI computes each piece's input digest. It uses a prebuilt
piece only if the store holds one under exactly that digest. Otherwise it
compiles from source, as today, and prints one line naming the first input
that differed from the nearest prebuilt (`zenoh-pico: built from source —
ZPICO_MAX_QUERYABLES is 12, the prebuilt has 8`). There is no "close enough"
and no override that forces a mismatched piece in.

This makes a prebuilt piece unable to be wrong by construction: either its
inputs equal the build's, or it is not used. Whether it is USED is what the
cache hit rate measures, which is D5's question.

### D3 — Where a prebuilt piece comes from, and why the bytes are trusted

- **Producer:** the release workflow builds the pieces for each board's
  DEFAULT image inputs (the board's knobs, no image overrides) and publishes
  them as `[prebuilt.<board>.<piece>]` rows in `nros-sdk-index.toml`:
  digest, URL, SHA-256, the same shape and integrity rule as a `dist` row
  (RFC-0099).
- **Integrity:** the store checks the SHA-256 on download, as for every
  `dist` row. A piece whose bytes do not hash to the row is not installed.
- **Correctness of the bytes for the digest:** the producer must be
  reproducible, so CI can check that a prebuilt piece equals what the source
  builds. A weekly job rebuilds a sample of published pieces from source
  under the same digest and compares the bytes; a mismatch is a red, and the
  row is withdrawn. Reproducibility needs `-ffile-prefix-map` /
  `--remap-path-prefix`, a fixed `SOURCE_DATE_EPOCH` and deterministic
  archive members (`ar D`). How close the tree is to that today is not
  measured here (Q3).
- **Pairing checks stay:** the phase-541 checks (the sizes header's variant
  symbol is defined by the archive) run on a prebuilt piece as on a built
  one.

### D4 — What a first build then compiles

With every default-input piece prebuilt, the tutorial's first build compiles
the user's node, the generated message crates and the entry, and links. The
expected saving is most of the cargo time and all of the C library time
above; the acceptance (phase-578 W8) states a measured time, by the probe.

A piece the user's image changes (a pool size, a heap) is built from source,
once, and from then on the build directory reuses it as today.

### D5 — Gates

- A fixture: a default-input image links prebuilt pieces (the build says so,
  per piece), and an image with one changed knob builds that piece from
  source and names the knob.
- The reproducibility job (D3), weekly, advisory until it has been green for
  a release, then gating the release.
- A gate that every input the build passes to a prebuildable piece is in its
  declared input set: a flag that reaches the compiler and not the digest is
  a prebuilt that can be silently wrong. Mutation-tested: add a `-D` to the
  build, and the digest must change.

## Questions for the ruling

1. **Which pieces first?** Draft: zenoh-pico and nano-ros's Rust image
   crate. They are the large compile and they are the pieces whose inputs
   are already derived in one place. lwIP and the kernel are smaller and
   depend on board headers users edit more often.
2. **Fix the C image's second cargo run first?** Draft: yes, as its own
   change. It is about 20 s of an 84 s first build, needs no prebuilt, and
   any prebuilt design must otherwise ship two image crates per board.
3. **Reproducibility before or after the first prebuilt ships?** Draft:
   before. A prebuilt whose bytes CI cannot reproduce can only be trusted
   by the release job's word, which is the trust this RFC exists to avoid.
4. **Download size.** A prebuilt piece per board per RMW adds to `nros
   setup`'s 1.7 GB unless the store then skips that piece's source. Draft:
   fetch a prebuilt piece's source lazily, only when a build needs it from
   source.

## Rejected

- **Key by board and nano-ros version only** (the phase-541 shape). It is
  right for PX4's single configuration and wrong for a board, whose images
  differ in the inputs above; it would link a pool sized for another image.
- **sccache as the answer.** It already makes the second build fast on one
  host. It does not help the first build on a new host, and its key is
  per-compiler-invocation, not reviewed or published.

## Design background

- [RFC-0097: release composition](0097-release-composition-and-version-axes.md), D13: the PX4 POSIX archives, the one prebuilt piece today.
- [RFC-0099: provisioning is planned once and prefers prebuilts](0099-provisioning-is-planned-once-and-prefers-prebuilts.md): `dist` rows, integrity by SHA-256.
- [RFC-0100: RMW-agnostic sizing model](0100-rmw-agnostic-sizing-model.md): image-derived pool sizes, the inputs that make a board-only key wrong.
- [RFC-0104: dependency pinning and the project lock](0104-dependency-pinning-and-project-lock.md) D5: content stamps, never mtimes.
