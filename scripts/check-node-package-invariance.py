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
  4. tracked-set completeness: every path a `CMakeLists.txt` or
     `system.toml` in scope names is in `git ls-files` (reach stated beside
     `untracked_refs`; `package.xml` names packages, never paths).

CHECKED ELSEWHERE, OVER THIS SCOPE (`--list-roots` prints it):
  2. every `<depend>` resolves — `packages/cli/nros-cli-core/tests/
     declared_depends_resolve.rs`, through the CLI's own ladder (no fast-line
     gate may exec the CLI, issue 0466; it runs in `check-cli-tests`).

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
    for f, why in untracked_refs(files, roots).items():
        found.setdefault(f, []).extend(why)
    return roots, pkgs, found


# --- Rule 4 — every path a build declaration names is TRACKED ---------------
#
# The static analogue of copying out: `check-template-copy-out` copies the
# TRACKED set and builds it, so a file that exists only in the author's
# worktree is what it catches. Asking "is every referenced path tracked" gets
# most of that with no build. NOT all: tracked content that is wrong still
# builds nowhere, and that residual (issue 1453) stays with the copy-out lane.
#
# REACH, stated so it is not over-read:
#   * `CMakeLists.txt` — a relative path with a source/data extension, bare or
#     under `${CMAKE_CURRENT_SOURCE_DIR}` / `${CMAKE_CURRENT_LIST_DIR}` /
#     `${PROJECT_SOURCE_DIR}`, and every `add_subdirectory(<dir>)`. A path under
#     any OTHER variable is a build product or a toolchain file, not ours.
#   * `system.toml` — `launch` / `default_launch` (under `launch/` or beside
#     the file) and `conf` fragments (anywhere under the bringup, which is
#     where the Zephyr overlay resolver looks once the application is
#     generated). `out` names OUTPUTS and `params_files` holds inline YAML.
#   * `package.xml` names packages, never paths: nothing to check.
PATH_EXT = r"(?:c|cc|cpp|cxx|h|hh|hpp|rs|msg|srv|action|idl|yaml|yml|xml|toml|conf|overlay|ld|py|json)"
CMAKE_PATH = re.compile(
    r'(?<![\w${}/.-])(?:\$\{(?:CMAKE_CURRENT_SOURCE_DIR|CMAKE_CURRENT_LIST_DIR|PROJECT_SOURCE_DIR)\}/)?'
    r'((?:\.\.?/)*[A-Za-z0-9_][A-Za-z0-9_.\-/]*\.' + PATH_EXT + r')(?![\w.-])')
ADD_SUBDIR = re.compile(
    r'add_subdirectory\(\s*"?(?:\$\{(?:CMAKE_CURRENT_SOURCE_DIR|CMAKE_CURRENT_LIST_DIR)\}/)?([^"\s)$]+)"?')


def _tracked(path, tracked, tracked_dirs):
    return path in tracked or path in tracked_dirs


def _norm(base, rel):
    return os.path.normpath(os.path.join(base, rel)).replace(os.sep, "/")


def cmake_refs(text):
    code = strip_hash_comments(text)
    refs = [m.group(1) for m in CMAKE_PATH.finditer(code)]
    subdirs = [m.group(1) for m in ADD_SUBDIR.finditer(code)]
    return refs, subdirs


def _toml_values(node, keys):
    if isinstance(node, dict):
        for k, v in node.items():
            if k in keys:
                for x in (v if isinstance(v, list) else [v]):
                    if isinstance(x, str):
                        yield k, x
            else:
                yield from _toml_values(v, keys)
    elif isinstance(node, list):
        for x in node:
            yield from _toml_values(x, keys)


