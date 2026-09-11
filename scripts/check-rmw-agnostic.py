#!/usr/bin/env python3
"""RFC-0071's `check-rmw-agnostic` — core and the API crates name no backend.

RFC-0071 § Verification states the rule and says it is "falsifiable by grep":

  * `packages/core/**` and `packages/api/nros{,-c,-cpp}/**` contain no
    `zenoh|xrce|cyclonedds|uorb` outside prose/comments;
  * `nros_rmw_dispatch` has no backend name in it.

Issue 1219: the gate was never written, and while the RFC's other waves landed
the closed backend lists grew from three to at least five. This is that gate,
with exactly the RFC's REACH — the two bullets above — across every language
in it (Rust, C/C++, CMake, TOML, Python), because two of the five lists issue
1219 counted were in neither Rust nor a manifest. Closed lists OUTSIDE that
reach (the CLI's scaffold, colcon's `RMW_BACKENDS`, the platform descriptor's
backend-named knob sections) are a different rule and are tracked by their own
issue, named in `BASELINE`'s header comment — not silently out of scope.

# What counts as a NAME

The canonical backend names, IMPORTED from `check-entry-rmw-vocabulary`'s
`cmake_known()` (each in-tree backend's first `<nano_ros_provides kind="rmw"/>`
announcement): a gate against second spellings must not carry one. A name
matches as a token PART — `nros_rmw_zenoh`, `CONFIG_NROS_ZENOH_LOCATOR`,
`rmw-cyclonedds` — but not inside another word (`uxrce_dds_client` is PX4's
client, not our backend).

# What is NOT code

  * comments, per language;
  * PROSE strings — a string literal containing whitespace is a message to a
    human (`"E2E message-integrity (CRC) — zenoh only"`); a whitespace-free
    literal (`"zenoh"`, `"nros-rmw-zenoh?/safety-e2e"`) is a value, and a value
    that names a backend is exactly what the rule is about;
  * test code — a `tests/` directory, or a Rust item under `#[cfg(test)]`.
    Issue 1219 measured the noise: a test that uses `"zenoh"` as sample data is
    a test PLAN, not dispatch, and a reader who has to learn to skip it stops
    reading the gate.

Everything else that names a backend is either an EXEMPTION (it names a
different axis that happens to share a word — each is `(path, name)`, so the
same file naming another backend is still caught) or DEBT in `BASELINE`, whose
every entry names an OPEN issue and whose counts may only fall.

Buildless; reads `git ls-files`. Runs its own self-test first, every time.
"""

from __future__ import annotations

import importlib.util
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
from issue_status import issue_status  # noqa: E402

# RFC-0071 § Verification, bullet 1 (directories) and bullet 2 (the dispatch).
SCOPE_DIRS = (
    "packages/core/",
    "packages/api/nros/",
    "packages/api/nros-c/",
    "packages/api/nros-cpp/",
)
SCOPE_FILES = ("cmake/NanoRosRmwDispatch.cmake",)

C_LIKE = {"rs", "c", "h", "cc", "cpp", "hpp", "hh"}
HASH = {"cmake", "toml", "py", "sh", "bash", "kconfig"}
HASH_NAMES = {"CMakeLists.txt", "Kconfig"}
# Not code: prose, data, lockfiles, message definitions.
SKIP_EXT = {"md", "json", "lock", "xml", "yaml", "yml", "msg", "srv", "action",
            "txt", "dox", "rst", "png", "svg", "html", "css", "clang-format"}

# (path, name) -> why this is NOT the RMW axis. An entry that matches nothing is
# a failure, so an exemption cannot outlive the code it describes.
EXEMPT = {
    # RFC-0088: `uorb` is also a SERIALIZATION FORMAT — the wire encoding a
    # message is laid out in — orthogonal to which RMW carries it. Issue 1219
    # counted these and did not indict them.
    ("packages/core/nros-serdes/src/format.rs", "uorb"):
        "serialization-format axis (RFC-0088), not the RMW axis",
    # (`packages/core/nros-node/src/format_check.rs` needs no entry: its `Uorb`
    # uses are all inside `#[cfg(test)]`, which is already not code here. It
    # HAD one until the "an exemption that matches nothing is a failure" rule
    # deleted it, which is the rule earning its place on its first run.)
    ("packages/api/nros-c/include/nros/serialization_format.h", "uorb"):
        "serialization-format axis (RFC-0088), not the RMW axis",
    ("packages/api/nros-cpp/include/nros/serialization_format.hpp", "uorb"):
        "serialization-format axis (RFC-0088), not the RMW axis",
}

