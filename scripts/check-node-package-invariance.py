#!/usr/bin/env python3
"""A node package names no platform and no RMW — issue 1509 (phase-477 W4).

A workspace has three kinds of package and only one may know what it runs on:
the NODE package (application logic), the ENTRY package (`*_entry`, the
platform seam) and the BRINGUP (`system.toml` + launch files, the
declaration). Measured true on 2026-09-27 over every bringup-shaped workspace,
and kept true by nothing until this gate.

SCOPE (decided in the issue, not here):
  * roots: every `examples/workspaces/<ws>` and every `examples/templates/...`
    directory whose TRACKED `system.toml` declares an `[image.*]` — the
    predicate `check-template-copy-out.sh --list` uses, asked of the file,
    never of a directory name (two templates are Form-1 self-bringups);
  * a package is a directory holding a tracked `package.xml`; entries
    (`*_entry`) are exempt — the platform seam belongs there;
  * `zephyr-byo` is exempt BY NAME (platform-specific by design), as are the
    per-platform standalone examples (`examples/{native,zephyr,…}`), which
    exist to show platform integration.

RULES CHECKED HERE:
  1. no platform/RMW token in node-package CODE. Comments are stripped first —
     measured, a raw-text rule failed 12 legitimate files on day one and a
     code-only rule fails 0. Reach: `*.rs *.c *.h *.cc *.cpp *.hpp *.hh *.cxx`
     plus `CMakeLists.txt` and `Cargo.toml` (`#` comments). A `build.rs` in a
     node package is reported as such: a node package that needs a build
     script is usually an entry package in disguise.
  3. shape: `.colcon_workspace` is tracked at each `examples/workspaces/*`
     root, and no node package commits a `generated/` tree. The other shape
     rules have owners already and are NOT restated:
     `check-workspace-root-build-files` (no root build file) and
     `check-leaf-lockfiles` (the lock invariant).

NOT CHECKED HERE (issue 1509 stays open for them):
  2. every `<depend>` resolves — needs the CLI's `prereq_resolve::classify`
     ladder, which no fast-line artifact exposes;
  4. tracked-set completeness of referenced paths.
And no static rule covers issue 1453's residual: tracked content that is
complete and still does not build is the copy-out lane's question.

Exceptions: `.config/node-package-invariance-baseline.txt`, one
`<path>  # <reason>` per line, RATCHETED both ways — an unlisted violation
fails, and a listed path that no longer violates fails until its line goes.

  check-node-package-invariance.py [--self-test]
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join(ROOT, ".config", "node-package-invariance-baseline.txt")

# Platform / RMW tokens. Case-insensitive SUBSTRING match on code, so
# `nros_rmw_zenoh`, `CONFIG_NROS_ZEPHYR` and `zpico_*` are caught too.
TOKENS = (
    "zenoh", "zpico", "cyclonedds", "cyclone", "xrce", "zephyr", "freertos",
    "nuttx", "threadx", "netxduo", "esp32", "esp_idf", "picolibc", "native_sim",
    "mps2", "mps3", "stm32", "smoltcp", "lwip", "px4", "uorb",
)
EXEMPT_ROOTS = {"examples/templates/zephyr-byo"}
CODE_EXT = (".rs", ".c", ".h", ".cc", ".cpp", ".hpp", ".hh", ".cxx")
HASH_COMMENT_FILES = ("CMakeLists.txt", "Cargo.toml")


def git_files():
    out = subprocess.run(["git", "-C", ROOT, "ls-files", "-z", "examples"],
                         capture_output=True, check=True).stdout
    return [p.decode() for p in out.split(b"\0") if p]


def strip_c_comments(text):
    """Drop `//` and `/* */` comments, keeping string and char literals."""
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        # A `'` opens a char literal only as `'x'` or `'\…'`; otherwise it is
        # a Rust lifetime (`'a`, `'static`) and must not swallow the code
        # after it — which is how a comment after a lifetime read as code.
        if c == "'" and not (text.startswith("\\", i + 1) or text[i + 2:i + 3] == "'"):
            out.append(c)
            i += 1
        elif c in "\"'":
            j = i + 1
            while j < n and text[j] != c:
                j += 2 if text[j] == "\\" else 1
            out.append(text[i:j + 1])
            i = j + 1
        elif text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
        elif text.startswith("/*", i):
            j = text.find("*/", i + 2)
            out.append(" ")
            i = n if j < 0 else j + 2
        else:
            out.append(c)
            i += 1
    return "".join(out)


def strip_hash_comments(text):
    return "\n".join(re.sub(r"#.*$", "", line) for line in text.splitlines())


def code_tokens(path, text):
    base = os.path.basename(path)
    if base in HASH_COMMENT_FILES:
        code = strip_hash_comments(text)
    elif path.endswith(CODE_EXT):
        code = strip_c_comments(text)
    else:
        return []
    low = code.lower()
    return sorted({t for t in TOKENS if t in low})


def scope(files):
    """(roots, {package_dir: [files]}) for node packages in scope."""
    tracked = set(files)
    roots = set()
    for f in files:
        parts = f.split("/")
        if len(parts) >= 3 and parts[1] == "workspaces" and len(parts) > 3:
            roots.add("/".join(parts[:3]))
        if parts[1] == "templates" and parts[-1] == "system.toml":
            try:
                body = open(os.path.join(ROOT, f), errors="replace").read()
            except OSError:
                continue
            if re.search(r"(?m)^\[image\.", body):
                # The template root is the directory under examples/templates/
                # that holds this bringup's workspace: its first two levels,
                # or three for a per-platform template (`<t>/<platform>/…`).
                t = "/".join(parts[:3])
                sub = "/".join(parts[:4])
                roots.add(sub if os.path.isfile(os.path.join(ROOT, sub, ".colcon_workspace"))
                          or (sub + "/src") in {os.path.dirname(x) for x in tracked if x.startswith(sub + "/src/")}
                          else t)
    roots -= EXEMPT_ROOTS
    pkgs = {}
    pkg_dirs = sorted({os.path.dirname(f) for f in files
                       if os.path.basename(f) == "package.xml"
                       and any(f.startswith(r + "/") for r in roots)},
                      key=len, reverse=True)
    for d in pkg_dirs:
        # `*_entry` and `*_entry_*` (`zephyr_entry_robot1`): `_entry` as a
        # whole segment of the package name.
        if re.search(r"(^|_)entry(_|$)", os.path.basename(d)):
            continue
        pkgs[d] = []
    for f in files:
        owner = next((d for d in pkg_dirs if f.startswith(d + "/")), None)
        if owner in pkgs:
            pkgs[owner].append(f)
    return sorted(roots), pkgs


def violations(files):
    roots, pkgs = scope(files)
    found = {}
    for d, fs in pkgs.items():
        for f in fs:
            rel = f[len(d) + 1:]
            if "/generated/" in "/" + rel:
                found.setdefault(f, []).append("committed generated/ tree")
                continue
            if os.path.basename(f) == "build.rs":
                found.setdefault(f, []).append("build.rs in a node package")
            try:
                text = open(os.path.join(ROOT, f), errors="replace").read()
            except OSError:
                continue
            toks = code_tokens(f, text)
            if toks:
                found.setdefault(f, []).append("token(s) in code: " + ", ".join(toks))
    for r in roots:
        if r.startswith("examples/workspaces/") and (r + "/.colcon_workspace") not in set(files):
            found.setdefault(r, []).append("no tracked .colcon_workspace")
    return roots, pkgs, found


def load_baseline():
    out = {}
    if not os.path.exists(BASELINE):
        return out
    for line in open(BASELINE):
        line = line.rstrip("\n")
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        path, _, reason = line.partition("#")
        out[path.strip()] = reason.strip()
    return out


def self_test():
    """Negative controls: rule 1 must catch code and ignore comments."""
    ok = True
    def expect(cond, what):
        nonlocal ok
        if not cond:
            ok = False
            print(f"check-node-package-invariance self-test FAILED: {what}", file=sys.stderr)
    expect(code_tokens("a/lib.rs", "use nros_rmw_zenoh as _;\n") == ["zenoh"],
           "a backend `use` in code is caught")
    expect(code_tokens("a/lib.rs", "// zenoh does X\n/// cyclonedds\nfn f() {}\n") == [],
           "Rust line/doc comments are ignored")
    expect(code_tokens("a/x.c", "/* zephyr\n picolibc */ int x;\n") == [],
           "C block comments are ignored")
    expect(code_tokens("a/x.cpp", 'const char* s = "freertos";\n') == ["freertos"],
           "a string literal is code")
    expect(code_tokens("a/x.c", "char c = '/'; /* nuttx */ int y;\n") == [],
           "a '/' char literal does not start a comment")
    expect(code_tokens("a/CMakeLists.txt", "# zephyr\nadd_library(x)\n") == [],
           "CMake # comments are ignored")
    expect(code_tokens("a/Cargo.toml", 'nros-rmw-zenoh = { path = ".." }\n') == ["zenoh"],
           "a backend dependency in Cargo.toml is caught")
    expect(code_tokens("a/lib.rs", "fn f<'a>(x: &'a str) {}\n// zenoh\n") == [],
           "a comment after a Rust lifetime is still a comment")
    expect(code_tokens("a/lib.rs", "let c = '\\''; // zenoh\nlet d = 'z';\n") == [],
           "escaped and plain char literals are skipped whole")
    entry = re.compile(r"(^|_)entry(_|$)")
    expect(all(entry.search(n) for n in ("zephyr_entry", "zephyr_entry_robot1", "entry")),
           "every entry spelling is recognised")
    expect(not any(entry.search(n) for n in ("sentry_pkg", "entrypoint_pkg", "talker_pkg")),
           "a name merely CONTAINING 'entry' is not an entry")
    return ok


def main():
    if not self_test():
        return 1
    print("check-node-package-invariance self-test: OK (11 cases)")
    if "--self-test" in sys.argv:
        return 0
    files = git_files()
    roots, pkgs, found = violations(files)
    base = load_baseline()
    new = {p: v for p, v in found.items() if p not in base}
    stale = [p for p in base if p not in found]
    if new or stale:
        print("check-node-package-invariance: FAILED (issue 1509)", file=sys.stderr)
        for p, why in sorted(new.items()):
            print(f"  {p}: {'; '.join(why)}", file=sys.stderr)
        if new:
            print("  A node package must look the same on every platform and RMW: move the\n"
                  "  platform/RMW fact to the bringup (system.toml) or an *_entry package. A\n"
                  "  justified exception goes in .config/node-package-invariance-baseline.txt\n"
                  "  with its reason.", file=sys.stderr)
        for p in stale:
            print(f"  {p}: baselined but no longer violates — delete its line", file=sys.stderr)
        return 1
    print(f"check-node-package-invariance: OK — {len(pkgs)} node package(s) in "
          f"{len(roots)} workspace/template root(s); {len(base)} baselined exception(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
