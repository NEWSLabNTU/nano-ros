#!/usr/bin/env python3
"""A board with a Rust triple states it: `[build] target` in its `cargo_config`.

WHY THIS EXISTS (phase-445 W2, RFC-0098 D4). The board descriptor owns every
board fact, and later waves GENERATE each image's cargo settings from it and
delete the leaf `.cargo/config.toml` files. A fact the descriptor omits is then
a fact no image gets. Measured on 2026-09-10: esp32, both NuttX boards and
ThreadX-RV64 stated `[build] target`; the two mps2 boards and the two armv8r
FreeRTOS boards did not — so 19 mps2 leaves hand-wrote
`target = "thumbv7m-none-eabi"`, the one board family that had to, and the
file RFC-0098 deletes was the only place their triple lived.

THE RULE, per `[[board]]` entry of every tracked `packages/boards/**/nros-board.toml`:

  * the board HAS a Rust triple when its `cargo_config` configures a
    `[target.<triple>]`, or when it is a `board-run` board (a cross image whose
    cargo settings come from this descriptor — it has a triple whether or not
    the blob says so, and a blob that says nothing is the gap itself);
  * such a board must set `[build] target`.

Exempt, and REPORTED as exempt rather than silently skipped:

  * `hosted-main` with no `[target.*]` — the host triple; there is no cross
    target to state;
  * `zephyr-staticlib` with no `[target.*]` — zephyr-lang-rust's
    `_rust_map_target` chooses the triple inside the Zephyr build, and RFC-0098
    leaves the Zephyr lane's configuration where it is.
  * `board-run` on a HOST-SIMULATOR platform with no `[target.*]` — issue
    1759. `entry_kind` states the entry's SHAPE, not whether the build crosses:
    `threadx-linux` is a `#![no_std]` + `#![no_main]` entry that glibc's crt0
    calls, built for the host triple. "`board-run` implies a cross triple" was
    true of every board until then and is a correlation, not the rule; the
    platform is what says the triple is the host's. A host-simulator board that
    DOES configure a `[target.*]` is still judged.

Whether `[build] target` NAMES one of the configured triples is
`check-board-cargo-config-shape`'s rule, not repeated here.

Discovery is the git index (`scripts/lib/tracked.py`, issue 0721), with the
inherited repository environment cleared first (issue 0986): this runs from the
`pre-push` hook's lane, where an inherited `GIT_DIR` names another index.

Exit 0 when every board with a triple states it, 1 otherwise.
"""

import os
import sys
import tempfile
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402
from tracked import tracked  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent

# Board kinds whose triple is NOT the descriptor's to state, and why.
EXEMPT_KINDS = {
    "hosted-main": "host triple",
    "zephyr-staticlib": "zephyr-lang-rust maps the triple",
}


# Platforms whose port runs inside a host process, so a `board-run` entry on
# them builds for the HOST triple and has none of its own to state (issue 1759).
HOST_TRIPLE_PLATFORMS = {
    "threadx-linux": "ThreadX's Linux simulator: board-run entry, host triple",
}


def descriptors(root):
    """Tracked board descriptors — never a leaf's `.cargo/nros-board.toml`
    projection, which shares the file name."""
    # No `repo=`: the real root is inside the repo and resolves through the
    # index; a `--self-test` temp root is outside it and is walked (tiny).
    return [
        p
        for p in tracked(Path(root) / "packages" / "boards", name="nros-board.toml")
        if ".cargo" not in p.parts
    ]


def check_entry(rel, entry):
    """Returns (verdict, message): verdict is 'ok', 'exempt' or 'fail'."""
    names = "/".join(entry.get("names", [])) or "<unnamed>"
    kind = entry.get("entry_kind", "")
    blob = entry.get("cargo_config")
    cfg = {}
    if blob is not None:
        try:
            cfg = tomllib.loads(blob)
        except tomllib.TOMLDecodeError as e:
            return "fail", f"{rel} [{names}]: `cargo_config` is not valid TOML: {e}"
    triples = sorted(cfg.get("target", {}))
    build_target = cfg.get("build", {}).get("target")

    if not triples and kind in EXEMPT_KINDS:
        return "exempt", f"{names} ({kind}: {EXEMPT_KINDS[kind]})"
    platform = entry.get("platform", "")
    if not triples and kind == "board-run" and platform in HOST_TRIPLE_PLATFORMS:
        return "exempt", f"{names} ({kind} on {platform}: {HOST_TRIPLE_PLATFORMS[platform]})"
    has_triple = bool(triples) or kind == "board-run"
    if not has_triple:
        # A kind this gate does not know and no triple configured: say so
        # rather than pass it as though it had been judged.
        return "fail", (
            f"{rel} [{names}]: entry_kind `{kind}` is not one this gate knows "
            f"and the board configures no `[target.*]` — add the kind to "
            f"EXEMPT_KINDS with a reason, or state the board's triple."
        )
    if isinstance(build_target, str) and build_target:
        return "ok", names
    where = f"configures {', '.join(triples)}" if triples else "is a `board-run` board"
    return "fail", (
        f"{rel} [{names}]: {where} but its `cargo_config` has no `[build] target`. "
        f"Every image of this board would have to hand-write the triple in its own "
        f"`.cargo/config.toml` — the file RFC-0098 deletes. Add "
        f"`[build]\\ntarget = \"<triple>\"` to the blob."
    )


