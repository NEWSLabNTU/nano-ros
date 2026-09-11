#!/usr/bin/env python3
"""Every root-workspace `exclude` entry names a real directory, for a real reason.

Issue 1217, phase-451 W3. The root `Cargo.toml` carried 58 `members` and 165
`exclude` entries, no globs in either. Measured 2026-09-11: **36 of the 165
named directories that do not exist**, and one — `packages/rmw/transport-callbacks`
— was excluded for no discoverable reason at all, wedged between the comments
belonging to its two neighbours. It builds clean on the host.

WHY THIS IS A GATE AND NOT A CLEANUP

An `exclude` entry is a declaration with no failure mode. Naming a deleted
directory costs nothing at build time, so nothing ever says so, and the list
only grows. That is the same shape as the dead cmake module in W1 of this
phase: a declaration whose only remaining effect is on BELIEF. A reader
answering "is this crate part of the workspace, and why not?" gets an answer
from a list that has been wrong for months.

Cleaning it once buys a month. The list was audited before (`docs/development/
audit-findings-2026-07-28.md`), and grew back.

WHAT COUNTS AS A REASON

Derived, never read off a comment — a comment is what drifted. An entry is
justified when the tree itself says so, by one of:

  R1  the directory has its own `[workspace]` table
  R2  an ANCESTOR directory has one (a member of a nested workspace; this is
      every fixture workspace's leaf packages)
  R3  the directory has its own TRACKED `Cargo.lock`
  R4  its `.cargo/config.toml` pins a non-host `[build] target`
  R5  its manifest depends on a cross-only crate (cortex-m, esp-hal, rtic,
      stm32f4xx-hal, …) — it cannot build for the host by construction
  R6  it declares no Rust target at all: no `src/`, no `[lib]`, no `[[bin]]`.
      Metadata-only, e.g. `packages/interfaces/rcl-interfaces`, whose real
      crates are the generated ones underneath it

Anything else must be listed in `.config/workspace-exclude-reasons.txt` with a
one-line reason, and that file is a RATCHET: it may only SHRINK. The allowlist
exists because four board and platform crates are genuinely cross-only in a way
no manifest fact states — `nros-board-freertos` and its siblings carry no
cross-only dependency and no pinned target; they are excluded because the kernel
build glue is not a cargo fact. Inventing a derivation for that would be a gate
asserting something it did not measure, which is exactly what phase-450 is
about. Declaring it, with the list allowed only to shrink, is honest.

Run:  python3 scripts/check-workspace-exclude-list.py [--self-test]
"""

import os
import subprocess
import sys

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib

ALLOWLIST = ".config/workspace-exclude-reasons.txt"

CROSS_ONLY = {
    "cortex-m",
    "cortex-m-rt",
    "cortex-m-rtic",
    "esp-alloc",
    "esp-backtrace",
    "esp-hal",
    "esp-println",
    "panic-halt",
    "riscv",
    "riscv-rt",
    "rtic",
    "stm32f4xx-hal",
}


def load_toml(path):
    try:
        with open(path, "rb") as fh:
            return tomllib.load(fh)
    except Exception:
        return None


def tracked_files():
    out = subprocess.run(
        ["git", "ls-files"], capture_output=True, text=True, check=False
    ).stdout
    return set(out.split("\n"))


def ancestor_workspace(path):
    cur = os.path.dirname(path)
    while cur and cur not in (".", "/"):
        manifest = os.path.join(cur, "Cargo.toml")
        if os.path.isfile(manifest):
            doc = load_toml(manifest)
            if doc is not None and "workspace" in doc:
                return cur
        cur = os.path.dirname(cur)
    return None


def declares_no_target(path, doc):
    if os.path.isdir(os.path.join(path, "src")):
        return False
    return "lib" not in doc and "bin" not in doc


def reason_for(path, tracked):
    """Return a derived reason string, or None if the tree does not justify it."""
    manifest = os.path.join(path, "Cargo.toml")
    if not os.path.isfile(manifest):
        return "R6: no Cargo.toml (metadata only)"
    doc = load_toml(manifest)
    if doc is None:
        return "R6: Cargo.toml does not parse as a package manifest"
    if "workspace" in doc:
        return "R1: own [workspace] table"
    ancestor = ancestor_workspace(path)
    if ancestor:
        return "R2: member of nested workspace %s" % ancestor
    if os.path.join(path, "Cargo.lock") in tracked:
        return "R3: own tracked Cargo.lock"
    config = os.path.join(path, ".cargo", "config.toml")
    if os.path.isfile(config):
        cfg = load_toml(config)
        if cfg is not None and (cfg.get("build") or {}).get("target"):
            return "R4: .cargo/config.toml pins [build] target"
    deps = set(doc.get("dependencies") or {})
    for _, table in (doc.get("target") or {}).items():
        deps |= set(table.get("dependencies") or {})
    hit = deps & CROSS_ONLY
    if hit:
        return "R5: cross-only dependency (%s)" % ", ".join(sorted(hit))
    if declares_no_target(path, doc):
        return "R6: declares no Rust target"
    return None


