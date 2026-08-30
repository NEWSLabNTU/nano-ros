#!/usr/bin/env python3
"""`nano_ros_workspace(BACKEND …)` and the named bringup's `[system].rmw` must agree.

Issue 0934 R1 / phase-405 W2. `rmw` is authored in five places and two of them
meet in ONE cmake call:

    nano_ros_workspace(
        BACKEND  zenoh          # <- authored here
        SYSTEM   demo_bringup)  # <- and again in src/demo_bringup/system.toml

`cmake/NanoRosWorkspace.cmake` builds the path to that `system.toml`
(`:351`, as a `CMAKE_CONFIGURE_DEPENDS`) and never parses it. Nothing anywhere
compares the two values. A user who edits one and not the other gets no
diagnostic at all: cmake stamps `NROS_RMW` / `NANO_ROS_RMW` from `BACKEND`, so
the C/C++ lane links what BACKEND says, while the CLI resolves the entry's
backend through `resolved_rmw` (`cargo_metadata_schema.rs:915`), which never
sees `BACKEND`. Two authored copies of one fact, two resolution paths, no
stated precedence.

## What this gate does and deliberately does not do

It does NOT fix the duplication — deciding which side wins is W2's follow-up and
needs a design decision. It makes a DISAGREEMENT impossible to land silently,
which is the same posture `scripts/check-zephyr-knob-agreement.py` takes for the
zenoh tx trio, and carries the same caveat that file states about itself:

    Not a substitute for merging the sources.

## The finding classes, and why only one of them is fatal

  agree          both sides authored, same value. The state today.
  disagree       both sides AUTHORED and DIFFERENT.  ** FATAL **
  missing-toml   SYSTEM names a bringup with no `system.toml` under the
                 workspace root.  ** FATAL ** — cmake itself FATAL_ERRORs on
                 this a few lines later ("no bringup pkg named …"), so failing
                 here is only earlier, not stricter.
  silent-drift   exactly ONE side authored. Reported, not fatal.
  no-system      `BACKEND` with no `SYSTEM`: cmake reads no `system.toml` at
                 all, so there is nothing it could disagree with. Reported with
                 any bringup the workspace nevertheless ships, because that is
                 where the next disagreement comes from.
  unresolvable   `BACKEND ${NROS_RMW}` / `SYSTEM ${x}` / a `${…}` workspace
                 root — the value is not decidable without configuring. Reported.

`silent-drift` is REPORTED rather than failed on purpose, and the reason is
worth writing down because it is tempting to promote it. Both sides have a
default and the defaults happen to be the same word: `nano_ros_workspace`
defaults `BACKEND` to `zenoh` (`NanoRosWorkspace.cmake:254`) and `resolved_rmw`
defaults an empty `[system].rmw` to `zenoh` too. So a one-sided declaration is
only a live split when the authored side is NOT zenoh — and turning that into a
failure is exactly the "which side wins" decision this wave was told not to
make. It is named on every run so it cannot accumulate unseen.

`[image_defaults].rmw` and `[image.<id>].rmw` are read and printed, never
compared for equality. An image is a per-TARGET axis and legitimately varies:
`examples/workspaces/rust/src/demo_bringup/system.toml` declares
`[image.native_cyclonedds]` for the express purpose of differing from the
system default. Failing on that would be the gate inventing a rule the design
does not have. They are shown next to the pair so a reader can see the whole
ladder at once.

Run:  python3 scripts/check-workspace-rmw-agreement.py [--self-test] [--audit]
"""

import os
import re
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from tracked import tracked  # issue 0721: index lookup, not a walk

try:
    import tomllib
except ModuleNotFoundError:  # 3.10 backport, as the sibling gates spell it
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[1]

# `cmake_parse_arguments(_NRW …)` at NanoRosWorkspace.cmake:211. Mirrored here
# because the gate has to tokenize the call the same way cmake does; a keyword
# this list is missing would be read as a positional VALUE and shift everything
# after it.
OPTIONS = {"ORDER_FROM_DEPENDS"}
ONE_VALUE = {"SYSTEM", "BACKEND", "PLATFORM", "EDITION",
             "NANO_ROS_ROOT", "WORKSPACE_ROOT"}
