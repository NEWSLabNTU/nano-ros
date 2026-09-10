"""What "core" means — one definition, derived, shared by every gate that needs it.

Issue 1212. Before this module, four places each kept their own idea of the core
crate set and no two agreed: `check-core-crates-are-no-std` listed 6,
`check-std-census` scoped `packages/core` + `packages/api` minus a hand-written
2, `just check no-std` built 12, and ARCHITECTURE section 2's agnosticism
contract named 6 — one of which, `nros-orchestration`, has never existed under
that spelling. RFC-0001's diagrams draw a fifth set. `nros-executor-layout` was
in NONE of them. That divergence IS the defect; a corrected literal list in each
place would only reset the clock on it (the issue-0196 shape).

## The definition

**A core crate is one that must compile for a target with no operating system
and no standard library.** Its home is `packages/core/`.

So the set is DERIVED, not enumerated: every directory under `packages/core/`
that holds a `Cargo.toml` is core, unless it classifies itself out. There are
exactly two ways out, both properties of the crate and both readable from the
crate's own manifest:

* **`proc-macro = true`** — structural, and unfakeable. A proc-macro runs on the
  HOST at compile time; its `std::` occurrences are tokens it EMITS, not a
  dependency of anything embedded. (`nros-macros`.)
* **`[package.metadata.nros] host-only = true`**, with its `host-only-reason` —
  the marker issue 0287 already introduced for exactly this claim, read by
  `scripts/build/host-only-members.sh` to exclude a member from
  `check workspace-embedded`. Reusing it rather than inventing a second spelling
  is the point: a `packages/core` crate that does NOT declare it is already
  being compiled for `thumbv7em-none-eabihf` by that lane, so the `#![no_std]`
  obligation this module hands out is one the crate is already living under.
  A crate cannot be inside the embedded workspace build and exempt from the
  property that build depends on.

A directory with no `Cargo.toml` at all is not a Rust crate and carries no Rust
obligation; `packages/core/nros-rmw-abi` is the one such directory today (see
the ruling below). It is reported separately rather than silently skipped, so a
crate whose manifest goes missing cannot quietly vanish from the gate.

The consequence that matters: a NEW crate landing in `packages/core/` is core by
default and must declare `#![no_std]`. The previous design had the polarity
backwards — a deliberate list, which a new crate joins only if someone
remembers — and its own docstring argued for it ("a new crate landing there
should have to be added here on purpose"). That argument is about the DECISION,
not about the list, and the `host-only` opt-out keeps the decision while
removing the list: landing a host crate in `packages/core` requires writing down
why, and landing a target crate requires nothing, because the obligation is the
default. Three target-side crates (`nros-serdes`, `nros-serdes-packed`,
`nros-diagnostics`) and a fourth nobody had noticed at all
(`nros-executor-layout`) sat outside the old list reading as "considered and
excluded" when they had simply never been enumerated.

## The three placement rulings (issue 1212, and the study behind 1211)

**`packages/core/nros-rmw-abi` — correctly placed.** It holds no Rust: a
`CMakeLists.txt`, a `Doxyfile` and five headers under `include/nros/`, and it is
the SSoT for the RMW C ABI (RFC-0054) that both the Rust seam (`nros-rmw-cffi`)
and every C backend implement. `packages/core` is the FOUNDATIONAL layer, not
"the Rust crates directory" — the contract two languages meet at is as core as
anything in the tree, and moving it would put the ABI definition further from
the layer it defines than the code that consumes it. What it is not is a Rust
crate, so it carries no `#![no_std]` obligation, and the derivation skips it
STRUCTURALLY (absence of `Cargo.toml`), never by name.

**`packages/core/nros-macros` — correctly placed, and host.** ARCHITECTURE
section 2 already rules on this directly: "entry macros that emit per-target
boot code are framework API and legitimately live in `nros`/`nros-macros`, NOT
platform-impl". `nros::main!` is core API by every measure except where its own
code runs. `proc-macro = true` classifies it, so no annotation is needed and
none can drift.

**`packages/core/nros-orchestration-ir` — correctly placed, and host.** It is
the `system.toml` schema, and its two consumers are the `nros` CLI's codegen and
the `nros::main!` proc-macro — it exists to be shared by the host halves of the
core, and it is a dependency of `nros-macros`. Moving it to `packages/tooling`
would misfile it: tooling crates are build-support for this repo, while this is
the schema the framework's own entry macro resolves a user's `system.toml`
against. It already carried `host-only = true` for issue 0287's reason, and that
marker now carries this meaning too.

## The seams, which are explicit on purpose

Three crates carry the same no-OS obligation while living beside their layer's
implementations rather than in `packages/core`: the vtable seams named in
ARCHITECTURE section 2's agnosticism contract. They are listed literally in
`SEAM_CRATES` below, and that is not the drift risk the derived half is — the
contract that defines them is closed (there are two axes, RMW and platform), and
a fourth seam appearing would be an architecture change with an RFC behind it,
not a crate quietly landing in a directory.

Run `python3 scripts/lib/core_crates.py` to print what this currently resolves to.
"""

