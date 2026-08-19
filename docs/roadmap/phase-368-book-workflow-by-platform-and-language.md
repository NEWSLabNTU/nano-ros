# Phase 368 — What a reader actually runs, per platform and per language

**Status (2026-08-19).** IN PROGRESS — W1, W2, W3, W5, W6 landed; W4 planned.

Implements the fix for [issue 0694](../issues/0694-platform-starters-omit-nros-sync.md).

## The finding

The book documents the current workflow **only where a Linux reader lands**.
`nros sync` — the step without which no Rust leaf can even be parsed by cargo —
appears on 13 of 107 pages, and none of the 12 platform starters is one of them.
Those 12 pages carry 26 build commands between them, and every Rust one of those
26 fails on a fresh clone with an error whose four frames never name the missing
step.

The gap is not that the pages are old. `user-guide/workflow.md` was touched
yesterday and still presents `nros metadata` / `nros plan` / `nros check` as the
multi-component flow, with no `nros sync` anywhere. Recency of edit is not
currency of content.

## The organizing fact

The tree already answers "what do I run?" precisely, and the answer is keyed on
the **builder**, which follows from the language — not on the platform. From the
413 rows of `examples/fixtures.toml`:

| builder | languages | platforms it appears on |
|---|---|---|
| `cargo` | rust, mixed | every one (linux, freertos, nuttx, nuttx-riscv, threadx-linux, threadx-riscv64, zephyr, esp32, both baremetals) |
| `cmake` | c, cpp, mixed, rust | linux, freertos, nuttx, nuttx-riscv, threadx-linux, threadx-riscv64, zephyr |
| `west` | c, cpp, rust | zephyr, zephyr-cortex-m |

And the sync requirement partitions on exactly that seam: a tracked leaf
`.cargo/config.toml` — the file whose `include` is the trap — exists under
`rust/` leaves and nowhere else. So:

- **Rust (cargo, including cargo under west)** — `nros sync` before the first
  build, on every platform.
- **C / C++ (cmake)** — no sync. Codegen runs inside cmake via
  `nros_find_interfaces()`; the cargo builds cmake drives resolve against the
  repo-root `.cargo/config.toml`, which carries no `include`.

Stating that rule once, and routing to it, is the phase. Pasting `nros sync`
into 12 files is not — it would leave the C/C++ reader running a step they do
not need and teach nobody why.

## Work items

**W1 — the missing step, stated as the rule. LANDED.** Three starter pages
hand the reader a bare `cd <leaf> && cargo build`, and those three are the ones
that break: `freertos.md`, `threadx.md`, `bare-metal.md`. Each now runs
`nros sync` first and says what the failure looks like if you skip it. The
remaining nine reach their builds through `just` recipes, `west`, `make` or
`idf.py`, all of which sync (or drive codegen) themselves — they were not
broken, and pasting the step into them would teach a requirement that is not
theirs.

Verified rather than argued: moving the generated `nros-patch.toml` aside
reproduces the five-frame parse failure in
`examples/qemu-arm-baremetal/rust/talker`; `nros sync` there regenerates the
file byte-identically and the build proceeds past it. No C/C++ block gained a
step.

The finding that made the scope decision: **every `just` build path already
syncs** (`fixtures-build.sh`'s pre-pass, issue 0649) — so the one path in the
repo that skips it is the copy-pasteable leaf build the book teaches. CI
exercises the recipes; the book teaches the leaf.

**W2 — the platform × language workflow page. LANDED.**
`book/src/user-guide/workflow-by-platform.md`, first entry under User Guide.
The builder grid is generated from the 413 coordinate rows of
`examples/fixtures.toml`, so it states what CI executes rather than what the
prose remembers. All nine `nros setup <board>` names in its per-platform table
were validated against the CLI with `--dry-run`; the `just` module names come
from `just --list`.

Two claims were written, checked, and corrected before landing rather than
after: `[build] target` is set by the Cortex-M leaves only, not universally
(zephyr, threadx-linux and nuttx Rust leaves do not set it), and the Arm FVP
page provisions with `nros setup zephyr` plus a license-gated binary, not the
`fvp-aemv8r-smp` index row. Both would have read as authoritative.

**W3 — currency of `user-guide/workflow.md`. LANDED.** Its step 5 showed
`nros metadata` / `nros plan` / `nros check` as the multi-component build path.
Those commands are real, but they are the INSPECTION path — they produce and
validate an `nros-plan.json`. The build path, per the header of
`workspace-fixtures-build.sh`, is `nros sync` → `nros codegen-system --bringup
<b> --out <o>` → `cargo build -p <entry>` (or the cmake target). The page now
says that, notes which single-example builds need sync, and describes
SystemModels as build artifacts an entry names by INPUT rather than by path.

Recency of edit is not currency of content: this page was touched the day
before the audit and was wrong about the whole multi-node flow.

One correction found in review: the draft recommended `nros ws model-dims` for
inspecting a resolved model, which CLAUDE.md also suggests. That subcommand is
`hide = true` — its own doc-comment says it exists so `check-model-dims.sh` can
ask. Pointing users at a hidden gate seam is not documenting a workflow; the
page says `nros model-path` instead, which is public and prints the resolved
path.

**W4 — probe coverage.** `probe=` covers 3 blocks on 2 pages, and both are pages
that already had the step right; the pages that were wrong were exactly the
unprobed ones. Extend the tagged set so a starter's build block is executed in
the clean container rather than asserted. Acceptance: at least the native and
one cross-compiled Rust starter path run under `just probe bootstrap`.
Explicitly out of scope: starters needing hardware, a vendor SDK, or a QEMU
image the probe container does not provision — for those, W2's table is the
guarantee and the phase says so rather than pretending coverage.

**W5 — in-checkout vs copied-out, said once.** Found while verifying W1: the
starter pages' `cd <leaf> && cargo build` writes `examples/**/target/`, which
`check-example-leaf-target-dirs` rejects (269 MB from one Cortex-M leaf, and a
red `check-fast`). The command is not wrong — it is right for a copied-out
example and wrong inside the checkout, and no page says which the reader is in.
W2's page now draws the distinction. Acceptance for the rest: no starter page
tells an in-checkout reader to `cargo build` in a leaf without saying the recipe
is the in-tree path.

**W6 — dead relative links, and a gate for them. LANDED.** The audit found nine
relative links in the book resolving to nothing. Two were archival (`phase-115`,
`phase-212` moved under `archived/`); seven were depth errors — `../../docs/…`
written from `book/src/<dir>/page.md`, which is `book/docs/…`, one level short
of the repo root. All nine fixed; 468 relative links across 108 pages now
resolve.

`check-doc-refs` was green throughout, correctly: it resolves the numbered
series by ID and deliberately accepts an archived file, because the id is what
the reference means. A rendered link is a PATH, and no gate asked that question.
`scripts/check-book-links.py` now does, wired into `check-fast` (buildless —
reads tracked files, resolves paths, never invokes mdbook). Self-tested: a dead
link makes it exit 1, and the page restores byte-identical.

Note for W4: `mdbook` is not installed on this host and `just book` additionally
fails on a pre-existing rustdoc intra-doc link error in `nros` (`no item named
'std' in scope`), unrelated to this phase. The link gate is what verified the
book here; a real render has not been done.