def read_allowlist():
    """path -> reason. Blank lines and `#` comments ignored."""
    entries = {}
    if not os.path.isfile(ALLOWLIST):
        return entries
    with open(ALLOWLIST, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            path, _, why = line.partition("#")
            entries[path.strip()] = why.strip()
    return entries


def self_test():
    """The rules must be able to say NO. A gate that cannot fail is not a gate."""
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        plain = os.path.join(tmp, "plain")
        os.makedirs(os.path.join(plain, "src"))
        with open(os.path.join(plain, "Cargo.toml"), "w", encoding="utf-8") as fh:
            fh.write('[package]\nname = "p"\nversion = "0.0.0"\n')
        assert reason_for(plain, set()) is None, "a plain host crate must be unjustified"

        crossed = os.path.join(tmp, "crossed")
        os.makedirs(os.path.join(crossed, "src"))
        with open(os.path.join(crossed, "Cargo.toml"), "w", encoding="utf-8") as fh:
            fh.write('[package]\nname = "c"\nversion = "0.0.0"\n\n[dependencies]\ncortex-m = "0.7"\n')
        assert (reason_for(crossed, set()) or "").startswith("R5"), "cross-only dep must justify"

        meta = os.path.join(tmp, "meta")
        os.makedirs(meta)
        with open(os.path.join(meta, "Cargo.toml"), "w", encoding="utf-8") as fh:
            fh.write('[package]\nname = "m"\nversion = "0.0.0"\n')
        assert (reason_for(meta, set()) or "").startswith("R6"), "no-target pkg must justify"

    sys.stdout.write("check-workspace-exclude-list self-test: OK (3 cases)\n")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    self_test()

    root = load_toml("Cargo.toml")
    if root is None or "workspace" not in root:
        sys.stderr.write("check-workspace-exclude-list: no [workspace] in ./Cargo.toml\n")
        return 1
    excludes = root["workspace"].get("exclude", [])
    allow = read_allowlist()
    tracked = tracked_files()

    missing = [p for p in excludes if not os.path.isdir(p)]
    unjustified = []
    used_allow = set()
    for path in excludes:
        if not os.path.isdir(path):
            continue
        if reason_for(path, tracked) is not None:
            continue
        if path in allow:
            used_allow.add(path)
            continue
        unjustified.append(path)

    stale_allow = sorted(set(allow) - used_allow)

    if missing or unjustified:
        sys.stderr.write("check-workspace-exclude-list: FAIL\n\n")
        if missing:
            sys.stderr.write(
                "  %d exclude entry/entries name a directory that does not exist:\n"
                % len(missing)
            )
            for path in missing:
                sys.stderr.write("      %s\n" % path)
            sys.stderr.write(
                "\n  An exclude line for a deleted directory costs nothing at build\n"
                "  time, so nothing else will ever tell you. Delete the line.\n\n"
            )
        if unjustified:
            sys.stderr.write(
                "  %d exclude entry/entries satisfy no structural reason:\n"
                % len(unjustified)
            )
            for path in unjustified:
                sys.stderr.write("      %s\n" % path)
            sys.stderr.write(
                "\n  Either make it a workspace member, or — if it really cannot be\n"
                "  one — add it to %s with the reason.\n"
                "  That file may only SHRINK.\n\n" % ALLOWLIST
            )
        return 1

    note = ""
    if stale_allow:
        note = (
            "\ncheck-workspace-exclude-list: %d allowlist entry/entries are no longer\n"
            "needed (the entry is gone, or the tree now justifies it). Remove them —\n"
            "the list may only shrink:\n%s\n"
            % (len(stale_allow), "".join("  %s\n" % p for p in stale_allow))
        )
        sys.stdout.write(note)

    sys.stdout.write(
        "check-workspace-exclude-list: OK — %d exclude entry/entries, "
        "%d derived, %d declared.\n"
        % (len(excludes), len(excludes) - len(used_allow), len(used_allow))
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