# path -> (offending code lines, OPEN issue, what it is). A RATCHET: a count
# may only FALL, and a count that fell must be lowered here in the same change
# (a stale entry fails), so the debt cannot silently regrow into headroom.
#
# Closed backend lists OUTSIDE this gate's reach — `cargo-nano-ros`'s
# `workspace_scaffold.rs`, `colcon_nano_ros`'s `RMW_BACKENDS`, the platform
# descriptor's `[knobs.zenoh.tx]`, `bridge_gen.rs` — are a DIFFERENT rule and
# are tracked by issue 1300, not silently out of scope.
#
# 70 lines over 13 files, classified 2026-09-11 by reading every one of them;
# `--list` prints them. `packages/api/nros-c/cmake/NanoRosLink.cmake` was a
# fourteenth (4 lines) and is DELETED rather than baselined — it was dead
# (issue 1218).
BASELINE: dict[str, tuple[int, str, str]] = {
    # Issue 1297 — the API crates select and register backends BY NAME.
    "packages/api/nros-c/Cargo.toml":
        (16, "1297", "per-backend features, cffi aliases, `?/` forwarding, optional deps"),
    "packages/api/nros-cpp/Cargo.toml":
        (8, "1297", "the twin of nros-c's feature table"),
    "packages/api/nros-c/src/rmw_backend.rs":
        (4, "1297", "`#[cfg(feature = \"rmw-<x>\")] nros_rmw_<x>::register()`"),
    "packages/api/nros-cpp/src/rmw_backend.rs":
        (4, "1297", "the twin"),
    "packages/api/nros-c/src/lib.rs":
        (1, "1297", "`cfg(any(feature = \"rmw-zenoh\", feature = \"rmw-xrce\"))`"),
    "packages/api/nros-cpp/src/lib.rs":
        (4, "1297", "the same cfg(any(...)) four times"),
    "packages/api/nros-cpp/include/nros/node.hpp":
        (8, "1297", "`#ifdef NROS_RMW_<X>` + `extern \"C\" nros_rmw_<x>_register()`, all four"),
    "packages/api/nros/Cargo.toml":
        (1, "1297", "`rmw-cyclonedds` naming a capability forward"),
    # Issue 1298 — core names backends in code.
    "packages/core/nros-macros/src/main_macro.rs":
        (5, "1298", "`rmw_crate_ident`'s closed match + the hardcoded cyclonedds register"),
    "packages/core/nros-orchestration-ir/src/lib.rs":
        (1, "1298", "`pub mod cyclonedds_type_sizing;`"),
    # Issue 1299 — backend-named configuration inside the API crates (D8).
    "packages/api/nros-c/include/nros/entry_config.h":
        (6, "1299", "locator/domain from CONFIG_NROS_{ZENOH,XRCE,CYCLONE}_*"),
    "packages/api/nros-c/include/nros/zephyr/app_config.h":
        (9, "1299", "a `.zenoh` struct member and its task knobs"),
    "packages/api/nros/src/env.rs":
        (3, "1299", "the deprecated ZENOH_LOCATOR / ZENOH_MODE aliases"),
}


