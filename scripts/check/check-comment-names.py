#!/usr/bin/env python3
"""check-comment-names — a code comment may not quote a symbol that exists nowhere.

phase-515 W0.d. W0.b and W0.c hold the API-parity ledger's prose to the tree:
a backtick-quoted name in a row's `why` must resolve to something that ships,
or be listed as history. Code comments had no such rule, and they carried the
same defect at ten times the volume — 2,365 quoted names under `packages/` and
`examples/` resolved nowhere when this was written, among them the
`nros_executor_add_subscription_*` family phase-487 renamed, still described
in the generated C header as the call to make.

The rule, for every comment in a tracked C, C++ or Rust source in SCOPE:

  A backtick-quoted name (`quoted_names` in `api-parity.py`: C-prefixed, a
  `::` path, or anything only an identifier could be) must resolve to an
  identifier in the tracked sources -- the same index the ledger check uses,
  built from code with its comments stripped, so a comment cannot vouch for
  itself.

  A name the comment means as HISTORY is struck through: ~~`old_name`~~. That
  is the inline marker. It renders as struck text in rustdoc, it reads as
  "gone" in plain source, and it states tense instead of leaving a reader (or
  this gate) to guess it from "was" and "until". A struck name that EXISTS is
  itself a finding: either the marker is wrong or the name came back.

  A name from another project (a zenoh-pico internal, a DDS spec constant, a
  FreeRTOS hook, a ROS package) is listed once in
  `scripts/check/comment-foreign-names.txt`. A listed name that resolves here,
  or that no comment in scope quotes any more, is a finding too, so the list
  cannot rot into a blanket exemption.

  The index also holds what a tracked shell script defines (a function, an
  assigned or exported variable) and the upper-case variables it reads
  (`${NROS_INSTALL_URL:-...}`), since `scripts/` is a library and an
  environment interface the comments cite.

  A Kconfig symbol is judged by its own rule: `CONFIG_NROS_<X>` must be a
  `config NROS_<X>` in a tracked Kconfig file, and any other `CONFIG_<X>` is
  the RTOS's (Zephyr, NuttX, ESP-IDF, PX4), as `k_` and `zsock_` already are.

  A placeholder (`Node::create_X`, `foo_pkg`, `param_NN_`) is not a symbol and
  is not judged.

SCOPE is `packages/` and `examples/` whole. It was widened one area per pull
request while the comments were swept (phase-515 W0.d); it was never a
baseline, and nothing inside it is exempt.

Buildless, tracked files only; `--self-test` runs both directions of every
rule and runs first on every check.
"""
import argparse
import importlib.util
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))

_spec = importlib.util.spec_from_file_location("api_parity", os.path.join(ROOT, "scripts", "api-parity.py"))
ap = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ap)

SCOPE = (
    "examples",
    "packages",
)
FOREIGN_LIST = os.path.join(HERE, "comment-foreign-names.txt")
# The ledger check's roots plus the two trees a comment cites that the ledger
# never does: the Zephyr module's cmake and the RTOS integrations.
KNOWN_ROOTS = ("packages", "examples", "scripts", "just", "cmake", "zephyr", "integrations",
               "docs/reference/api-surface", "docs/reference/editions")

# Comments only: a literal is matched first so a `"/**"` or `"//"` inside one
# does not open a comment (the 840-line swallow `api-parity.py` records).
COMMENT_RE = ap.CODE_COMMENT_RE
STRUCK_RE = re.compile(r"~~(.+?)~~")
# `[~~`gone`~~]` is an intra-doc link to a name that is gone: rustdoc reports it
# as unresolved, so the marker goes on plain text, never inside a link.
STRUCK_LINK_RE = re.compile(r"\[~~`")
PLACEHOLDER_RE = re.compile(r"(?:^|_|::)(?:X|N|NN|XX|XXX|xxx|foo|bar|baz)(?:_|$|::)|^(?:my_|My[A-Z])")


# What a shell script DEFINES -- a function, an assigned or exported variable --
# not every word in it: a gate's table of retired spellings is mostly strings,
# and counting those would let a struck name read as live.
SHELL_DEF_RE = re.compile(
    r"^[ \t]*(?:function[ \t]+([A-Za-z_][A-Za-z0-9_]*)|([A-Za-z_][A-Za-z0-9_]*)[ \t]*\(\)"
    r"|(?:export[ \t]+|local[ \t]+|readonly[ \t]+)?([A-Za-z_][A-Za-z0-9_]*)=)", re.M)