def untracked_refs(files, roots):
    """{declaring file: [problem, ...]} for rule 4."""
    try:
        import tomllib
    except ModuleNotFoundError:  # Python < 3.11
        import tomli as tomllib
    tracked = set(files)
    tracked_dirs = set()
    for f in files:
        d = os.path.dirname(f)
        while d and d not in tracked_dirs:
            tracked_dirs.add(d)
            d = os.path.dirname(d)
    found = {}
    for f in files:
        if not any(f.startswith(r + "/") for r in roots):
            continue
        base = os.path.dirname(f)
        name = os.path.basename(f)
        if name == "CMakeLists.txt":
            try:
                text = open(os.path.join(ROOT, f), errors="replace").read()
            except OSError:
                continue
            refs, subdirs = cmake_refs(text)
            for r in refs:
                # A header is named as the code `#include`s it (`HEADER
                # ctrl_pkg/Ctrl.hpp`), so it resolves through the package's
                # include roots as well as beside the file.
                cands = [_norm(base, r)]
                if r.endswith((".h", ".hh", ".hpp")):
                    cands += [_norm(base, "include/" + r), _norm(base, "src/" + r)]
                if not any(_tracked(c, tracked, tracked_dirs) for c in cands):
                    found.setdefault(f, []).append(f"names `{r}`, which is not tracked")
            for d in subdirs:
                p = _norm(base, d)
                if (p + "/CMakeLists.txt") not in tracked:
                    found.setdefault(f, []).append(
                        f"add_subdirectory(`{d}`) has no tracked CMakeLists.txt")
        elif name == "system.toml":
            try:
                doc = tomllib.load(open(os.path.join(ROOT, f), "rb"))
            except (OSError, tomllib.TOMLDecodeError):
                continue
            under = [t for t in tracked if t.startswith(base + "/")]
            # `os_config` (alias `conf`) fragments, per image: the overlay resolver looks under the
            # bringup's board directory, then in the image's west APPLICATION
            # (a hand-written `[image.<id>] entry`, else `<id>_entry`), then in
            # the bringup (`builder/zephyr.rs::resolve_fragment`).
            for img_id, img in (doc.get("image") or {}).items():
                if not isinstance(img, dict):
                    continue
                app = _norm(base, "../" + (img.get("entry") or f"{img_id}_entry"))
                for v in [*(img.get("conf") or []), *(img.get("os_config") or [])]:
                    if v.endswith(".toml"):
                        continue  # a board network file, beside system.toml
                    ok = any(os.path.basename(t) == v for t in under) or \
                        any(t.startswith(app + "/") and os.path.basename(t) == v
                            for t in tracked)
                    if not ok:
                        found.setdefault(f, []).append(
                            f"[image.{img_id}] os_config `{v}` is tracked neither under the "
                            f"bringup nor in its application")
            for key, v in _toml_values(doc, {"launch", "default_launch"}):
                pkg_dir, file_ = base, v
                if ":" in v:
                    pkg, file_ = v.split(":", 1)
                    pkg_dir = _norm(base, "../" + pkg) if pkg else base
                cands = (_norm(pkg_dir, "launch/" + file_), _norm(pkg_dir, file_))
                if not any(c in tracked for c in cands):
                    found.setdefault(f, []).append(f"{key} `{v}` is not tracked")
    return found


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
    # Rule 4's reader: a path under OUR source dir counts, a build product
    # does not, a comment does not, and add_subdirectory names a directory.
    expect(cmake_refs("add_library(x src/a.c)\n# src/b.c\n") == (["src/a.c"], []),
           "rule 4 reads a source path and skips a commented one")
    expect(cmake_refs("target_sources(x PRIVATE ${CMAKE_CURRENT_BINARY_DIR}/gen.c)\n")
           == ([], []), "rule 4 ignores a build-dir product")
    expect(cmake_refs('add_library(x "${CMAKE_CURRENT_SOURCE_DIR}/src/a.cpp")\n')
           == (["src/a.cpp"], []), "rule 4 reads a CMAKE_CURRENT_SOURCE_DIR path")
    expect(cmake_refs('add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../ctrl_pkg" ctrl_pkg)\n')
           == ([], ["../ctrl_pkg"]), "rule 4 reads add_subdirectory")
    return ok


def main():
    # Issue 1509 rule 2 lives in the CLI's own tests (it needs the CLI's
    # `<depend>` ladder, and no fast-line gate may exec the CLI — issue 0466),
    # but its SCOPE is this one: it asks for the roots here rather than
    # re-deriving them.
    if "--list-roots" in sys.argv:
        roots, _ = scope(git_files())
        print("\n".join(roots))
        return 0
    if not self_test():
        return 1
    print("check-node-package-invariance self-test: OK (15 cases)")
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
        msgs = [m for v in new.values() for m in v]
        if any("tracked" in m for m in msgs):
            print("  Rule 4: a build declaration names a path git does not carry, so a\n"
                  "  clone or a copied-out template cannot build it. `git add` the file\n"
                  "  (or fix the reference).", file=sys.stderr)
        if any("tracked" not in m for m in msgs):
            print("  A node package must look the same on every platform and RMW: move the\n"
                  "  platform/RMW fact to the bringup (system.toml) or an *_entry package.",
                  file=sys.stderr)
        if new:
            print("  A justified exception goes in .config/node-package-invariance-baseline.txt\n"
                  "  with its reason.", file=sys.stderr)
        for p in stale:
            print(f"  {p}: baselined but no longer violates — delete its line", file=sys.stderr)
        return 1
    print(f"check-node-package-invariance: OK — {len(pkgs)} node package(s) in "
          f"{len(roots)} workspace/template root(s); {len(base)} baselined exception(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