from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11 — the repo's interpreter is 3.10
    import tomli as tomllib

REPO = Path(__file__).resolve().parents[2]

CORE_DIR = "packages/core"

# The vtable seams: core by the no-OS property, resident beside their layer.
# See the module docstring for why this half is a list and the other is derived.
SEAM_CRATES = [
    "packages/platform/nros-platform-api",
    "packages/platform/nros-platform-cffi",
    "packages/rmw/cffi",
]


class CoreLayout:
    """The resolved answer to "what is core", with the reasons attached.

    `target`   — repo-relative paths that must be `#![no_std]` and must compile
                 for a bare target: `packages/core` residents plus the seams.
    `host`     — [(rel, why)] classified out, each with the reason it gave.
    `non_rust` — [rel] directories under `packages/core` with no `Cargo.toml`.
    `bad`      — [(rel, complaint)] manifests that claim `host-only` without
                 saying why. A silent opt-out is the state this module exists to
                 make impossible.
    """

    def __init__(self, target, host, non_rust, bad):
        self.target = target
        self.host = host
        self.non_rust = non_rust
        self.bad = bad


def _classify(manifest_path):
    """(role, why) for one manifest; role is "target", "host", or None for bad."""
    with open(manifest_path, "rb") as fh:
        doc = tomllib.load(fh)

    if doc.get("lib", {}).get("proc-macro") is True:
        return "host", "`proc-macro = true` -- runs on the build host"

    meta = doc.get("package", {}).get("metadata", {}).get("nros", {})
    if meta.get("host-only") is not True:
        return "target", None
    reason = str(meta.get("host-only-reason") or "").strip()
    if not reason:
        return None, "`host-only = true` with no `host-only-reason` beside it"
    return "host", reason


def layout(repo=None):
    base = Path(repo) if repo else REPO
    target, host, non_rust, bad = [], [], [], []

    for entry in sorted(p for p in (base / CORE_DIR).iterdir() if p.is_dir()):
        rel = f"{CORE_DIR}/{entry.name}"
        manifest = entry / "Cargo.toml"
        if not manifest.is_file():
            non_rust.append(rel)
            continue
        role, why = _classify(manifest)
        if role is None:
            bad.append((rel, why))
        elif role == "host":
            host.append((rel, why))
        else:
            target.append(rel)

    target.extend(SEAM_CRATES)
    return CoreLayout(target, host, non_rust, bad)


def core_crate_paths(repo=None):
    """Repo-relative paths of every crate that must be `#![no_std]`."""
    return layout(repo).target


def host_crate_names(repo=None):
    """Crate directory names under `packages/core` that are host-only."""
    return {rel.rsplit("/", 1)[1] for rel, _ in layout(repo).host}


