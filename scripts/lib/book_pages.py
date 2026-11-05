"""The page map of both books, in one place (RFC-0123 D2, phase-532 W0).

`book/page-map.toml` holds one row per page of the nano-ros Book (`book/`)
and nano-ros Internals (`book-internals/`): where the page is today (`old`),
which book and path it belongs at (`book`, `path`), what phase-532 does to it
(`action`) and in which work item (`batch`). Gates and generators that need to
know "which pages are the user book" or "where did this page go" ask this
module instead of hardcoding `book/src/<dir>/*.md` globs.

A page's *current* location is `old` until its batch has run and `path`
after; `current()` answers by looking at the tree, so the same map serves
every step of the migration without a "done" flag to keep in sync.
"""

from __future__ import annotations

import pathlib
from dataclasses import dataclass

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib

ROOT = pathlib.Path(__file__).resolve().parents[2]
MAP = ROOT / "book" / "page-map.toml"
BOOKS = {"user": ROOT / "book" / "src", "internals": ROOT / "book-internals" / "src"}

# RFC-0123 D2: a page's directory is its part. Order is SUMMARY order.
PARTS = {
    "user": [
        ("start", "Start"),
        ("get-started", "Get Started"),
        ("build", "Build a Project"),
        ("targets", "Run on a Target"),
        ("communicate", "Communicate"),
        ("realtime", "Real-Time and Safety"),
        ("operate", "Operate and Debug"),
        ("from-ros2", "Coming from ROS 2"),
        ("concepts", "Concepts and Rationale"),
        ("extending", "Extending nano-ros"),
        ("reference", "Reference"),
        ("migrating", "Migrating from Older Releases"),
    ],
    "internals": [
        ("orientation", "Orientation"),
        ("dev-env", "Development Environment"),
        ("pipeline", "Build Pipeline and Codegen"),
        ("rmw", "RMW Internals"),
        ("platform", "Platform Internals"),
        ("realtime", "Real-Time Internals"),
        ("testing", "Testing and Verification"),
        ("release", "Release"),
    ],
}
ACTIONS = ("keep", "move", "merge-into", "split", "new", "import")


@dataclass(frozen=True)
class Page:
    book: str  # "user" | "internals"
    path: str  # path under BOOKS[book] after phase-532
    action: str  # keep | move | merge-into <p> | split <p> | new <owner>
    batch: str
    old: str | None = None  # path under book/src today; None for a new page
    generated: str | None = None
    note: str | None = None

    @property
    def verb(self) -> str:
        return self.action.split()[0]

    @property
    def arg(self) -> str | None:
        parts = self.action.split(maxsplit=1)
        return parts[1] if len(parts) > 1 else None

    @property
    def part(self) -> str:
        return self.path.split("/", 1)[0]

    @property
    def target(self) -> pathlib.Path:
        return BOOKS[self.book] / self.path

    def batch_done(self) -> bool:
        """Whether this row's batch has run, judged by the other rows of the
        same batch: any of them at its target means the batch was applied."""
        return any(q.batch == self.batch and q.verb in ("move", "split") and q.target.exists()
                   for q in load())

    @property
    def source(self) -> pathlib.Path | None:
        """For `import <repo path>`: the document outside the books that the
        page is imported from (phase-532 W7). It keeps a one-line "moved"
        stub after the import."""
        return ROOT / self.arg if self.verb == "import" and self.arg else None

    def current(self) -> pathlib.Path | None:
        """Where the page is in the tree right now: its target if its batch
        has run, its old path if not, None if neither exists (a `new` page
        not yet written, or a merged page already folded in). A generated page
        is always at its target."""
        if self.verb != "merge-into" and self.target.exists():
            return self.target
        if self.generated and self.verb != "merge-into":
            # A generated page is gitignored and written by its generator
            # before mdbook runs (`just book`), so a clean checkout lacks the
            # file; it is still a page at its target.
            return self.target
        if self.verb == "import":
            return None  # not a book page until its batch runs; see `source`
        if self.old and (BOOKS["user"] / self.old).exists():
            return BOOKS["user"] / self.old
        return None


def page_file(old: str) -> pathlib.Path:
    """The file a page that lived at `book/src/<old>` is at NOW (or will be
    written to, for a generator). A generator names its page by the path it
    had when the map was seeded and asks here, so moving the page is a map
    edit and never a generator edit (RFC-0123 D7)."""
    p = by_old(load()).get(old)
    if p is None:
        raise KeyError(f"book/page-map.toml has no row for {old!r}")
    cur = p.current()
    if cur is not None:
        return cur
    # Not on disk (a gitignored generated page before its first run): it goes
    # wherever the map says it lives once its batch has run, which is its old
    # path until that book exists.
    return p.target if p.batch_done() else BOOKS["user"] / old