def scan(root):
    oks, exempt, problems = [], [], []
    for path in descriptors(root):
        rel = os.path.relpath(path, root)
        try:
            data = tomllib.loads(path.read_text(encoding="utf-8"))
        except tomllib.TOMLDecodeError as e:
            problems.append(f"{rel}: not valid TOML: {e}")
            continue
        for entry in data.get("board", []):
            verdict, msg = check_entry(rel, entry)
            {"ok": oks, "exempt": exempt, "fail": problems}[verdict].append(msg)
    return oks, exempt, problems


def self_test():
    """Negative controls — each rule must FIRE on the shape it names, every run."""
    head = '[[board]]\nnames = ["x"]\nentry_kind = "{kind}"\n'
    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp, "packages/boards/nros-board-x")
        d.mkdir(parents=True)
        path = d / "nros-board.toml"
        # A decoy projection INSIDE the scanned tree, so the `.cargo` filter is
        # actually exercised (the test bins and fixture entries carry these).
        proj = d / "leaf" / ".cargo"
        proj.mkdir(parents=True)

        def run(kind, blob=None, platform=None):
            text = head.format(kind=kind)
            if platform is not None:
                text += 'platform = "%s"\n' % platform
            if blob is not None:
                text += "cargo_config = '''\n%s'''\n" % blob
            path.write_text(text, encoding="utf-8")
            return scan(tmp)

        target = '[target.thumbv7m-none-eabi]\nrunner = "q"\n'
        build = '[build]\ntarget = "thumbv7m-none-eabi"\n'

        oks, _, problems = run("board-run", build + target)
        assert oks == ["x"] and not problems, (oks, problems)

        # THE case: mps2's shape before phase-445 W2.
        _, _, problems = run("board-run", target)
        assert any("no `[build] target`" in p for p in problems), problems

        # A cross board with no blob at all is the gap, not an exemption.
        _, _, problems = run("board-run")
        assert any("`board-run` board" in p for p in problems), problems

        # A hosted board that DOES configure a cross triple is judged, not exempt.
        _, _, problems = run("hosted-main", target)
        assert problems, "a configured triple outranks the kind exemption"

        _, exempt, problems = run("hosted-main")
        assert not problems and exempt, (exempt, problems)

        # issue 1759 — a board-run entry on a host simulator states no triple.
        _, exempt, problems = run("board-run", platform="threadx-linux")
        assert not problems and exempt, (exempt, problems)
        # …but only there: the same row on a cross platform is still the gap.
        _, _, problems = run("board-run", platform="threadx-riscv64")
        assert any("`board-run` board" in p for p in problems), problems
        # …and a host-simulator board that configures a triple is judged.
        _, _, problems = run("board-run", target, platform="threadx-linux")
        assert any("no `[build] target`" in p for p in problems), problems
        _, exempt, problems = run("zephyr-staticlib")
        assert not problems and exempt, (exempt, problems)

        # An unknown kind is refused, never waved through.
        _, _, problems = run("some-new-kind")
        assert any("not one this gate knows" in p for p in problems), problems

        # A leaf projection shares the file name and must not be read as a board.
        (proj / "nros-board.toml").write_text(target, encoding="utf-8")
        path.write_text(head.format(kind="hosted-main"), encoding="utf-8")
        assert not scan(tmp)[2], "a `.cargo/nros-board.toml` projection is not a descriptor"
    return 0


def main(argv):
    nros_clear_inherited_git_env()
    self_test()
    if "--self-test" in argv:
        print("check-board-build-target self-test: OK")
        return 0
    # `--root <dir>` scans another tree (a negative control over real
    # descriptors). It must sit OUTSIDE the repo: an in-repo untracked dir
    # resolves through the index and yields nothing, which the empty-set
    # refusal below turns into a failure rather than a vacuous pass.
    root = ROOT
    if "--root" in argv:
        root = Path(argv[argv.index("--root") + 1]).resolve()
    oks, exempt, problems = scan(root)
    if problems:
        print("check-board-build-target: FAILED")
        for p in problems:
            print(f"  - {p}")
        return 1
    if not oks:
        print("check-board-build-target: no board with a Rust triple found — refusing "
              "to pass on an empty set")
        return 1
    print(f"check-board-build-target: OK ({len(oks)} board(s) with a triple state "
          f"`[build] target`; {len(exempt)} exempt: {'; '.join(exempt)})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