def self_test():
    """The DERIVATION is what replaced a list, so it verifies itself before use.

    A synthetic `packages/core` with one of each kind: an ordinary crate, a
    proc-macro, a declared host crate, a host crate that gives no reason, and a
    directory with no manifest. This runs on the NORMAL path of every entry
    point -- the module's own CLI and `check-core-crates-are-no-std` -- so the
    `just check no-std` lane exercises it too before spending five minutes on
    the flags it emits. A negative control nobody runs decays into a comment.

    Returns the number of problems; prints each to stderr.
    """
    import tempfile
    import textwrap

    want_target = {"packages/core/ordinary"}
    want_host = {"packages/core/macros", "packages/core/schema"}
    want_non_rust = {"packages/core/headers"}
    want_bad = {"packages/core/silent"}

    manifests = {
        "ordinary": '[package]\nname = "ordinary"\n',
        "macros": '[package]\nname = "macros"\n\n[lib]\nproc-macro = true\n',
        "schema": textwrap.dedent(
            """\
            [package]
            name = "schema"

            [package.metadata.nros]
            host-only = true
            host-only-reason = "a schema the CLI reads"
            """
        ),
        "silent": textwrap.dedent(
            """\
            [package]
            name = "silent"

            [package.metadata.nros]
            host-only = true
            """
        ),
    }

    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        for name, body in manifests.items():
            (root / "packages" / "core" / name).mkdir(parents=True)
            (root / "packages" / "core" / name / "Cargo.toml").write_text(body)
        (root / "packages" / "core" / "headers").mkdir(parents=True)
        lay = layout(repo=root)

    # The seams are appended unconditionally; the derived half is under test.
    got_target = {r for r in lay.target if r.startswith(CORE_DIR + "/")}
    got_host = {r for r, _ in lay.host}
    got_bad = {r for r, _ in lay.bad}

    problems = []
    if got_target != want_target:
        problems.append(f"target: got {sorted(got_target)}, want {sorted(want_target)}")
    if got_host != want_host:
        problems.append(f"host: got {sorted(got_host)}, want {sorted(want_host)}")
    if set(lay.non_rust) != want_non_rust:
        problems.append(f"non_rust: got {sorted(lay.non_rust)}, want {sorted(want_non_rust)}")
    if got_bad != want_bad:
        problems.append(f"bad: got {sorted(got_bad)}, want {sorted(want_bad)}")
    if set(SEAM_CRATES) - set(lay.target):
        problems.append("the seam crates were dropped from the derived set")

    import sys as _s

    for problem in problems:
        print(f"  core_crates self-test FAIL: {problem}", file=_s.stderr)
    return len(problems)


def package_name(rel, repo=None):
    """The cargo package name of the crate at repo-relative `rel`.

    Read from the manifest, not inferred from the directory. They coincide for
    every `packages/core` resident today and do NOT in general -- `nros-rmw-cffi`
    lives at `packages/rmw/cffi` -- so inferring would be a rule that happens to
    hold where it is used and breaks the first time a seam is added.
    """
    base = Path(repo) if repo else REPO
    with open(base / rel / "Cargo.toml", "rb") as fh:
        return tomllib.load(fh)["package"]["name"]


def _cargo_flags():
    """`-p <name>` for every core crate under `packages/core`.

    The SEAMS are deliberately absent: `just check no-std` builds them on a
    per-target basis (`nros-rmw-cffi` needs pointer atomics, so it is Cortex-M
    only), which is a target-capability fact and not part of what core means.
    The lane adds them itself; what it must not do is keep its own copy of the
    derived half.
    """
    lay = layout()
    return " ".join(
        f"-p {package_name(rel)}"
        for rel in lay.target
        if rel.startswith(CORE_DIR + "/")
    )


def _main():
    import sys

    # On the NORMAL path, both modes: the flags this emits decide what a
    # five-minute cross-compile lane covers, so a silently-wrong derivation
    # would show up as a lane that passes over less than it claims.
    if self_test():
        print("core_crates: self-test FAILED -- refusing to report a derived set", file=sys.stderr)
        return 1

    if len(sys.argv) == 2 and sys.argv[1] == "--cargo-flags":
        lay = layout()
        if lay.bad:
            for rel, why in lay.bad:
                print(f"core_crates: {rel}: {why}", file=sys.stderr)
            return 1
        print(_cargo_flags())
        return 0

    lay = layout()
    print(f"core -- must be unconditionally no_std ({len(lay.target)}):")
    for rel in lay.target:
        seam = "  (seam)" if rel in SEAM_CRATES else ""
        print(f"  {rel}{seam}")
    print(f"host -- classified out of {CORE_DIR} ({len(lay.host)}):")
    for rel, why in lay.host:
        print(f"  {rel}: {why}")
    print(f"non-Rust under {CORE_DIR} ({len(lay.non_rust)}):")
    for rel in lay.non_rust:
        print(f"  {rel}")
    if lay.bad:
        print(f"UNCLASSIFIABLE ({len(lay.bad)}):")
        for rel, why in lay.bad:
            print(f"  {rel}: {why}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(_main())