SHELL_READ_RE = re.compile(r"\$\{?([A-Z][A-Z0-9_]*)")


def shell_definitions():
    files = subprocess.run(["git", "-C", ROOT, "ls-files", "--", "*.sh", "*.bash", "activate*"],
                           capture_output=True, text=True, check=True).stdout.split()
    out = set()
    for rel in files:
        with open(os.path.join(ROOT, rel), errors="ignore") as fh:
            text = fh.read()
            for m in SHELL_DEF_RE.finditer(text):
                out.add(m.group(1) or m.group(2) or m.group(3))
            out.update(SHELL_READ_RE.findall(text))
    return out


KCONFIG_SYMBOL_RE = re.compile(r"^\s*(?:menu)?config\s+([A-Z0-9_]+)", re.M)


def kconfig_symbols():
    files = subprocess.run(["git", "-C", ROOT, "ls-files", "--", "*Kconfig*"],
                           capture_output=True, text=True, check=True).stdout.split()
    out = set()
    for rel in files:
        with open(os.path.join(ROOT, rel), errors="ignore") as fh:
            out.update(KCONFIG_SYMBOL_RE.findall(fh.read()))
    return out


def kconfig_verdict(name, tail, symbols):
    """None when `name` is not a Kconfig symbol; else whether it resolves."""
    if not name.startswith("CONFIG_"):
        return None
    sym = name[len("CONFIG_"):]
    if not sym.startswith("NROS"):
        return True
    if tail in ("*", "{") or sym.endswith("_"):
        return any(s.startswith(sym) for s in symbols)
    return sym in symbols


def load_foreign(path=FOREIGN_LIST):
    names = {}
    with open(path) as fh:
        for n, line in enumerate(fh, 1):
            body = line.split("#", 1)[0].strip()
            if body:
                names[body] = n
    return names


def comment_names(text):
    """`(line, name, tail, struck)` for every judged name in `text`'s comments."""
    out = []
    for m in COMMENT_RE.finditer(text):
        if m.group(1):
            continue
        comment, base = m.group(0), m.start()
        for link in STRUCK_LINK_RE.finditer(comment):
            out.append((text.count("\n", 0, base + link.start()) + 1, "[~~", "", "link"))
        struck = [(s.start(1), s.end(1)) for s in STRUCK_RE.finditer(comment)]
        for q in ap.quoted_name_matches(comment):
            pos = q.start()
            out.append((text.count("\n", 0, base + pos) + 1, q.group(1), q.group(2),
                        any(a <= pos < b for a, b in struck)))
    return out


def exists_exactly(index, name):
    """A struck name is history only if no segment of it ships. Exact tokens,
    not the prefix rules `resolves` applies: `~~`nros_spin_`~~` must not count
    as live because `nros_spin_once_helper` happens to start with it."""
    segs = [s.lstrip("~") for s in name.split("::")]
    return all(s in index.known for s in segs if s)


def check_text(rel, text, index, foreign, seen_foreign, symbols=frozenset()):
    findings = []
    for line, name, tail, struck in comment_names(text):
        where = "%s:%d" % (rel, line)
        if struck == "link":
            findings.append("%s: a struck name inside an intra-doc link (`[~~`...`~~]`) "
                            "is a link to a gone item; drop the brackets" % where)
            continue
        if PLACEHOLDER_RE.search(name):
            continue
        if name in foreign:
            seen_foreign.add(name)
            continue
        if struck:
            if name.startswith("CONFIG_"):
                # A Kconfig symbol is live when a Kconfig declares it; a refusal
                # table naming the retired spelling does not make it live.
                live = (name[len("CONFIG_"):].startswith("NROS")
                        and kconfig_verdict(name, tail, symbols))
            else:
                live = exists_exactly(index, name)
            if live:
                findings.append("%s: ~~`%s`~~ is struck as history but EXISTS; unstrike it, "
                                "or name what replaced it" % (where, name))
            continue
        kconfig = kconfig_verdict(name, tail, symbols)
        if kconfig is not None:
            if not kconfig:
                findings.append("%s: `%s` is no Kconfig symbol in this tree; name the live "
                                "one, or strike it if the comment means its history"
                                % (where, name))
            continue
        if index.resolves(name, tail) or name in ap.UPSTREAM_NAMES_OUTSIDE_SURFACE:
            continue
        findings.append("%s: `%s` exists nowhere in the tree -- name the live symbol, strike "
                        "it (~~`%s`~~) as history, or list it in comment-foreign-names.txt "
                        "as another project's" % (where, name, name))
    return findings


