#!/usr/bin/env python3
"""Does a configured build dir still REFER to files that exist? — issue 1406.

# The blind spot this closes

`nros-reconfigure-stale.sh` probed each `build.ninja` with a load-only
`ninja -t targets` and reported "OK (352 build dir(s) load)". A manifest naming
a source deleted seventeen days earlier LOADS PERFECTLY WELL — ninja does not
stat an edge's inputs until it runs the edge — so 38 such manifests sat in the
OK column, and one of them killed the native fixture build with a `cc1plus:
fatal error: … No such file or directory` four hundred lines into a log,
attributed to whichever fixture reached it first.

That is the second time the same blind spot has been measured. The first was a
CMakeCache holding a `CMAKE_MAKE_PROGRAM` that had been deleted: also invisible,
for the same reason — the probe asked ninja, and ninja never looks. Both facts
are "this directory references something that is gone", so this module answers
both, and a caller that only knew about one of them would leave the other
exactly where it was.

# What is checked, and why the shape of the rule matters

*Manifest inputs.* Every explicit and implicit input of every `build` edge,
minus everything the manifest declares as an OUTPUT (a generated file that has
not been built yet is not stale, it is pending). Paths are canonicalised against
the build dir before comparing, because cmake writes an output relative and the
same file absolute one line later; comparing the spellings reported 363 paths
where comparing the files reports 147.

*Cache tool paths.* Absolute `:FILEPATH=` entries in `CMakeCache.txt` whose
value is NOT under the build dir. The exclusion is structural rather than a list
of variable names: what lives under the build dir is this build's own byproducts
(`BYPRODUCT_KERNEL_BIN_NAME` points at a `zephyr.bin` that has simply not been
linked yet), and what lives outside it is a tool or input cmake RESOLVED once and
will re-invoke without re-checking. A name list would have been shorter and would
have gone stale the first time a toolchain file cached a new variable.

Order-only inputs (`||`) are deliberately NOT checked. They are overwhelmingly
phony targets and stamps, and the ones that are files are also reachable as real
inputs somewhere; including them bought nothing and would make the probe's
verdict depend on build ORDER state rather than on references.

A token still carrying a `$` after unescaping is an unexpanded ninja variable
(`${cmake_ninja_workdir}foo`). It is skipped rather than guessed at — this probe
must not invent a path that no build ever names.

Usage::

    python3 scripts/lib/ninja_stale_refs.py <build-dir> ...   # TSV findings
    python3 scripts/lib/ninja_stale_refs.py --selftest        # explicit run

The selftest also runs on the normal path, because a probe that reports nothing
and a probe that cannot report look identical from the outside — which is the
whole reason this file exists.
"""

import os
import sys

# The manifest files a `build.ninja` may pull in. `include` shares the scope,
# `subninja` does not, but for "what does this build refer to?" the difference
# does not matter: both contribute edges.
INCLUDE_KEYWORDS = ("include ", "subninja ")


def _unescape_split(text):
    """Split a manifest fragment into tokens, honouring ninja's escapes.

    `$ ` is a literal space, `$:` a literal colon, `$$` a literal dollar. The
    common case has no `$` at all, and the fast path matters: this runs over
    155 MB of manifests on a full tree.
    """
    if "$" not in text:
        return text.split()
    toks, cur, i, n = [], [], 0, len(text)
    while i < n:
        c = text[i]
        if c == "$" and i + 1 < n and text[i + 1] in (" ", ":", "$"):
            cur.append(text[i + 1])
            i += 2
            continue
        if c == " ":
            if cur:
                toks.append("".join(cur))
                cur = []
            i += 1
            continue
        cur.append(c)
        i += 1
    if cur:
        toks.append("".join(cur))
    return toks


def _separator_colon(line):
    """Index of the `:` that ends a build statement's output list, or -1.

    A colon preceded by an ODD number of `$` is escaped (`$:`); `$$:` is a
    literal dollar followed by a real separator.
    """
    start = 0
    while True:
        idx = line.find(":", start)
        if idx < 0:
            return -1
        dollars = 0
        j = idx - 1
        while j >= 0 and line[j] == "$":
            dollars += 1
            j -= 1
        if dollars % 2 == 0:
            return idx
        start = idx + 1


