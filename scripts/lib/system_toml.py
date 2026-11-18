"""The one Python reader of a bringup's `system.toml` (RFC-0024 §5, issue 2587).

A `system.toml` may name shared fragments with `include = ["<path>", ...]`.
`load(path)` returns the file with every include expanded, which is what a
reader of a system's MEANING must see. The rules are the Rust reader's
(`packages/tooling/nros-system-include`), and both run the shared cases in
`packages/tooling/nros-system-include/cases/`, so they cannot drift:

1. `include` is a top-level array of paths, each relative to the file that
   names it; a fragment may include further fragments.
2. Tables merge key by key, at every depth.
3. A key that is not a table, stated by two files, is refused, naming both.
4. An array of tables concatenates: the includes' entries in order, then the
   including file's own.
5. A cycle, or a file reached twice, is refused.
6. A fragment may not set `[system] name`, `[sources]` or `[tools]`.

A script that only LINTS a file's own text (a spelling, a retired key) reads
the file itself; that is a different question from what the system means.

Run as a program, it checks itself against the shared cases.
"""

from __future__ import annotations

import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib  # type: ignore[no-redef]

INCLUDE_KEY = "include"
OWN_ONLY = ("sources", "tools")
CASES = Path(__file__).resolve().parents[2] / "packages/tooling/nros-system-include/cases"


class SystemTomlError(ValueError):
    """A system.toml or one of its includes cannot be expanded."""


def has_include(path: Path) -> bool:
    """Does the file at `path` itself name an include?"""
    try:
        with open(path, "rb") as fh:
            return INCLUDE_KEY in tomllib.load(fh)
    except (OSError, tomllib.TOMLDecodeError):
        return False


def load_with_files(path: Path) -> tuple[dict, list[Path]]:
    """`path` expanded, and every file read (the including file first)."""
    files: list[Path] = []
    origin: dict[str, Path] = {}
    table = _expand(Path(path), [], files, origin, True)
    return table, files


def load(path: Path) -> dict:
    """`path` expanded. Raises SystemTomlError naming the file at fault."""
    return load_with_files(path)[0]


def text(path: Path) -> str:
    """`path` expanded, as TOML text: for a reader that scans text (a regex
    over `board = "..."`, a line walk over `[image.<id>]` headers), so it sees
    every fragment's keys, not only the file's own. Tables are written as
    `[a.b]` headers with `key = value` lines, arrays of tables as `[[a]]`."""
    return dumps(load(path))


def _value(v) -> str:
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (int, float)):
        return repr(v)
    if isinstance(v, str):
        return '"' + v.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'
    if isinstance(v, list):
        return "[" + ", ".join(_value(x) for x in v) + "]"
    if isinstance(v, dict):
        return "{ " + ", ".join(f"{_key(k)} = {_value(x)}" for k, x in v.items()) + " }"
    return '"' + str(v) + '"'  # a date/time: written as its text


def _key(k: str) -> str:
    return k if k and all(c.isalnum() or c in "_-" for c in k) else _value(k)


def dumps(table: dict, prefix: str = "") -> str:
    """TOML text for a parsed table (the subset a system.toml uses)."""
    lines, subs = [], []
    for k, v in table.items():
        if isinstance(v, dict):
            subs.append((k, v, False))
        elif _is_table_array(v):
            subs.append((k, v, True))
        else:
            lines.append(f"{_key(k)} = {_value(v)}")
    out = "\n".join(lines) + ("\n" if lines else "")
    for k, v, many in subs:
        name = f"{prefix}.{_key(k)}" if prefix else _key(k)
        if many:
            for item in v:
                out += f"\n[[{name}]]\n" + dumps(item, name)
        else:
            out += f"\n[{name}]\n" + dumps(v, name)
    return out