def scope_files(scope):
    files = subprocess.run(["git", "-C", ROOT, "ls-files", "--", *scope],
                           capture_output=True, text=True, check=True).stdout.split()
    return [f for f in files if f.endswith(ap.CODE_COMMENT_EXTS) and "/generated/" not in f]


def check(scope=SCOPE):
    # The foreign list is a `.txt` under `scripts/`; `known_identifiers` leaves
    # it out by default, or every entry would "exist here".
    known = ap.known_identifiers([], roots=KNOWN_ROOTS) | shell_definitions()
    index = ap._NameIndex(known)
    foreign = load_foreign()
    symbols = kconfig_symbols()
    seen, findings = set(), []
    for rel in scope_files(scope):
        with open(os.path.join(ROOT, rel), errors="ignore") as fh:
            findings += check_text(rel, fh.read(), index, foreign, seen, symbols)
    for name, line in sorted(foreign.items(), key=lambda kv: kv[1]):
        where = "scripts/check/comment-foreign-names.txt:%d" % line
        if exists_exactly(index, name):
            findings.append("%s: `%s` is listed as another project's but EXISTS here; drop it"
                            % (where, name))
        elif name not in seen:
            findings.append("%s: `%s` is quoted by no comment in scope any more; drop it"
                            % (where, name))
    return findings


def self_test():
    failures = []
    index = ap._NameIndex({"rclc_executor_spin", "Node", "create_publisher", "live_fn"})

    def run(src, foreign=()):
        return check_text("t.rs", src, index, set(foreign), set(), {"NROS_MAX_TIERS"})

    def expect(label, src, want, foreign=()):
        got = len(run(src, foreign))
        if got != want:
            failures.append("%s: %d finding(s), want %d: %r" % (label, got, want, run(src, foreign)))

    expect("live name", "// calls `rclc_executor_spin` then `Node::create_publisher`\n", 0)
    expect("dead name", "// calls `nros_executor_spin_some`\n", 1)
    expect("dead name in block", "/* see\n * `nros_executor_spin_some`\n */\n", 1)
    expect("dead path", "/// see `Node::create_gone`\n", 1)
    expect("struck dead", "// renamed from ~~`nros_executor_spin_some`~~\n", 0)
    expect("struck live", "// renamed from ~~`rclc_executor_spin`~~\n", 1)
    expect("struck span, two names", "// ~~`gone_a` and `gone_b`~~ went\n", 0)
    expect("struck one, other live", "// ~~`gone_a`~~ became `live_fn`\n", 0)
    expect("struck one, other dead", "// ~~`gone_a`~~ became `gone_b`\n", 1)
    expect("foreign", "// zenoh-pico's `_z_open`\n", 0, foreign=("_z_open",))
    expect("our Kconfig symbol", "// `CONFIG_NROS_MAX_TIERS`\n", 0)
    expect("gone Kconfig symbol", "// `CONFIG_NROS_GONE`\n", 1)
    expect("RTOS Kconfig symbol", "// `CONFIG_HEAP_MEM_POOL_SIZE`\n", 0)
    expect("struck retired Kconfig symbol", "// ~~`CONFIG_NROS_GONE`~~\n", 0)
    expect("struck live Kconfig symbol", "// ~~`CONFIG_NROS_MAX_TIERS`~~\n", 1)
    expect("struck inside an intra-doc link", "/// see [~~`gone_fn`~~]\n", 1)
    expect("placeholder", "// each `Node::create_X<T>` and `foo_pkg`\n", 0)
    expect("string literal is not a comment", 'let s = "// `gone_fn`";\n', 0)
    expect("literal does not open a comment", 'let g = "/**"; let h = `gone_fn`;\n', 0)
    lines = [ln for ln, *_ in comment_names("fn a() {}\n\n/* x\n   `gone_fn` */\n")]
    if lines != [4]:
        failures.append("line numbers: got %r want [4]" % lines)
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    failures = self_test()
    if failures:
        print("check-comment-names: self-test FAILED")
        for f in failures:
            print("  " + f)
        return 1
    if args.self_test:
        print("check-comment-names: self-test OK")
        return 0
    findings = check()
    if findings:
        print("check-comment-names: FAILED — %d finding(s)" % len(findings))
        for f in findings:
            print("  " + f)
        return 1
    print("check-comment-names: OK — comments in %s quote only live, struck or foreign names"
          % ", ".join(SCOPE))
    return 0


if __name__ == "__main__":
    sys.exit(main())