def backend_names() -> set[str]:
    """The canonical names, from the one reader of the announcements."""
    path = os.path.join(ROOT, "scripts", "check-entry-rmw-vocabulary.py")
    spec = importlib.util.spec_from_file_location("_vocab", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return set(mod.cmake_known())


def name_re(names) -> re.Pattern:
    alt = "|".join(sorted((re.escape(n) for n in names), key=len, reverse=True))
    return re.compile(rf"(?i)(?<![a-z0-9])({alt})(?![a-z])")


def _kind(path: str) -> str | None:
    base = path.rsplit("/", 1)[-1]
    if base in HASH_NAMES:
        return "hash"
    parts = base.split(".")
    # `foo.rs.template`, `foo.h.in`: the INNER extension says what it is.
    if len(parts) >= 3 and parts[-1] in ("template", "in"):
        ext = parts[-2]
    elif len(parts) >= 2:
        ext = parts[-1]
    else:
        return "hash"
    ext = ext.lower()
    if ext in C_LIKE:
        return "c"
    if ext in HASH:
        return "hash"
    if ext in SKIP_EXT:
        return None
    return "hash"


def _string_end(t: str, i: int, q: str) -> int:
    """Index just past the literal opening at `t[i] == q`."""
    j = i + 1
    while j < len(t) and t[j] != q:
        if t[j] == "\\":
            j += 1
        elif t[j] == "\n" and q == "'":
            return i + 1  # not a char literal after all (a Rust lifetime)
        j += 1
    return j + 1


def _prose_or_value(lit: str) -> str:
    """A literal with whitespace in it is prose: keep its newlines, drop text."""
    body = lit[1:-1]
    if re.search(r"\s", body):
        return lit[0] + "\n" * body.count("\n") + lit[-1]
    return lit


def strip_c(t: str) -> str:
    out, i, n = [], 0, len(t)
    while i < n:
        if t.startswith("//", i):
            j = t.find("\n", i)
            i = n if j < 0 else j
            continue
        if t.startswith("/*", i):
            j = t.find("*/", i + 2)
            j = n if j < 0 else j + 2
            out.append("\n" * t.count("\n", i, j))
            i = j
            continue
        m = re.match(r'r(#*)"', t[i:i + 8]) if t[i] == "r" and (i == 0 or not t[i - 1].isalnum() and t[i - 1] != "_") else None
        if m:
            close = '"' + m.group(1)
            j = t.find(close, i + len(m.group(0)))
            j = n if j < 0 else j + len(close)
            out.append(_prose_or_value('"' + t[i + len(m.group(0)): j - len(close)] + '"'))
            i = j
            continue
        if t[i] == '"':
            j = _string_end(t, i, '"')
            out.append(_prose_or_value(t[i:j]))
            i = j
            continue
        if t[i] == "'":
            # A char literal is at most an escape plus a few chars; a Rust
            # lifetime (`'a`) has no closing quote, so it passes through.
            m = re.match(r"'(\\.[^']{0,8}|[^'\\\n])'", t[i:i + 12])
            if m:
                out.append("''")
                i += len(m.group(0))
                continue
        out.append(t[i])
        i += 1
    return "".join(out)


def strip_hash(t: str) -> str:
    out = []
    for line in t.split("\n"):
        res, i, n = [], 0, len(line)
        while i < n:
            c = line[i]
            if c == "#":
                break
            if c in "\"'":
                j = i + 1
                while j < n and line[j] != c:
                    j += 2 if line[j] == "\\" else 1
                res.append(_prose_or_value(line[i:j + 1]) if j < n else line[i:])
                i = j + 1
                continue
            res.append(c)
            i += 1
        out.append("".join(res))
    return "\n".join(out)


_CFG_TEST = re.compile(r"#\[cfg\((?:all\(|any\()?\s*test\b[^\]]*\]")


def strip_cfg_test(code: str) -> str:
    """Blank every Rust item under `#[cfg(test)]`, keeping line numbers."""
    out = code
    pos = 0
    while True:
        m = _CFG_TEST.search(out, pos)
        if not m:
            return out
        i = m.end()
        brace = out.find("{", i)
        semi = out.find(";", i)
        if brace < 0 or (0 <= semi < brace):
            end = (semi + 1) if semi >= 0 else len(out)
        else:
            depth, j = 0, brace
            while j < len(out):
                if out[j] == "{":
                    depth += 1
                elif out[j] == "}":
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            end = j + 1
        blank = "".join("\n" if ch == "\n" else " " for ch in out[m.start():end])
        out = out[:m.start()] + blank + out[end:]
        pos = m.start() + 1


def code_of(path: str, text: str) -> str | None:
    kind = _kind(path)
    if kind is None:
        return None
    if kind == "c":
        code = strip_c(text)
        if path.endswith(".rs") or ".rs." in path:
            code = strip_cfg_test(code)
        return code
    return strip_hash(text)


def in_scope(path: str) -> bool:
    if path in SCOPE_FILES:
        return True
    if not path.startswith(SCOPE_DIRS):
        return False
    return "/tests/" not in path and "/generated/" not in path


def scan_text(path: str, text: str, rx: re.Pattern, exempt=None) -> list[tuple[int, str, list[str]]]:
    """[(line, code, names)] for every code line naming a non-exempt backend."""
    exempt = EXEMPT if exempt is None else exempt
    code = code_of(path, text)
    if code is None:
        return []
    hits = []
    for ln, line in enumerate(code.split("\n"), 1):
        names = sorted({m.group(1).lower() for m in rx.finditer(line)}
                       - {n for (p, n) in exempt if p == path})
        if names:
            hits.append((ln, line.strip(), names))
    return hits


def exempt_used(path: str, text: str, rx: re.Pattern) -> set[tuple[str, str]]:
    code = code_of(path, text) or ""
    return {(path, m.group(1).lower()) for m in rx.finditer(code)}


def tracked() -> list[str]:
    r = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "--", *SCOPE_DIRS, *SCOPE_FILES],
        capture_output=True, text=True, check=False,
    )
    if r.returncode != 0:
        sys.exit(f"check-rmw-agnostic: `git ls-files` failed:\n  {r.stderr.strip()}")
    return sorted(p for p in r.stdout.splitlines() if in_scope(p))