def parse_build_statement(line):
    """`build OUT…[| IMPLICIT_OUT]: RULE IN…[| IMPLICIT][|| ORDER]` -> (outs, ins).

    Implicit inputs join the explicit ones — both must exist before the edge can
    run. Order-only inputs are dropped; see the module docstring.
    """
    colon = _separator_colon(line)
    if colon < 0:
        return [], []
    outs = _unescape_split(line[6:colon])
    rest = _unescape_split(line[colon + 1:])
    if not rest:
        return outs, []
    rest = rest[1:]  # the rule name
    ins = []
    for tok in rest:
        if tok == "||":
            break
        if tok == "|":
            continue
        ins.append(tok)
    return outs, ins


def _iter_statements(path):
    """Yield each `build` statement of a manifest, continuations joined."""
    try:
        fh = open(path, "r", errors="replace")
    except OSError:
        return
    with fh:
        pending = None
        for raw in fh:
            line = raw.rstrip("\n")
            if pending is not None:
                pending = pending + " " + line.strip()
                if line.endswith("$"):
                    pending = pending[:-1]
                    continue
                yield pending
                pending = None
                continue
            if line.startswith("include ") or line.startswith("subninja "):
                yield line
                continue
            if not line.startswith("build "):
                continue
            if line.endswith("$"):
                pending = line[:-1]
                continue
            yield line
        if pending is not None:
            yield pending


def _canon(build_dir, token):
    return os.path.normpath(os.path.join(build_dir, token))


def scan_manifest(build_dir, manifest, _seen=None):
    """-> (set of canonical outputs, list of canonical inputs).

    Follows `include` / `subninja`, because an output declared in an included
    file is still an output, and treating it as a missing input would report the
    generated half of every build as stale.
    """
    seen = _seen if _seen is not None else set()
    real = os.path.realpath(manifest)
    if real in seen:
        return set(), []
    seen.add(real)
    outputs, inputs = set(), []
    for stmt in _iter_statements(manifest):
        for kw in INCLUDE_KEYWORDS:
            if stmt.startswith(kw):
                arg = stmt[len(kw):].strip()
                if arg and "$" not in arg:
                    sub_out, sub_in = scan_manifest(
                        build_dir, _canon(build_dir, arg), seen
                    )
                    outputs |= sub_out
                    inputs.extend(sub_in)
                break
        else:
            outs, ins = parse_build_statement(stmt)
            for tok in outs:
                outputs.add(_canon(build_dir, tok))
            for tok in ins:
                if "$" in tok:
                    continue  # an unexpanded variable; never guessed at
                inputs.append(_canon(build_dir, tok))
    return outputs, inputs


def missing_inputs(build_dir):
    """Canonical paths this manifest consumes that are neither outputs nor files."""
    manifest = os.path.join(build_dir, "build.ninja")
    if not os.path.isfile(manifest):
        return []
    outputs, inputs = scan_manifest(build_dir, manifest)
    missing, seen = [], set()
    for path in inputs:
        if path in outputs or path in seen:
            continue
        seen.add(path)
        if not os.path.exists(path):
            missing.append(path)
    return missing


def missing_cache_paths(build_dir):
    """-> [(cache variable, path)] for tool paths the cache names and cmake lost.

    Only ABSOLUTE values outside the build dir: see the module docstring for why
    that exclusion is structural and not a list of variable names.
    """
    cache = os.path.join(build_dir, "CMakeCache.txt")
    if not os.path.isfile(cache):
        return []
    prefix = os.path.join(os.path.normpath(build_dir), "")
    found = []
    with open(cache, "r", errors="replace") as fh:
        for line in fh:
            if ":FILEPATH=" not in line or line.lstrip().startswith(("#", "//")):
                continue
            key, _, value = line.partition(":FILEPATH=")
            value = value.strip()
            if not value or not os.path.isabs(value):
                continue  # `FOO-NOTFOUND` and relative values say nothing
            value = os.path.normpath(value)
            if value.startswith(prefix):
                continue  # this build's own byproduct, possibly not built yet
            if not os.path.exists(value):
                found.append((key, value))
    return found


def stale_references(build_dir):
    """-> [(kind, path)] — everything this dir names that is not there."""
    out = [("input", p) for p in missing_inputs(build_dir)]
    out.extend((f"cache:{k}", p) for k, p in missing_cache_paths(build_dir))
    return out