# RFC-0123 D7: where each book is served, relative to the site root.
DEPLOY = {"user": "", "internals": "internals/"}


def book_of(path: pathlib.Path) -> str | None:
    for b, src in BOOKS.items():
        if src.resolve() == path.resolve() or src.resolve() in path.resolve().parents:
            return b
    return None


def deploy_url(path: pathlib.Path) -> str:
    """Site-relative deploy URL (no leading slash) of a page source file."""
    b = book_of(path)
    if b is None:
        raise ValueError(f"{path} is in neither book")
    inner = pathlib.PurePosixPath(path.resolve().relative_to(BOOKS[b].resolve()))
    return DEPLOY[b] + str(inner.with_suffix(".html"))


def link(src: pathlib.Path, dest: pathlib.Path) -> str:
    """The link text page `src` uses to reach page `dest`: a relative `.md`
    path inside one book, a relative `.html` path between deploy roots across
    books (D7). Generators call this rather than spelling a path."""
    import os

    if book_of(src) == book_of(dest):
        return os.path.relpath(dest, src.parent)
    return os.path.relpath(deploy_url(dest), os.path.dirname(deploy_url(src)))


def scrub_ids(text: str) -> str:
    """Drop internal-id citations from text a generator copies out of the
    tree into a USER page (RFC-0123 D3): a parenthetical that is only a
    citation, `(issue 1037)`, `(phase-479 W5)`, `(RFC-0100 D4; issue 1340)`,
    and a trailing `, issue NNNN` / `— issue NNNN`. The source keeps the
    citation; the reader of the book has no tree to look it up in. Prose
    that NAMES an id as its subject is left for the gate to report, so a
    sentence is never silently mangled."""
    import re

    cite = r"(?:[Pp]hase[- ]\d{2,4}(?:\s+[A-Z]\d+[\w.']*)?|[Ii]ssues?[- ]#?\d{3,4}(?:\s*(?:,|and)\s*#?\d{3,4})*|RFC-?\d{4}(?:\s+[A-Z]\d+[\w.]*)?|#\d{3,4})"
    text = re.sub(r"\s*\((?:" + cite + r")(?:\s*[;,/]\s*" + cite + r")*\)", "", text)
    text = re.sub(r"\s*[,—–-]\s*" + cite + r"(?=[.;:)]|$)", "", text, flags=re.M)
    # "the default since issue 1424" -> "the default"; a leading "issue 1808: "
    text = re.sub(r"\s+since\s+" + cite + r"\b", "", text)
    text = re.sub(r"(?<=[*:|] )" + cite + r":\s+", "", text)
    return text


def relink(text: str, seeded_old: str) -> str:
    """Rewrite a generated page's relative `.md` links for where things are
    now. A generator writes its links as they were when its page sat at
    `book/src/<seeded_old>` (the map's `old` for that page); each link is
    read from there, followed through the map, and re-spelled from the page's
    current location (`link`, so a target now in the other book gets a
    cross-book `.html` link). Links that do not name a mapped page are left
    alone. Generators call this once on their output, so a page move never
    edits a generator (RFC-0123 D7)."""
    import re

    pages = load()
    here = page_file(seeded_old)
    seeded_dir = BOOKS["user"] / pathlib.PurePosixPath(seeded_old).parent
    olds = by_old(pages)

    def sub(m):
        target = m.group(2)
        if re.match(r"^(?:[a-z][a-z0-9+.-]*:|/|#)", target, re.I) or not target.endswith(".md"):
            return m.group(0)
        try:
            rel = str((seeded_dir / target).resolve().relative_to(BOOKS["user"].resolve()))
        except ValueError:
            return m.group(0)
        row = olds.get(rel)
        if row is None:
            return m.group(0)
        dest_rel = row.arg if row.verb == "merge-into" else None
        dest = page_file(rel) if dest_rel is None else next(
            (q.current() for q in pages if q.book == "user" and q.path == dest_rel and q.verb != "merge-into"), None)
        if dest is None:
            return m.group(0)
        return f"{m.group(1)}{link(here, dest)}{m.group(3) or ''})"

    return re.sub(r"(\]\()([^)\s#]+)(#[^)\s]*)?\)", sub, text)


