# RFC-0059 — Split the launch toolchain: Python-linked front-end, in-tree compiler

**Status:** Draft (2026-07-26)
**Supersedes nothing. Amends:** RFC-0050 (SystemModel) §ownership table.
**Motivated by:** issue 0285 (version skew broke every platform's fixture build),
and the standing phase-195.A constraint recorded in `nros-cli-core/Cargo.toml`.

## Summary

Move the Python-free stages of launch resolution *into* our toolchain as a
linked library, and keep only the stages that genuinely require CPython in a
separately-built tool. The interface between them becomes a **data artifact**,
not a CLI verb.

The load-bearing measurement: **all 101 tracked launch files under `examples/`
are `.launch.xml`, none uses `$(eval …)`, and the only substitutions in the
tree are `$(var …)` (7) and `$(env …)` (2).** So the entire in-tree corpus —
and, we expect, the large majority of embedded users — needs no CPython at all.
Today every one of them is blocked behind a tool that embeds a Python
interpreter.

## Problem

Three separate tools own pieces of one pipeline, and only one of them is in
our tree:

| Tool | Where | Python | How we use it |
| --- | --- | --- | --- |
| `nros-launch-parser` | in-tree (`packages/cli/`) | none | **linked** |
| `play_launch_parser` | submodule | pyo3 **mandatory**, `auto-initialize` | shelled |
| `play_launch` (`resolve`) | **not vendored** (tier4) | inherits the above | shelled |

Three consequences, all observed rather than hypothesised:

1. **Version skew is unbounded.** `nros sync` calls `play_launch resolve`, a
   verb of a tool we neither vendor nor pin. Issue 0285: an unrelated ROS 2
   record/replay tool of the *same binary name* sat on PATH, and every
   platform's `build-examples` failed inside a cmake configure.
2. **The contract is a CLI surface.** Subcommand sets are wide and unversioned.
   The 0285 failure was precisely a subcommand-set mismatch — the narrowest
   possible symptom of the widest possible contract.
3. **CPython is a hard build prerequisite for users who need none of it.**
   `pyo3` with `auto-initialize` is not optional in `play_launch_parser`
   (`crates/play_launch_parser/Cargo.toml`), so the interpreter is embedded
   whether or not a `.launch.py` is ever parsed.

And the reason it was built this way is sound, so any fix must respect it —
`nros-cli-core/Cargo.toml` (phase-195.A): pyo3 `auto-initialize` embeds
CPython, abi3 cannot unpin an embedding binary's libpython, and keeping it out
of the link is what makes the shipped `nros` a portable libc-only binary.

## Where the Python boundary actually is

Not where one would guess. It is **not** "XML is Python-free, `.py` is not":

- **`.launch.py` execution** — genuinely needs CPython. Cleanly
  extension-dispatched (`play_launch_parser` `src/lib.rs`, `traverser/ir_builder.rs`),
  and the `ir` feature already degrades it to an `OpaqueFunction` placeholder
  rather than executing — a ready-made Python-free path.
- **`$(eval …)` substitution** — needs CPython today and is **not**
  extension-gated: XML and YAML reach it equally
  (`substitution/eval.rs`, deliberately delegating to Python's `eval()` to
  match ROS 2 exactly, with no Rust fallback). This is the real boundary.
- **`From<pyo3::PyErr> for ParseError`** (`src/error.rs`) leaks pyo3 into the
  core error type, so every module transitively depends on it. Mechanical to
  gate, but it is why the crate has no Python-free build today.

Everything else — XML/YAML structure, includes, conditions, remaps,
namespaces, record generation, the whole `ros-launch-manifest`
types/model/sched trio — is already pure Rust.

## Design

**The seam is the expanded-launch IR, not the launch file.**

```
  .launch.xml / .yaml ─┐
  .launch.py ──────────┤→  [A] launch expansion  →  expanded-launch IR
  $(eval …) ───────────┘         (CPython only when the tree needs it)
                                                          │
  contract manifests ─┐                                   ▼
  system.toml ────────┴→  [B] resolve: bind args → filter conditions →
                             merge scopes → check → emit   (pure Rust, LINKED)
                                                          │
                                                          ▼
                                                    SystemModel
```

**[A] launch expansion** — a tool, built on the user's machine against the
user's Python, exactly the way `just setup-cli` builds `nros`. That is what
dissolves the abi3 constraint: we never ship a binary linked against a
libpython we guessed.

It runs Python-free whenever the launch tree contains no `.launch.py` and no
`$(eval …)`. On the measured corpus that is 101 files out of 101.

**[B] resolve** — a library in our tree, over the already-linked
`ros-launch-manifest-{types,model,sched}`. Versioned with the CLI, so it cannot
skew against it.

**The interface is the IR artifact**, and it is *committable* the way
`system_model.yaml` already is. A CI machine or a Python-less user builds from
the committed artifact; only someone **editing** a `.launch.py` or an
`$(eval)` expression needs the Python tool.

## The user-side build mechanism already exists

This is the part that makes the split cheap rather than speculative: the
"built on the user's machine against the user's Python" model is **not new
work**. It is already how the CPython-linked parser ships:

- `just/workspace.just` pins `PLAY_LAUNCH_PARSER_VERSION` to an exact rev;
- installs it with `cargo install --path …/crates/play_launch_parser` from a
  checkout at that rev, into `~/.nros/sdk/play_launch_parser`;
- `.envrc` / `activate.sh` prepend that `bin` so PATH is ours, not
  `~/.local/bin`'s;
- a doctor lane stamps `.installed-version` and reports `[OK]` / `[PATH]` /
  `[MISSING]`.

So [A] needs no new distribution mechanism — it needs the existing one pointed
at the right thing. And issue 0285's item 1 ("pin and install a resolve-capable
build under `~/.nros/…`") is asking for exactly this machinery to be extended
to the second tool; this RFC's position is that the better move is to need
less of the second tool rather than to pin more of it.

Two further facts worth recording, because they narrow the work:

- **`play_launch_parser` is barely shelled from `nros` today.** The only live
  spawn in `nros-cli-core` is `play_launch resolve` (`cmd/ws.rs`); the parser
  binary is consumed by tests and `scripts/build/compile-check-fixtures.sh`,
  which already degrades gracefully when it is absent. So the Python-free
  library path has few call sites to convert.
- **A `--no-default-features` build is the shape of [A]'s Python-free half.**
  Once `pyo3` is optional (three surgical changes: optional dep, `#[cfg]` the
  `From<PyErr>` impl and the `python` module, decide the `$(eval)` policy),
  `nros` can link the XML/YAML path directly and keep the pinned out-of-tree
  binary for `.launch.py` and `$(eval)` alone.

## Decisions this RFC must make

1. **`$(eval …)` policy** — the only non-extension-gated Python dependency.
   - (a) **Reject with a clear diagnostic** in the Python-free path. Honest,
     trivial, and correct for the whole current corpus.
   - (b) **Pure-Rust evaluator for the restricted subset.** The existing
     sandbox already restricts to pure builtins (arithmetic, comparisons,
     str/list ops), so the target is bounded and testable — but "matching ROS 2
     exactly" was the stated reason for delegating, and a divergence here is a
     silent wrong answer, not an error.
   - (c) **Defer it into the IR** unevaluated, and let the Python tool fill it
     in later.

   Recommendation: **(a) now, (c) as the extension point, (b) only against a
   differential test corpus** — never by eyeballing semantics.

2. **What happens to `nros-launch-parser`.** It is a prior, minimal attempt at
   the same job: pure Rust, linked, restricted tag set, no `$(eval)`, no `.py`,
   no nested substitutions. Either grow it into [A]'s Python-free path, or
   retire it in favour of a feature-gated `play_launch_parser`. Keeping both is
   the outcome to avoid — two parsers disagreeing about one launch file is the
   drift this repo pays for elsewhere.

3. **Whether `check`'s 14 rules come along.** `ros-launch-manifest-check` is
   Python-free but deps **z3**, a heavy native C++ dependency, which is why it
   is not linked today. Python-free and *cheap to link* are different axes, and
   this RFC only claims the first.

4. **Where [B] lives** — grown in-tree, or upstreamed into
   `ros-launch-manifest` and vendored. Upstreaming is better if `play_launch`
   would also consume it, since then there is one resolver rather than two.

## What this does not change

`play_launch` keeps `resolve` as a verb for its own users; this RFC is about
what *nano-ros* depends on. RFC-0050's SystemModel schema is untouched — only
who computes it changes.

## Acceptance

- [ ] A workspace of `.launch.xml` files with no `$(eval)` resolves to a
      `SystemModel` with **no Python on the machine at all**, proven by a
      container lane that has no interpreter installed.
- [ ] `nros sync` no longer shells a tool we do not pin for the Python-free
      path.
- [ ] A launch tree that DOES need Python fails with a diagnostic naming the
      file and the construct, not a clap error from inside a cmake configure.
- [ ] Exactly one launch parser in the tree.