def verdict(counts: dict[str, int], baseline, status_of=issue_status) -> list[str]:
    """The ratchet: every problem with `counts` against `baseline`."""
    bad = []
    for path, n in sorted(counts.items()):
        if path not in baseline:
            bad.append(f"{path}: {n} code line(s) name a backend, and it is not "
                       "baselined — make it ask a descriptor/capability instead")
            continue
        want, issue, _what = baseline[path]
        if n > want:
            bad.append(f"{path}: {n} code line(s) name a backend, baseline {want} "
                       f"(issue {issue}) — the debt GREW")
        elif n < want:
            bad.append(f"{path}: {n} code line(s), baseline {want} — it SHRANK; "
                       f"lower the baseline to {n} in this change")
    for path, (want, issue, _what) in sorted(baseline.items()):
        if path not in counts:
            bad.append(f"{path}: baselined at {want} but names no backend now — "
                       "delete the entry")
        st = status_of(issue)
        if st != "open":
            bad.append(f"{path}: baseline names issue {issue}, which is "
                       + (f"`{st}`" if st else "not a file under docs/issues/")
                       + " — debt must be tracked by an OPEN issue")
    return bad


def self_test(rx: re.Pattern) -> list[str]:
    bad = []
    cases = [
        # (path, text, expected offending line count)
        ("packages/core/x/src/a.rs", 'let r = "zenoh";\n', 1),
        ("packages/core/x/src/a.rs", "// zenoh is great\n/* xrce\n */ let a = 1;\n", 0),
        ("packages/core/x/src/a.rs", 'panic!("only zenoh has this");\n', 0),
        ("packages/core/x/src/a.rs", "#[cfg(test)]\nmod t {\n    const A: &str = \"uorb\";\n}\n", 0),
        ("packages/core/x/src/a.rs", "#[cfg(test)]\nuse nros_rmw_zenoh as z;\nfn f() { nros_rmw_zenoh::register(); }\n", 1),
        ("packages/core/x/src/a.rs", "fn px4_matches_uxrce_dds_client() {}\n", 0),
        ("packages/core/x/src/a.rs", "let c = '\"'; let s = \"cyclonedds\";\n", 1),
        ("packages/core/x/src/a.rs", 'let s = r#"zenoh"#;\n', 1),
        ("packages/core/x/src/a.rs", "fn f<'a>(x: &'a str) -> &'a str { x }\nlet z = nros_rmw_zenoh::X;\n", 1),
        ("packages/api/nros-c/cmake/X.cmake", 'if(RMW STREQUAL "zenoh")  # xrce too\n', 1),
        ("packages/api/nros-c/cmake/X.cmake", 'option(E "CRC — zenoh only" OFF)\n', 0),
        ("packages/api/nros-c/Cargo.toml", 'rmw-zenoh = ["dep:nros-rmw-zenoh"]\n# rmw-xrce\n', 1),
        ("packages/api/nros-c/include/nros/x.h", "#ifdef CONFIG_NROS_ZENOH_LOCATOR\n", 1),
        ("packages/api/nros/x.py", 'B = ("zenoh", "xrce")\n"""a zenoh docstring"""\n', 1),
    ]
    for path, text, want in cases:
        got = len(scan_text(path, text, rx, exempt={}))
        if got != want:
            bad.append(f"scan case {text!r} in {path}: {got} offending line(s), want {want}")
    # An exemption is per (path, NAME): the exempt name passes, another backend
    # in the same file is still caught.
    ex = {("p/f.rs", "uorb"): "planted"}
    if len(scan_text("p/f.rs", 'let a = "uorb";\n', rx, ex)) != 0:
        bad.append("an exempt (path, name) was reported")
    if len(scan_text("p/f.rs", 'let a = "zenoh";\n', rx, ex)) != 1:
        bad.append("an exemption for one name hid a DIFFERENT backend in the same file")
    # Scope: tests/ and generated/ are out, the dispatch file is in.
    for p, want in (("packages/core/a/tests/t.rs", False), ("packages/core/a/src/l.rs", True),
                    ("packages/cli/x.rs", False), ("cmake/NanoRosRmwDispatch.cmake", True),
                    ("packages/api/nros-cffi/x.rs", False), ("packages/api/nros/src/l.rs", True)):
        if in_scope(p) != want:
            bad.append(f"in_scope({p!r}) = {not want}, want {want}")
    # The ratchet, with a planted resolver.
    fake = {"1219": "open", "0776": "resolved"}.get
    base = {"a": (2, "1219", "x")}
    for counts, n_bad, label in (({"a": 2}, 0, "at baseline"),
                                 ({"a": 3}, 1, "grew"),
                                 ({"a": 1}, 1, "shrank without lowering"),
                                 ({}, 1, "gone but still baselined"),
                                 ({"a": 2, "b": 1}, 1, "a new file")):
        got = verdict(counts, base, fake)
        if len(got) != n_bad:
            bad.append(f"ratchet {label}: {len(got)} problem(s), want {n_bad}: {got}")
    if len(verdict({"a": 2}, {"a": (2, "0776", "x")}, fake)) != 1:
        bad.append("a baseline row naming a RESOLVED issue was accepted")
    # The real tree, mutated: a clean core source gains one backend literal.
    probe = "packages/core/nros-rmw/src/lib.rs"
    try:
        text = open(os.path.join(ROOT, probe), encoding="utf-8").read()
    except OSError:
        bad.append(f"negative-control probe {probe} is gone — pick another clean core file")
    else:
        before = len(scan_text(probe, text, rx))
        after = len(scan_text(probe, text + '\npub const BACKEND: &str = "zenoh";\n', rx))
        if after != before + 1:
            bad.append(f"a backend literal appended to {probe} was not caught ({before} -> {after})")
    return bad