def self_test():
    """Negative control, on the normal path.

    Every case here is one this probe's PREDECESSOR passed: the manifests load,
    `ninja -t targets` is happy, and the references are wrong anyway.
    """
    import tempfile

    def manifest(d, body):
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "build.ninja"), "w") as fh:
            fh.write("rule cc\n  command = cc $in -o $out\n\n" + body)

    with tempfile.TemporaryDirectory() as tmp:
        src = os.path.join(tmp, "src")
        os.makedirs(src)
        open(os.path.join(src, "present.c"), "w").close()

        # 1. THE case: a manifest that loads and names a file that is gone.
        b = os.path.join(tmp, "b1")
        manifest(b, "build out.o: cc ../src/gone.c\n")
        assert missing_inputs(b) == [os.path.join(src, "gone.c")], missing_inputs(b)

        # 2. A present source is not a finding — otherwise case 1 proves only
        #    that the probe reports everything.
        manifest(b, "build out.o: cc ../src/present.c\n")
        assert missing_inputs(b) == [], missing_inputs(b)

        # 3. A generated file the manifest produces is PENDING, not missing —
        #    and it stays that way when the two edges spell it differently,
        #    which is what cmake actually emits.
        manifest(
            b,
            "build gen/thing.c: cc ../src/present.c\n"
            f"build thing.o: cc {b}/gen/thing.c\n",
        )
        assert missing_inputs(b) == [], missing_inputs(b)

        # 4. Implicit inputs (after `|`) are inputs; order-only (`||`) are not.
        manifest(b, "build out.o: cc ../src/present.c | ../src/hdr.h || ../src/stamp\n")
        assert missing_inputs(b) == [os.path.join(src, "hdr.h")], missing_inputs(b)

        # 5. Escapes and continuations, since both change where a token ends.
        manifest(
            b,
            "build out$ 1.o: cc ../src/present.c $\n"
            "  ../src/second$ file.c\n",
        )
        assert missing_inputs(b) == [
            os.path.join(src, "second file.c")
        ], missing_inputs(b)

        # 6. An output declared in an `include`d file is still an output.
        with open(os.path.join(b, "extra.ninja"), "w") as fh:
            fh.write("build gen/other.c: cc ../src/present.c\n")
        manifest(b, "include extra.ninja\nbuild other.o: cc gen/other.c\n")
        assert missing_inputs(b) == [], missing_inputs(b)

        # 7. An unexpanded variable is skipped, never guessed at.
        manifest(b, "build out.o: cc ${cmake_ninja_workdir}nowhere.c\n")
        assert missing_inputs(b) == [], missing_inputs(b)

        # 8. The cache arm: a tool that is gone is a finding…
        cache = os.path.join(b, "CMakeCache.txt")
        gone = os.path.join(tmp, "sdk", "bin", "ninja")
        with open(cache, "w") as fh:
            fh.write(
                "//comment\n"
                f"CMAKE_MAKE_PROGRAM:FILEPATH={gone}\n"
                "CMAKE_C_COMPILER:FILEPATH=" + os.path.join(src, "present.c") + "\n"
                "CCACHE:FILEPATH=CCACHE-NOTFOUND\n"
                f"BYPRODUCT_KERNEL_BIN:FILEPATH={b}/zephyr/zephyr.bin\n"
            )
        assert missing_cache_paths(b) == [
            ("CMAKE_MAKE_PROGRAM", gone)
        ], missing_cache_paths(b)

        # …and a byproduct UNDER the build dir is not, however absent — which is
        # the difference between "not built yet" and "gone".
        assert not os.path.exists(f"{b}/zephyr/zephyr.bin")

        # 9. A dir with no manifest and no cache reports nothing rather than
        #    raising, because the caller scans whatever `find` turned up.
        empty = os.path.join(tmp, "empty")
        os.makedirs(empty)
        assert stale_references(empty) == []


def main(argv):
    self_test()
    args = [a for a in argv[1:] if a != "--selftest"]
    if "--selftest" in argv[1:] and not args:
        print("ninja_stale_refs: selftest OK")
        return 0
    findings = 0
    for d in args:
        # The dir is echoed back EXACTLY as given, so a shell caller can key an
        # array on it without having to reproduce this script's idea of
        # "absolute" (`os.path.abspath` and `cd … && pwd` disagree about a
        # symlinked component, and the disagreement is silent: every lookup
        # misses and the sweep reports nothing).
        for kind, path in stale_references(d):
            print(f"{d}\t{kind}\t{path}")
            findings += 1
    return 1 if findings else 0


if __name__ == "__main__":
    # 0 = clean, 1 = findings, 2 = the probe itself broke. A caller reads the
    # difference: an uncaught traceback exits 1 by default, which is exactly the
    # code that means "found something" — and since a crash also prints nothing
    # to stdout, the shell would have read a broken probe as a clean sweep.
    try:
        sys.exit(main(sys.argv))
    except SystemExit:
        raise
    except BaseException:
        import traceback

        traceback.print_exc()
        sys.exit(2)
