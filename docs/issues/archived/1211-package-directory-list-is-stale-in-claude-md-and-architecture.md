---
id: 1211
title: "CLAUDE.md and ARCHITECTURE §1 name four `packages/` directories that do
  not exist and omit four that hold 38 crates, including the whole platform and
  API layers"
status: resolved
type: docs
area: [docs, core, platform]
related: [1210, 0001]
---

## What

Two places state the workspace layout, identically and wrongly.

`CLAUDE.md:72`:
> Workspace: `packages/{core,zpico,xrce,dds,boards,drivers,interfaces,testing,verification,reference,codegen,cli}/`

`docs/design/ARCHITECTURE.md:16-17`:
> nano-ros is a `no_std` ROS 2 client. Crates live under `packages/{core,zpico,xrce,dds,boards,drivers,interfaces,testing,verification,reference,codegen,cli}/`.

On disk (`ls packages/`):

```
api  boards  cli  core  drivers  interfaces  platform  reference  rmw  testing  tooling  verification
```

| documented, absent | present, undocumented | crates |
| --- | --- | --- |
| `zpico` | `api` | 3 (`nros`, `nros-c`, `nros-cpp`) |
| `xrce` | `platform` | 14 |
| `dds` | `rmw` | 17 |
| `codegen` | `tooling` | 8 |

The three transport dirs collapsed into `packages/rmw/{zenoh,xrce,cyclonedds}/`
and `packages/codegen` was retired into `packages/cli/` — CLAUDE.md itself says
so 30 lines later ("The retired `packages/codegen` submodule is fully gone"), so
the file contradicts itself on the same page.

## Why it matters

This is not a cosmetic path list. The four undocumented directories are the ones
that answer the questions the layer map exists for:

- **`packages/api/`** holds the three user-facing language surfaces. ARCHITECTURE
  §2's own agnosticism contract names `nros`, `nros-c`, `nros-cpp` at
  `:166` as a governed class, and §1 gives a reader no directory to find them in.
- **`packages/platform/`** holds all 14 platform crates *and* the five pure-C
  RTOS ports (`nros-platform-{posix,zephyr,freertos,threadx,esp-idf}` have no
  `Cargo.toml` at all). §1 describes the platform layer in prose one line below
  the wrong path list, so the reader is told the layer exists and pointed away
  from it.
- **`packages/tooling/`** is 8 build-support crates and appears nowhere.

CLAUDE.md's own instruction ("Run `ls packages/` for the current crate list") is
the tell: the line was already known to be untrustworthy and was patched with a
workaround instead of a correction.

RFC-0001, the doc both defer to as "the canonical layer/crate map", has no
textual crate table at all — its layer map is four mermaid diagrams
(`docs/design/0001-architecture-overview.md:18,73,675`), and those are stale in
the same direction: the "nano-ros Core Library Stack" block at `:82-91` places
`nros-c` and `nros-rmw-cffi` inside core, while on disk they live in
`packages/api/` and `packages/rmw/cffi/`. So a reader who follows the pointer
from CLAUDE.md to ARCHITECTURE §1 to RFC-0001 gets three stale answers and no
authoritative list.

`AGENTS.md:5` is the only one of the four that is close to right — it names
`packages/core/`, `packages/rmw/{zenoh,xrce,cyclonedds}/`, `packages/boards/`,
`packages/drivers/`, `packages/testing/nros-tests/` — but it too omits `api`,
`platform` and `tooling`.

## Fix sketch (not applied)

1. Correct the literal list in both `CLAUDE.md:72` and
   `docs/design/ARCHITECTURE.md:16-17` to the 12 directories on disk.
2. Give RFC-0001 a **textual** crate-to-layer table so there is one authoritative
   answer that a grep can check, rather than four diagrams that drift silently.
   The diagrams can stay; they should not be the only statement.
3. Consider a gate: the list is a fixed set of directory names checked against
   `ls packages/`, which is a three-line script and the cheapest possible
   ratchet. Absent that, this recurs the next time a directory moves — it has
   already survived at least the `codegen` retirement and the
   `zpico`/`xrce`/`dds` consolidation.

## Resolution

Re-measured 2026-09-10 before fixing: `ls packages/` gives the same 12
directories the issue reports, so the survey held. Every stale name found is
corrected, and the class is gated rather than the sites patched.

**Corrected.**

- `CLAUDE.md` and `docs/design/ARCHITECTURE.md` §1 now name the 12 directories
  on disk. The "run `ls packages/` for the current crate list" workaround is
  gone — it was the tell that the line was known untrustworthy — and §1 now
  says where the platform ports and the three language surfaces live, which was
  the reader-facing cost.
- `ARCHITECTURE.md` §2's agnosticism contract named **`nros-orchestration`**, a
  crate that has never existed under that spelling. The list is replaced by
  "every crate under `packages/core/`", which is both true and one fewer
  enumeration to drift (issue 1212).
- `CLAUDE.md`'s RFC-0054 entry pointed at
  `packages/core/{nros-rmw-cffi,nros-platform-cffi}/src/generated.rs` — a
  directory neither has lived in — and named two of the **three** surfaces
  `gen-abi-bindings.sh` writes. All three real paths are now listed.
- RFC-0001 twice cited `packages/codegen/interfaces/` for what is
  `packages/interfaces/`.

**Gated.** `check-package-directories` (fast line, buildless, self-testing)
parses the one `packages/{...}/` brace list in each of CLAUDE.md and
ARCHITECTURE §1 and compares it against `ls packages/`, in both directions — a
missing directory hides a layer, an extra one sends a reader after something
that is not there, which is how `packages/codegen` outlived its own retirement
note by two moves. Mutation-tested: dropping `platform` and adding `codegen`
fails with `omits platform; names codegen (no such directory)`.

**RFC-0001 gained the textual map** the fix sketch asked for: a
"Directory map" table saying what each of the 12 directories holds, because
four mermaid diagrams cannot be grepped, read in a diff, or checked. The
diagrams stay, with the one that draws `nros-rmw-cffi` and `nros-c` inside the
core stack relabelled — that grouping is correct as a LAYER and was being read
as a directory claim.

Not addressed here: `AGENTS.md:5` describes the layout in prose rather than a
brace list, so the gate does not reach it; it is incomplete rather than wrong.