def _expand(path: Path, stack: list[Path], files: list[Path], origin: dict, is_root: bool) -> dict:
    try:
        canon = path.resolve(strict=True)
    except OSError as e:
        raise SystemTomlError(f"{path}: {e}") from None
    if canon in stack:
        chain = " -> ".join(str(p) for p in stack[stack.index(canon):] + [canon])
        raise SystemTomlError(
            f"{path}: `include` reaches this file again through itself ({chain}); "
            "remove one of the includes in that chain"
        )
    if canon in files:
        raise SystemTomlError(
            f"{path}: this file is included twice (once directly or through another "
            "include); include it from one place only, or its `[[...]]` entries would "
            "be counted twice"
        )
    files.append(canon)
    try:
        with open(canon, "rb") as fh:
            own = tomllib.load(fh)
    except tomllib.TOMLDecodeError as e:
        raise SystemTomlError(f"{path}: not valid TOML: {e}") from None
    if not is_root:
        if "name" in (own.get("system") or {}):
            raise SystemTomlError(
                f"{path}: an included file may not set `[system] name`; the system is "
                "named by the bringup's own system.toml, so move `name` there"
            )
        for t in OWN_ONLY:
            if t in own:
                raise SystemTomlError(
                    f"{path}: an included file may not set `[{t}]`; it holds this "
                    "machine's paths for the project, not shared topology, and build "
                    "scripts read it from the bringup's own system.toml only. Move "
                    f"`[{t}]` into the bringup's own system.toml."
                )
    includes = own.pop(INCLUDE_KEY, [])
    if not isinstance(includes, list):
        raise SystemTomlError(
            f'{path}: `include` is a list of paths, e.g. `include = ["../shared.toml"]`, '
            f"not `{includes!r}`"
        )
    merged: dict = {}
    stack.append(canon)
    for inc in includes:
        if not isinstance(inc, str):
            raise SystemTomlError(f"{path}: `include` holds paths (strings), not `{inc!r}`")
        child = canon.parent / inc
        if not child.is_file():
            raise SystemTomlError(
                f"{path}: `include` names `{inc}`, but {child} is not a file (an include "
                "is relative to the file that names it)"
            )
        _merge(merged, _expand(child, stack, files, origin, False), "", child.resolve(), origin)
    stack.pop()
    _merge(merged, own, "", canon, origin)
    return merged


def _is_table_array(v) -> bool:
    return isinstance(v, list) and bool(v) and all(isinstance(x, dict) for x in v)


def _record(v, key: str, file: Path, origin: dict) -> None:
    if isinstance(v, dict):
        for k, x in v.items():
            _record(x, f"{key}.{k}", file, origin)
    else:
        origin[key] = file


def _merge(dst: dict, src: dict, prefix: str, file: Path, origin: dict) -> None:
    for k, v in src.items():
        key = f"{prefix}.{k}" if prefix else k
        if k not in dst:
            _record(v, key, file, origin)
            dst[k] = v
        elif isinstance(dst[k], dict) and isinstance(v, dict):
            _merge(dst[k], v, key, file, origin)
        elif _is_table_array(dst[k]) and _is_table_array(v):
            dst[k] = dst[k] + v
        else:
            first = origin.get(key, "another file")
            raise SystemTomlError(
                f"`{key}` is stated in both {first} and {file}; a key may be stated in "
                "one file only (an include does not override, and is not overridden), "
                "so remove one of them"
            )


def selftest(cases: Path = CASES) -> list[str]:
    """Run the shared cases; return what disagrees."""
    bad = []
    n = 0
    for case in sorted(p for p in cases.iterdir() if p.is_dir()):
        n += 1
        with open(case / "expect.toml", "rb") as fh:
            expect = tomllib.load(fh)
        root = case / expect["root"]
        try:
            table, files = load_with_files(root)
        except SystemTomlError as e:
            want = expect.get("error")
            if want is None or want not in str(e):
                bad.append(f"{case.name}: refused ({e}), want {want or 'an expansion'}")
            continue
        if "error" in expect:
            bad.append(f"{case.name}: expanded, want a refusal containing {expect['error']!r}")
        elif table != expect.get("table", {}) or len(files) != expect["files"]:
            bad.append(f"{case.name}: got {table} from {len(files)} file(s)")
        elif tomllib.loads(text(root)) != table:
            bad.append(f"{case.name}: text() does not read back as the expansion:\n{text(root)}")
    if n < 13:
        bad.append(f"only {n} shared case(s) under {cases}")
    return bad


if __name__ == "__main__":
    problems = selftest()
    for p in problems:
        print(f"system_toml selftest: {p}")
    if not problems:
        print(f"system_toml: OK -- the Python reader agrees with every shared case in {CASES}")
    sys.exit(1 if problems else 0)