MULTI_VALUE = {"SUBDIRS"}
KEYWORDS = OPTIONS | ONE_VALUE | MULTI_VALUE

# Statement position only. The negative lookbehind is what keeps
# `nano_ros_workspace_pkg_guard(` and `nano_ros_workspace_metadata(` out (their
# next character is `_`, not `(` or whitespace), and requiring the `(` keeps the
# DEFINITION `function(nano_ros_workspace)` out.
CALL_RE = re.compile(r"(?<![A-Za-z0-9_.])nano_ros_workspace\s*\(")
# A cmake argument: a quoted string (kept whole), or a run of non-space,
# non-paren characters.
TOKEN_RE = re.compile(r'"(?:[^"\\]|\\.)*"|[^\s()]+')
# Any unexpanded reference — `${x}`, `$ENV{x}`, `$CACHE{x}`, `$<…>`.
DYNAMIC_RE = re.compile(r"\$[A-Za-z]*[{<]")

FATAL = {"disagree", "missing-toml"}


# ---------------------------------------------------------------------------
# Parsing
# ---------------------------------------------------------------------------
def mask(text, strings=True):
    """Same-length text with comments — and optionally string interiors — blanked.

    Two things have to be invisible to the call scanner and both were learned
    the hard way by the selftest: a commented-out example call, and a call
    quoted inside a `message(WARNING …)`. `NanoRosWorkspace.cmake:265` tells the
    user to write `nano_ros_workspace(BACKEND \\${NROS_RMW} …)` inside a string
    literal, so a scanner that only strips comments reads the module's own
    diagnostic as a call site.

    Offsets are preserved (spaces, not deletions) so line numbers stay right and
    the caller can slice the ORIGINAL text for the argument list, where quoting
    still matters.
    """
    out, in_str, i, n = [], False, 0, len(text)
    while i < n:
        c = text[i]
        if in_str:
            if c == "\\" and i + 1 < n:
                out.append("  " if strings else text[i:i + 2])
                i += 2
                continue
            if strings:
                out.append('"' if c == '"' else (c if c == "\n" else " "))
            else:
                out.append(c)
            if c == '"':
                in_str = False
        elif c == '"':
            in_str = True
            out.append(c)
        elif c == "#":
            while i < n and text[i] != "\n":
                out.append(" ")
                i += 1
            continue
        else:
            out.append(c)
        i += 1
    return "".join(out)


def find_calls(text):
    """Every `nano_ros_workspace(...)` in `text`, as (line_no, argument text)."""
    masked = mask(text)            # comments + strings gone: what we SCAN
    clean = mask(text, strings=False)  # comments gone, quoting kept: what we SLICE
    calls = []
    for m in CALL_RE.finditer(masked):
        start = m.end()  # just past the opening paren
        depth, i, n = 1, start, len(masked)
        while i < n:
            c = masked[i]
            if c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        if depth:
            continue  # unbalanced — a broken file, not this gate's business
        # Sliced from the comment-free text: the tokenizer needs the quotes
        # back, but must not see `# SYSTEM foo` in a comment as a keyword.
        calls.append((masked.count("\n", 0, m.start()) + 1, clean[start:i]))
    return calls


def parse_args(arg_text):
    """`cmake_parse_arguments` semantics for the keyword set above."""
    args, cur = {}, None
    for raw in TOKEN_RE.findall(arg_text):
        tok = raw[1:-1] if len(raw) > 1 and raw[0] == '"' else raw
        if raw in KEYWORDS:  # a keyword is never quoted when it is a keyword
            if raw in OPTIONS:
                args[raw] = True
                cur = None
            else:
                args.setdefault(raw, [])
                cur = raw
            continue
        if cur in ONE_VALUE:
            args[cur].append(tok)
            cur = None
        elif cur in MULTI_VALUE:
            args[cur].append(tok)
    return {k: (v if isinstance(v, bool) else (v[0] if len(v) == 1 else v))
            for k, v in args.items()}


