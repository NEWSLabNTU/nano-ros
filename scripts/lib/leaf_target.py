"""The rustc triple a single-package leaf builds for — phase-445 W6.

A leaf used to pin its own triple in `.cargo/config.toml` `[build] target`, and
several gates read it from there. RFC-0098 D1 deleted that file: a leaf states
ONE board in its `system.toml`, and the triple comes from that board's
descriptor `cargo_config`. Two hops instead of one, and the reason this is a
shared helper rather than a second copy in each gate:

**The old read failed OPEN.** `if not cfg.is_file(): return True` meant "no
config, so it is a HOST build" — which was right when every cross leaf had a
config and is now right for none of them. A gate that asked "is this leaf
hosted" to decide whether to require a `#[panic_handler]` went from checking 30
embedded leaves to checking none, and stayed green the whole way. That is the
0196 shape, so the answer belongs in one place that either ANSWERS or says it
cannot.

No CLI: the fast lane is contractually CLI-free, and this is two TOML reads.
The board catalog is `packages/boards/*/nros-board.toml`, whose `[[board]]
names` is the same list `nros ws board-facts` resolves against.
"""

import re
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]


def leaf_board(leaf_dir) -> "str | None":
    """The board `[image.<id>] board` names, or `None`.

    `None` when there is no `system.toml`, no `[image.*]`, or no `board` key —
    i.e. when the leaf does not state a deployment. It is deliberately NOT an
    error: a node/library crate under `examples/` legitimately states none.
    """
    system = Path(leaf_dir) / "system.toml"
    if not system.is_file():
        return None
    text = system.read_text(encoding="utf-8", errors="replace")
    # `[image.<id>]` (or `[image_defaults]`) then the first `board = "..."`
    # under it. Written as a scan rather than a TOML parse so this helper keeps
    # the zero-dependency property the fast lane's other gates have.
    in_image = False
    for line in text.splitlines():
        s = line.split("#", 1)[0].strip()
        if s.startswith("["):
            in_image = s.startswith("[image.") or s == "[image_defaults]"
            continue
        if not in_image:
            continue
        m = re.match(r'board\s*=\s*"([^"]+)"', s)
        if m:
            return m.group(1)
    return None


def _catalog() -> "dict[str, str]":
    """board NAME (every alias) -> its descriptor's `[build] target`, or "".

    An empty string is a real answer: a board with a `cargo_config` that states
    no triple builds for the HOST. A board missing from this map is a typo, and
    the caller decides what to do about it.
    """
    out = {}
    for desc in sorted((REPO / "packages" / "boards").glob("*/nros-board.toml")):
        text = desc.read_text(encoding="utf-8", errors="replace")
        # One descriptor may declare several `[[board]]` blocks.
        for block in re.split(r"^\[\[board\]\]", text, flags=re.M)[1:]:
            names = re.search(r'^names\s*=\s*\[([^\]]*)\]', block, re.M)
            if not names:
                continue
            cfg = re.search(r"^cargo_config\s*=\s*'''(.*?)'''", block, re.M | re.S)
            triple = ""
            if cfg:
                t = re.search(
                    r'^\[build\]\s*$.*?^\s*target\s*=\s*"([^"]+)"',
                    cfg.group(1),
                    re.M | re.S,
                )
                if t:
                    triple = t.group(1)
            for n in re.findall(r'"([^"]+)"', names.group(1)):
                out[n] = triple
    return out


_CATALOG = None


def leaf_target(leaf_dir) -> "tuple[str | None, str]":
    """`(triple, why)` for `leaf_dir`.

    `triple` is `None` when it could not be resolved, and `why` always says
    which hop answered — so a caller can REPORT an unresolved leaf instead of
    treating it as the host, which is the failure this helper exists to stop.
    An empty-string triple means "the host", stated by a board that pins none.
    """
    global _CATALOG
    board = leaf_board(leaf_dir)
    if board is None:
        return None, "no `[image.<id>] board` in system.toml"
    if _CATALOG is None:
        _CATALOG = _catalog()
    if board not in _CATALOG:
        return None, f"board `{board}` is claimed by no descriptor"
    return _CATALOG[board], f"board `{board}`"


def leaf_crate_root_extra(leaf_dir) -> "str | None":
    """The board's `[board.entry] crate_root_extra`, for the leaf's board.

    Issue 1305 — a leaf that names its generated selection package
    ([`names_selection_package`]) gets these crate-root items from that
    package, not from its own source, so a gate asking "what does this image
    supply" has to read them here. `None` when the board does not resolve;
    `""` when it states none.
    """
    board = leaf_board(leaf_dir)
    if board is None:
        return None
    for desc in sorted((REPO / "packages" / "boards").glob("*/nros-board.toml")):
        text = desc.read_text(encoding="utf-8", errors="replace")
        for block in re.split(r"^\[\[board\]\]", text, flags=re.M)[1:]:
            names = re.search(r'^names\s*=\s*\[([^\]]*)\]', block, re.M)
            if not names or board not in re.findall(r'"([^"]+)"', names.group(1)):
                continue
            entry = re.search(r"^\[board\.entry\]\s*$(.*?)(?=^\[|\Z)", block, re.M | re.S)
            if not entry:
                return ""
            extra = re.search(r'^crate_root_extra\s*=\s*"((?:[^"\\]|\\.)*)"', entry.group(1), re.M)
            return extra.group(1) if extra else ""
    return None


def names_selection_package(leaf_dir) -> bool:
    """Does the leaf's `Cargo.toml` name its generated `<pkg>_nros_selection`
    package (issue 1305)? The same spelling `nros::main!` and `nros sync` use:
    the `[package] name` with `-` folded to `_`."""
    manifest = Path(leaf_dir) / "Cargo.toml"
    if not manifest.is_file():
        return False
    text = manifest.read_text(encoding="utf-8", errors="replace")
    m = re.search(r'^\[package\]\s*$.*?^name\s*=\s*"([^"]+)"', text, re.M | re.S)
    if not m:
        return False
    want = m.group(1).replace("-", "_") + "_nros_selection"
    return re.search(rf"^\s*{re.escape(want)}\s*=", text, re.M) is not None