def main(argv) -> int:
    names = backend_names()
    rx = name_re(names)
    bad = self_test(rx)
    if bad:
        for b in bad:
            sys.stderr.write("check-rmw-agnostic --self-test: " + b + "\n")
        return 2
    if "--self-test" in argv:
        print("check-rmw-agnostic --self-test: OK")
        return 0

    counts, detail, used = {}, {}, set()
    for path in tracked():
        try:
            text = open(os.path.join(ROOT, path), encoding="utf-8", errors="replace").read()
        except (IsADirectoryError, FileNotFoundError):
            continue  # an uninitialised submodule, or a deleted-but-staged path
        hits = scan_text(path, text, rx)
        used |= exempt_used(path, text, rx)
        if hits:
            counts[path] = len(hits)
            detail[path] = hits

    if "--list" in argv:
        for path in sorted(detail):
            print(f"{counts[path]:4d} {path}")
            for ln, line, nm in detail[path]:
                print(f"       {ln}: [{','.join(nm)}] {line[:120]}")
        return 0

    problems = verdict(counts, BASELINE)
    for key in sorted(set(EXEMPT) - used):
        problems.append(f"EXEMPT{key!r} matches nothing — delete the entry")
    if problems:
        sys.stderr.write("check-rmw-agnostic: FAIL — RFC-0071: core and the API crates "
                         "must name no backend outside prose/comments\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
            path = p.split(":", 1)[0]
            for ln, line, nm in detail.get(path, [])[:20]:
                sys.stderr.write(f"      {ln}: [{','.join(nm)}] {line[:110]}\n")
        sys.stderr.write(
            "\n  A core crate receives CAPABILITIES, never backend names (RFC-0071\n"
            "  D2); a backend declares itself in its `nros-rmw.toml` (D1). If the\n"
            "  name is a different axis that shares a word, add an EXEMPT\n"
            "  (path, name) with the reason. `--list` prints every hit.\n"
        )
        return 1
    debt = sum(v[0] for v in BASELINE.values())
    print(f"check-rmw-agnostic: OK — names {sorted(names)}; {len(counts)} file(s) "
          f"carry {debt} baselined line(s) across "
          f"{len({v[1] for v in BASELINE.values()})} open issue(s), "
          f"{len(EXEMPT)} exemption(s), no new leak")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