# ---------------------------------------------------------------------------
# The toml side
# ---------------------------------------------------------------------------
def system_rmws(path):
    """`([system].rmw, [image_defaults].rmw, {image_id: rmw})` from a system.toml."""
    with open(path, "rb") as fh:
        doc = tomllib.load(fh)
    sys_rmw = (doc.get("system") or {}).get("rmw") or None
    defaults = (doc.get("image_defaults") or {}).get("rmw") or None
    images = {}
    for iid, blk in (doc.get("image") or {}).items():
        if isinstance(blk, dict) and blk.get("rmw"):
            images[iid] = blk["rmw"]
    return sys_rmw, defaults, images


def bringups(ws_root):
    """Tracked `src/<pkg>/system.toml` under a workspace root, as {pkg: path}."""
    out = {}
    for p in tracked(ws_root, name="system.toml"):
        rel = Path(p).relative_to(ws_root).parts
        if len(rel) == 3 and rel[0] == "src":
            out[rel[1]] = Path(p)
    return out


# ---------------------------------------------------------------------------
# Analysis
# ---------------------------------------------------------------------------
def literal_default(text, var):
    """A single unambiguous `set(<var> <literal>)` in the same list file, or None.

    `examples/templates/multi-node-workspace-cpp` ships the shape
    `NanoRosWorkspace.cmake:265` tells users to write — a `-DNROS_RMW=` forward
    guarded by `if(NOT DEFINED NROS_RMW)`. That is CORRECT and must never fail
    this gate; the value is genuinely a configure-time choice. But the fallback
    IS authored, and it is the value nearly every configure gets, so naming it
    turns an "unresolvable, nothing to see" line into one a reader can act on.
    Only when there is exactly one candidate — two `set()`s mean a branch this
    gate has no business evaluating.
    """
    lits = {m.group(1) for m in re.finditer(
        r"set\s*\(\s*" + re.escape(var) + r"\s+([A-Za-z0-9_.+-]+)\s*\)",
        mask(text))}
    return lits.pop() if len(lits) == 1 else None


def analyse(cmake_path, line_no, args, repo_root, text=None):
    """One call site -> one finding dict.

    The BACKEND side is resolved LAST on purpose. Checking it first made a
    dynamic `BACKEND ${NROS_RMW}` short-circuit the whole finding, which threw
    away the fact that the very same workspace ships a bringup declaring an rmw
    — the most interesting thing about that call site.
    """
    d = Path(cmake_path).parent
    f = {"file": cmake_path, "line": line_no, "backend": args.get("BACKEND"),
         "system": args.get("SYSTEM"), "toml": None, "sys_rmw": None,
         "image_defaults": None, "images": {}, "detail": ""}
    backend = f["backend"]
    dyn = backend is not None and bool(DYNAMIC_RE.search(backend))
    if dyn:
        var = re.sub(r"^\$[A-Za-z]*\{|\}$", "", backend)
        if text is None:
            text = Path(cmake_path).read_text(encoding="utf-8", errors="replace")
        f["backend_default"] = literal_default(text, var)

    def unresolvable(why):
        f["kind"] = "unresolvable"
        f["detail"] = why
        return f

    ws_arg = args.get("WORKSPACE_ROOT")
    if ws_arg and DYNAMIC_RE.search(ws_arg):
        return unresolvable(f"WORKSPACE_ROOT is `{ws_arg}`")
    # cmake defaults WORKSPACE_ROOT to CMAKE_SOURCE_DIR and resolves a relative
    # value against the calling list file (NanoRosWorkspace.cmake:234-244). A
    # hand-written root IS the top-level source dir, so its own directory is
    # both — which is why the default below is `d` and not a repo-wide search.
    ws_root = (d / ws_arg).resolve() if ws_arg else d

    system = f["system"]
    if system is None:
        f["kind"] = "no-system"
        rmws = {}
        for pkg, p in sorted(bringups(ws_root).items()):
            s, _, _ = system_rmws(p)
            if s:
                rmws[pkg] = s
        f["images"] = rmws
        eff = backend if backend else "zenoh (BACKEND default)"
        if dyn and f.get("backend_default"):
            eff += f" (falls back to {f['backend_default']})"
        detail = f"no SYSTEM, so no system.toml is read; BACKEND={eff}"
        if rmws:
            ships = ", ".join(f"{k} rmw={v}" for k, v in rmws.items())
            detail += f"; the workspace nevertheless ships {ships}"
        f["detail"] = detail
        return f
    if DYNAMIC_RE.search(system):
        return unresolvable(f"SYSTEM is `{system}`")

    toml = ws_root / "src" / system / "system.toml"
    f["toml"] = str(toml.relative_to(repo_root)) if _under(toml, repo_root) else str(toml)
    if not toml.is_file():
        f["kind"] = "missing-toml"
        f["detail"] = f"SYSTEM {system} names no bringup — {f['toml']} does not exist"
        return f

    f["sys_rmw"], f["image_defaults"], f["images"] = system_rmws(toml)

    if dyn:
        # NOT a disagreement even when the fallback differs: the point of the
        # forwarding shape is that the value is chosen at configure time.
        extra = f", whose in-file fallback is {f['backend_default']}" \
            if f.get("backend_default") else ""
        return unresolvable(
            f"BACKEND is `{backend}`{extra} — not decidable without configuring; "
            f"[system].rmw = {f['sys_rmw']}")

    if backend is None and f["sys_rmw"] is None:
        f["kind"] = "agree"
        f["detail"] = "neither side authored; both default to zenoh"
    elif backend is None or f["sys_rmw"] is None:
        stated, absent = (("BACKEND", "[system].rmw") if backend
                          else ("[system].rmw", "BACKEND"))
        f["kind"] = "silent-drift"
        f["detail"] = (f"{stated}={backend or f['sys_rmw']} is authored, "
                       f"{absent} is not — nothing links them, so editing "
                       f"either later moves one side alone")
    elif backend == f["sys_rmw"]:
        f["kind"] = "agree"
        f["detail"] = f"both say {backend}"
    else:
        f["kind"] = "disagree"
        f["detail"] = (f"BACKEND={backend} (cmake links this) vs "
                       f"[system].rmw={f['sys_rmw']} (the CLI resolves this)")
    return f


