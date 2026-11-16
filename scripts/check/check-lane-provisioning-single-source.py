#!/usr/bin/env python3
"""A lane's provisioning has ONE table and ONE reader (phase-565 W1).

What the `submodules` claim provisions is `[[provision.submodule]]` in
`.config/gate-lanes.toml`, read only by `scripts/lib/lane_provision.py`. Until
W1 those rows were a second file, `.config/worktree-provisioning.txt`, parsed
by hand in three places (`setup-worktree`, the lane preflight, CI's
provisioner), so a worktree's provisioning and a lane's claims could disagree
and nothing would say so. This refuses either shape coming back:

  * a code line that reads the retired list;
  * a code line outside the helper that parses the table itself.

Comments may name either; docs are not scanned. Run with --self-test for the
negative controls (they also run first on the normal path).
"""

import re
import subprocess
import sys

HELPER = "scripts/lib/lane_provision.py"
SELF = "scripts/check/check-lane-provisioning-single-source.py"
ALLOWED = {HELPER, SELF, ".config/gate-lanes.toml"}
CODE = ("*.py", "*.sh", "*.just", "justfile", "*.yml", "*.yaml", "*.rs", "*.bash")
COMMENT = re.compile(r"^\s*(#|//)")

RETIRED = re.compile(r"worktree-provisioning\.txt")
# The table parsed in place: `["provision"]["submodule"]`, `.get("provision")`,
# `provision.submodule` as a key path, or the TOML header in a code string.
PARSE = re.compile(
    r"""\[\s*['"]provision['"]\s*\]"""
    r"""|\.get\(\s*['"]provision['"]"""
    # A key path (`yq .provision.submodule`), not the `[[provision.submodule]]`
    # header a message names to say where a row lives.
    r"""|(?<!\[\[)\bprovision\.submodule\b"""
)


def problems(name, text):
    if name in ALLOWED:
        return []
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        if COMMENT.match(line):
            continue
        if RETIRED.search(line):
            out.append(f"{name}:{n}: reads the retired .config/worktree-provisioning.txt: {line.strip()}")
        elif PARSE.search(line):
            out.append(f"{name}:{n}: parses the provisioning table itself: {line.strip()}")
    return out


def self_test():
    bad = 0

    def case(desc, text, refused):
        nonlocal bad
        got = bool(problems("x.py", text))
        ok = got == refused
        print(f"  {'ok  ' if ok else 'FAIL'} {desc}: {'refused' if got else 'accepted'}")
        bad += not ok

    case("a call to the helper", 'subprocess.run(["python3", "scripts/lib/lane_provision.py", "submodules"])\n', False)
    case("a comment naming the table", "# rows of [[provision.submodule]] in gate-lanes.toml\n", False)
    case("a comment naming the retired list", "# was .config/worktree-provisioning.txt (issue 1513)\n", False)
    case("error text naming the table header",
         'echo "fix its [[provision.submodule]] row in .config/gate-lanes.toml"\n', False)
    case("a sed over the retired list", "sed -e 's/#.*//' .config/worktree-provisioning.txt\n", True)
    # Negative control: a SECOND reader of the table, the shape W1 removed.
    case("negative control: a second reader of the table",
         'rows = tomllib.load(f)["provision"]["submodule"]\n', True)
    case("negative control: a .get() reader", 'decl.get("provision", {}).get("submodule")\n', True)
    case("negative control: a key-path reader", "yq '.provision.submodule[].path' .config/gate-lanes.toml\n", True)
    if problems(HELPER, 'decl.get("provision", {})\n'):
        print("  FAIL the helper itself is refused")
        bad += 1
    else:
        print("  ok   the helper itself may parse the table")
    if bad:
        print("check-lane-provisioning-single-source self-test: FAILED", file=sys.stderr)
        return 1
    return 0


def main(argv):
    if argv == ["--self-test"]:
        return self_test()
    import contextlib
    import io
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        rc = self_test()
    if rc:
        sys.stderr.write(buf.getvalue())
        return 1
    files = subprocess.run(["git", "ls-files", "-z", *CODE], capture_output=True,
                           check=True).stdout.decode().split("\0")
    found = []
    for f in filter(None, files):
        try:
            found += problems(f, open(f, encoding="utf-8").read())
        except (FileNotFoundError, UnicodeDecodeError, IsADirectoryError):
            continue
    if found:
        print("check-lane-provisioning-single-source: FAILED (phase-565 W1)", file=sys.stderr)
        for p in found:
            print("  " + p, file=sys.stderr)
        print("\nRead what a lane provisions through scripts/lib/lane_provision.py\n"
              "(`submodules [--tiers ...]`, `lane <name>`): it is the one reader of\n"
              "[[provision.submodule]] in .config/gate-lanes.toml.", file=sys.stderr)
        return 1
    print("check-lane-provisioning-single-source: OK (one provisioning table, one reader)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