def book_files() -> list[pathlib.Path]:
    """Every `.md` file of either book that git knows about, tracked or not
    yet added (but not ignored), from the index rather than a filesystem walk.
    A page written but not yet `git add`ed is still a page the gates must
    see. Gitignored generated pages are absent until `just book` writes them,
    and the SUMMARY cross-check covers those."""
    import subprocess

    out = subprocess.run(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--",
         "book/src/*.md", "book-internals/src/*.md"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    return sorted({ROOT / f for f in out.split("\0") if f and (ROOT / f).exists()})


def load(path: pathlib.Path = MAP) -> list[Page]:
    with open(path, "rb") as fh:
        data = tomllib.load(fh)
    if data.get("schema") != 1:
        raise ValueError(f"{path}: unknown schema {data.get('schema')!r}")
    return [Page(**row) for row in data["page"]]


def by_old(pages: list[Page]) -> dict[str, Page]:
    return {p.old: p for p in pages if p.old}


def new_path(old: str, pages: list[Page] | None = None) -> tuple[str, str] | None:
    """(book, path) a page that lived at book/src/<old> belongs at, or None."""
    p = by_old(pages if pages is not None else load()).get(old)
    if p is None:
        return None
    if p.verb == "merge-into" and p.arg:
        return ("user", p.arg)
    return (p.book, p.path)


def user_pages(pages: list[Page] | None = None) -> list[pathlib.Path]:
    """Every user-book page that exists in the tree right now, wherever it
    sits in the migration. This is the user-surface gates' subject (D3)."""
    out = []
    for p in pages if pages is not None else load():
        if p.book != "user":
            continue
        cur = p.current()
        if cur is not None and cur not in out:
            out.append(cur)
    return out


def user_page_paths(pages: list[Page] | None = None) -> set[str]:
    """Repo-relative paths a USER-book page may have at any step of the
    migration (its old path and its target). This is the user-surface gates'
    scope (RFC-0123 D3): membership comes from the map, not from a list of
    directories, so a page moved into Internals leaves the scope by the same
    edit that moves it."""
    out: set[str] = set()
    for p in pages if pages is not None else load():
        if p.book != "user":
            continue
        out.add(f"book/src/{p.path}")
        if p.old:
            out.add(f"book/src/{p.old}")
    return out


def tracked_user_pages() -> list[str]:
    """User-book pages that git tracks right now, repo-relative, sorted."""
    scope = user_page_paths()
    return sorted(str(f.relative_to(ROOT)) for f in book_files() if str(f.relative_to(ROOT)) in scope)


def check(pages: list[Page]) -> list[str]:
    """Structural problems with the map itself, as strings. Empty = clean."""
    errs: list[str] = []
    dirs = {b: {d for d, _ in PARTS[b]} for b in PARTS}
    seen_old: set[str] = set()
    seen_target: dict[tuple[str, str], Page] = {}
    for p in pages:
        where = p.old or p.path
        if p.book not in PARTS:
            errs.append(f"{where}: book {p.book!r} is not user|internals")
            continue
        if p.verb not in ACTIONS:
            errs.append(f"{where}: action {p.action!r} is not one of {ACTIONS}")
        if p.part not in dirs[p.book]:
            errs.append(f"{where}: {p.path} is not under a {p.book} part directory {sorted(dirs[p.book])}")
        if p.verb in ("keep", "move", "merge-into", "split") and not p.old:
            errs.append(f"{p.path}: action {p.verb} needs an `old` path")
        if p.verb in ("new", "import") and p.old:
            errs.append(f"{p.old}: a `{p.verb}` page has no `old` path")
        if p.verb == "import" and p.arg and p.arg.startswith(("book/", "book-internals/")):
            errs.append(f"{p.path}: `import` is for documents outside the books, not {p.arg}")
        if p.verb in ("merge-into", "split", "new", "import") and not p.arg:
            errs.append(f"{where}: `{p.verb}` needs an argument")
        if p.verb == "keep" and p.old != p.path:
            errs.append(f"{p.old}: `keep` but path differs ({p.path}); use `move`")
        if p.old:
            if p.old in seen_old:
                errs.append(f"{p.old}: two rows")
            seen_old.add(p.old)
        if p.verb == "merge-into":
            if p.arg != p.path:
                errs.append(f"{p.old}: merge-into {p.arg} but path is {p.path}")
            continue  # several rows may merge into one target
        key = (p.book, p.path)
        if key in seen_target:
            errs.append(f"{p.path}: target of both {seen_target[key].old} and {p.old}")
        seen_target[key] = p
    for p in pages:
        if p.verb == "merge-into" and ("user", p.arg) not in seen_target:
            errs.append(f"{p.old}: merges into {p.arg}, which no row owns")
        if p.verb == "split" and ("internals", p.arg) not in seen_target:
            errs.append(f"{p.old}: splits into internals {p.arg}, which no row owns")
    return errs