def _under(p, root):
    try:
        Path(p).relative_to(root)
        return True
    except ValueError:
        return False


def call_sites(repo_root=None):
    """Every tracked CMakeLists.txt that CALLS `nano_ros_workspace`."""
    repo_root = Path(repo_root) if repo_root else ROOT
    out = []
    for p in tracked(repo_root, name="CMakeLists.txt", repo=repo_root):
        text = Path(p).read_text(encoding="utf-8", errors="replace")
        if "nano_ros_workspace" not in text:
            continue
        for line_no, arg_text in find_calls(text):
            out.append((p, line_no, parse_args(arg_text), text))
    return out


def findings(repo_root=None):
    repo_root = Path(repo_root) if repo_root else ROOT
    return [analyse(p, ln, args, repo_root, text)
            for p, ln, args, text in call_sites(repo_root)]


def render(f, repo_root):
    rel = str(Path(f["file"]).relative_to(repo_root)) if _under(f["file"], repo_root) \
        else str(f["file"])
    head = f"  {f['kind']:<13} {rel}:{f['line']}"
    lines = [head, f"                {f['detail']}"]
    if f["toml"]:
        lines.append(f"                bringup: {f['toml']}")
    if f["image_defaults"]:
        lines.append(f"                [image_defaults].rmw = {f['image_defaults']}"
                     " (per-target axis; not compared)")
    if f["images"] and f["kind"] != "no-system":
        shown = ", ".join(f"{k}={v}" for k, v in sorted(f["images"].items()))
        lines.append(f"                [image.*].rmw: {shown}"
                     " (per-target axis; not compared)")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Selftest — runs on the NORMAL path, not only behind the flag.
# ---------------------------------------------------------------------------
def _write(base, rel, text):
    p = Path(base) / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text, encoding="utf-8")
    return p


def _one(tmp, name, cmake, toml=None, toml_rel=None):
    """Build a one-workspace tree and analyse its single call site."""
    root = Path(tmp) / name
    cm = _write(root, "CMakeLists.txt", cmake)
    if toml is not None:
        _write(root, toml_rel or "src/demo_bringup/system.toml", toml)
    calls = find_calls(cm.read_text())
    assert len(calls) == 1, f"{name}: expected 1 call, parsed {len(calls)}"
    ln, arg_text = calls[0]
    return analyse(str(cm), ln, parse_args(arg_text), Path(tmp))


def self_test():
    """The negative controls. A gate that has never been red is a comment."""
    # 1. Discovery on the REAL tree. If the call regex stops matching, every
    #    arm below still passes on its temp files while the gate reports a
    #    cheerful zero over the repository — the gen-config-surface lesson.
    sites = call_sites()
    assert sites, ("no nano_ros_workspace() call site found in the tree — the "
                   "discovery regex is broken, not the repository")

    # 2. The definition and the two sibling functions must NOT be call sites.
    src = (ROOT / "cmake" / "NanoRosWorkspace.cmake").read_text()
    assert "function(nano_ros_workspace)" in src, "signature moved; re-check the parser"
    assert not find_calls(src), (
        "the parser reads NanoRosWorkspace.cmake's own definition / "
        "`nano_ros_workspace_pkg_guard` as a call site")

    # 3. The keyword list must still be the one cmake parses. A keyword we do
    #    not know is read as a positional value and shifts every argument
    #    after it, which is a WRONG answer rather than a missing one.
    m = re.search(r"cmake_parse_arguments\(_NRW\s*\n\s*\"([^\"]*)\"\s*\n"
                  r"\s*\"([^\"]*)\"\s*\n\s*\"([^\"]*)\"", src)
    assert m, "cannot read nano_ros_workspace's cmake_parse_arguments block"
    for got, want, label in ((m.group(1), OPTIONS, "options"),
                             (m.group(2), ONE_VALUE, "one-value"),
                             (m.group(3), MULTI_VALUE, "multi-value")):
        have = {t for t in got.split(";") if t}
        assert have == want, (
            f"nano_ros_workspace's {label} keywords are {sorted(have)}, this "
            f"gate mirrors {sorted(want)} — update KEYWORDS")

    with tempfile.TemporaryDirectory() as tmp:
        agree = ("nano_ros_workspace(\n    BACKEND  zenoh\n    PLATFORM posix\n"
                 "    SYSTEM   demo_bringup\n    SUBDIRS  src/a src/b)\n")
        toml = '[system]\nname = "d"\nrmw = "{}"\ndomain_id = 0\n'

        f = _one(tmp, "ok", agree, toml.format("zenoh"))
        assert f["kind"] == "agree", f

        # THE defect this gate exists for.
        f = _one(tmp, "bad", agree, toml.format("cyclonedds"))
        assert f["kind"] == "disagree", f
        assert "zenoh" in f["detail"] and "cyclonedds" in f["detail"], (
            "a disagreement must name BOTH values; got " + f["detail"])
        assert f["kind"] in FATAL, "disagree must be fatal"

        # One side only.
        f = _one(tmp, "drift", agree, '[system]\nname = "d"\ndomain_id = 0\n')
        assert f["kind"] == "silent-drift", f
        f = _one(tmp, "drift2",
                 agree.replace("BACKEND  zenoh\n    ", ""), toml.format("xrce"))
        assert f["kind"] == "silent-drift", f
        assert f["kind"] not in FATAL, "silent-drift must not be fatal"

        # A bringup that does not exist.
        f = _one(tmp, "gone", agree)
        assert f["kind"] == "missing-toml" and f["kind"] in FATAL, f

        # Unexpanded variables are not decidable here — and the finding must
        # still carry the toml side, which an early return used to discard.
        for spelling in ("${NROS_RMW}", "$ENV{NROS_RMW}", "$CACHE{NROS_RMW}"):
            f = _one(tmp, "dyn" + spelling[2:5],
                     agree.replace("BACKEND  zenoh", "BACKEND  " + spelling),
                     toml.format("xrce"))
            assert f["kind"] == "unresolvable", (spelling, f)
            assert "xrce" in f["detail"], (spelling, f)

        # …including the in-file fallback, which IS authored.
        f = _one(tmp, "dynfallback",
                 "if(NOT DEFINED NROS_RMW)\n    set(NROS_RMW cyclonedds)\nendif()\n"
                 + agree.replace("BACKEND  zenoh", "BACKEND  ${NROS_RMW}"),
                 toml.format("zenoh"))
        assert f["kind"] == "unresolvable" and f["backend_default"] == "cyclonedds", f
        assert "cyclonedds" in f["detail"], f
        # Two candidate `set()`s are a branch this gate must not evaluate.
        assert literal_default("set(X a)\nset(X b)\n", "X") is None

        # No SYSTEM: nothing is read, so nothing can disagree — but the
        # workspace's own bringup is still named, because that is where the
        # next disagreement comes from.
        f = _one(tmp, "nosys", agree.replace("    SYSTEM   demo_bringup\n", ""),
                 toml.format("cyclonedds"))
        assert f["kind"] == "no-system", f
        assert "demo_bringup" in f["detail"] and "cyclonedds" in f["detail"], f

        # Per-target images vary BY DESIGN and must never fail.
        f = _one(tmp, "imgs", agree,
                 toml.format("zenoh")
                 + '\n[image_defaults]\nrmw = "zenoh"\n'
                   '\n[image.native_cyclonedds]\nrmw = "cyclonedds"\n')
        assert f["kind"] == "agree", f
        assert f["images"] == {"native_cyclonedds": "cyclonedds"}, f
        assert f["image_defaults"] == "zenoh", f
        assert "cyclonedds" in render(f, Path(tmp)), "image rmw must be SHOWN"

        # Parser: comments, quoting, WORKSPACE_ROOT indirection, and a keyword
        # appearing as another keyword's VALUE.
        f = _one(tmp, "quoted",
                 '# nano_ros_workspace(BACKEND cyclonedds SYSTEM x)  <- a comment\n'
                 'nano_ros_workspace(\n    BACKEND  "cyclonedds"  # not zenoh\n'
                 '    WORKSPACE_ROOT "."\n    SYSTEM   "demo_bringup"\n'
                 '    SUBDIRS  src/a\n    ORDER_FROM_DEPENDS)\n',
                 toml.format("cyclonedds"))
        assert f["kind"] == "agree" and f["backend"] == "cyclonedds", f

        f = _one(tmp, "root_indirect",
                 agree.replace("PLATFORM posix", "WORKSPACE_ROOT ws"),
                 toml.format("zenoh"), "ws/src/demo_bringup/system.toml")
        assert f["kind"] == "agree", f

        # `nano_ros_workspace_pkg_guard(` must not be read as a call.
        assert not find_calls("nano_ros_workspace_pkg_guard(NAME foo)\n")
        assert not find_calls("my_nano_ros_workspace(BACKEND zenoh)\n")

    print("workspace-rmw-agreement self-test: OK")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    # On the NORMAL path too, not only when asked. A gate whose own negative
    # control runs behind a flag is a gate that was red once, on the day it was
    # written, and is prose afterwards (phase-395, check-gate-selftests).
    self_test()

    fs = findings()
    order = ["disagree", "missing-toml", "silent-drift", "no-system",
             "unresolvable", "agree"]
    fs.sort(key=lambda f: (order.index(f["kind"]), f["file"], f["line"]))
    bad = [f for f in fs if f["kind"] in FATAL]

    audit = "--audit" in sys.argv
    for f in fs:
        if audit or f["kind"] != "agree":
            print(render(f, ROOT))

    counts = {}
    for f in fs:
        counts[f["kind"]] = counts.get(f["kind"], 0) + 1
    summary = ", ".join(f"{counts[k]} {k}" for k in order if k in counts)

    if bad:
        print(
            f"\nerror: {len(bad)} nano_ros_workspace() call site(s) declare the "
            f"RMW twice and disagree.\n"
            "  `BACKEND` is what the C/C++ lane LINKS; `[system].rmw` is what\n"
            "  `resolved_rmw` gives the CLI. Nothing reconciles them (issue 0934\n"
            "  R1), so the two halves of one build use different backends.\n"
            "  Fix: make the two values equal. Which one is the SSoT is\n"
            "  phase-405 W2's follow-up; until then they must not contradict.",
            file=sys.stderr)
        return 1

    print(f"workspace-rmw-agreement OK — {len(fs)} call site(s): {summary}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
