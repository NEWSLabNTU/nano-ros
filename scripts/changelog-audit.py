#!/usr/bin/env python3
"""Which merged changes since the last release carry no changelog entry: a REPORT.

Issue 2612, user ruling 2026-11-15, option (c) now and (a) later. Nothing
requires a pull request to add a `changelog.d/` fragment, so a user-facing
change can merge with none and reach a release unmentioned (#3883 did; the
nros11 pre-cut audit found it by hand). This script does that audit: it lists
every pull request merged since the last release tag that touches a SHIPPED
path and carries neither a fragment nor a `Changelog: none` trailer.

It never fails. Its output is a list for whoever writes the release's notes
page, printed in the release dry run (`release-nros.yml`, job
`changelog-audit`). Once it has been quiet across releases it becomes a pull
request check (option (a)); until then a row is a question, not an error.

  SHIPPED   packages/, cmake/, book/ — except packages/testing/ (test
            harnesses, not shipped), book/src/migrating/releases/ (the
            notes pages themselves) and the book's navigation files
            (SUMMARY.md, page-map.toml, the releases index), which alone
            change no page a reader reads. The selector nros 139 measured on the
            last 50 merged PRs (2026-11-15): 6 flagged, none missed.
  COVERED   any commit of the PR adds or edits a file under changelog.d/, or
            any commit message has a `Changelog: none` trailer line. After
            the merge, a PR that got neither is covered in two ways:
  CITED     a fragment in the checkout's changelog.d/ links the PR's own page,
            `/pull/<N>`. That is the only citation it accepts, because a
            fragment's file name and a bare `#N` both also name ISSUES
            (`1228.fix.md`, `[#1228](…/issues/1228)`), and a PR number in
            either would be a guess.
  NONE-LIST the checkout's `scripts/changelog-none.txt`: one `<PR number> <reason>`
            per line, `#` comments. It is the after-the-fact `Changelog:
            none` for a PR that merged without the trailer, because a merged
            commit cannot gain one. An entry with no reason credits nothing
            and is named in the report. Both are read from the files on
            disk, never from `until`'s tree: a credit given after the range
            still counts, and CI's tree:0 clone fetches no old blobs.
  GROUPED   by pull request, from GitHub's commit -> PR association (`gh`), so
            a PR's later commit that adds the fragment covers its earlier ones.
            With no `gh` (or a lookup that fails) each commit is its own row.
            ONE GraphQL call per 100 commits (`associatedPullRequests`, aliased),
            not one REST call per commit: the API quota is shared by every
            session on the host, and a release range is ~300 commits.
  DOCS-ONLY a group whose every path `scripts/ci/gate-tier.py` calls
            documentation (its `tier()` is `docs`: book/, book-internals/, any
            `.md` no Rust source reads) is SKIPPED, and counted in the report
            rather than dropped (user ruling 2026-11-16). One predicate, the
            gate's; this script does not keep a second.

  changelog-audit.py                    since the newest `nros-*` tag reachable from HEAD
  changelog-audit.py --since <ref> [--until <ref>]
  changelog-audit.py --summary <file>   also append a Markdown table (a job summary)
  changelog-audit.py --no-gh            one row per commit, no network
  changelog-audit.py --self-test        the selector's cases on a scratch repository
  changelog-audit.py --gate             the self-test, failing on a red (`just check changelog-audit`)

Every invocation runs the self-test first; a report from a selector whose
cases fail is not printed (a note says so), and the exit status is still 0.
"""
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402

SHIPPED = ("packages/", "cmake/", "book/")
NOT_SHIPPED = ("packages/testing/", "book/src/migrating/releases/")
# The book's NAVIGATION: a change to these alone changes no page a user reads
# (a new page is flagged by the page itself). Without this every notes-page PR
# reads as uncovered, for adding itself to the table of contents.
BOOK_NAV = ("book/src/SUMMARY.md", "book/page-map.toml", "book/src/migrating/from-older-releases.md")
FRAGMENTS = "changelog.d/"
TRAILER = re.compile(r"^Changelog:\s*none\s*$", re.I | re.M)
TAG_GLOB = "nros-*"
CITE = re.compile(r"/pull/([0-9]+)\b")
NONE_LIST = "scripts/changelog-none.txt"
GQL_BATCH = 100


def _gate_tier():
    spec = importlib.util.spec_from_file_location(
        "gate_tier", Path(__file__).resolve().parent / "ci" / "gate-tier.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


GATE_TIER = _gate_tier()


def docs_only(paths, included):
    """gate-tier's docs predicate, verbatim: every path is documentation."""
    return GATE_TIER.tier(paths, included)[0] == "docs"


def shipped(path):
    return path.startswith(SHIPPED) and not path.startswith(NOT_SHIPPED) and path not in BOOK_NAV


def git(args, cwd):
    p = subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True)
    if p.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)}: {p.stderr.strip()}")
    return p.stdout


def last_tag(cwd, until):
    """The nearest `nros-*` tag reachable from `until` (by history, not date:
    two tags made in one second tie on any date)."""
    p = subprocess.run(["git", "describe", "--tags", "--abbrev=0", "--match", TAG_GLOB, until],
                       cwd=cwd, capture_output=True, text=True)
    return p.stdout.strip() if p.returncode == 0 and p.stdout.strip() else None


def commits(cwd, since, until):
    """[(sha, subject, message, [paths])], oldest first, merges excluded."""
    shas = git(["rev-list", "--reverse", "--no-merges", f"{since}..{until}"], cwd).split()
    out = []
    for sha in shas:
        msg = git(["show", "-s", "--format=%B", sha], cwd)
        paths = [p for p in git(["diff-tree", "--no-commit-id", "--name-only", "-r", sha], cwd).splitlines() if p]
        out.append((sha, msg.splitlines()[0] if msg else "", msg, paths))
    return out


def pr_lookup(shas, use_gh):
    """{sha: (number, title)}. Tests inject; production asks GitHub."""
    inj = os.environ.get("NROS_CHANGELOG_AUDIT_PRS")
    if inj is not None:
        return {k: tuple(v) for k, v in json.loads(inj).items()}
    if not use_gh or not shutil.which("gh"):
        return {}
    repo = os.environ.get("GITHUB_REPOSITORY")
    if not repo:
        p = subprocess.run(["gh", "repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"],
                           capture_output=True, text=True)
        repo = p.stdout.strip()
    if not repo:
        return {}
    owner, _, name = repo.partition("/")
    found = {}
    for batch in chunks(shas, GQL_BATCH):
        p = subprocess.run(["gh", "api", "graphql", "-f", f"query={graphql_query(owner, name, batch)}"],
                           capture_output=True, text=True)
        if p.returncode != 0:
            continue  # those commits stay their own rows; a report, not a failure
        try:
            found.update(parse_graphql(json.loads(p.stdout), batch))
        except (json.JSONDecodeError, AttributeError, TypeError):
            pass
    return found


def chunks(xs, n):
    return [xs[i:i + n] for i in range(0, len(xs), n)]


def graphql_query(owner, name, shas):
    """One query for a batch: alias `c<i>` per commit."""
    body = " ".join(
        f'c{i}: object(oid: "{sha}") {{ ... on Commit {{ associatedPullRequests(first: 5) '
        f'{{ nodes {{ number title merged }} }} }} }}' for i, sha in enumerate(shas))
    return f'query {{ repository(owner: "{owner}", name: "{name}") {{ {body} }} }}'


def parse_graphql(resp, shas):
    """{sha: (number, title)} for each alias with a MERGED pull request."""
    repo = ((resp or {}).get("data") or {}).get("repository") or {}
    out = {}
    for i, sha in enumerate(shas):
        nodes = (((repo.get(f"c{i}") or {}).get("associatedPullRequests") or {}).get("nodes")) or []
        merged = [n for n in nodes if n and n.get("merged")]
        if merged:
            out[sha] = (merged[0]["number"], merged[0]["title"])
    return out


def cited_prs(cwd):
    """PR numbers a fragment in the checkout links as `/pull/<N>`."""
    found = set()
    for f in sorted((Path(cwd) / FRAGMENTS).glob("*.md")):
        found |= {int(m) for m in CITE.findall(f.read_text(errors="replace"))}
    return found


def none_list(cwd):
    """({PR number: reason}, [malformed lines]) from the checkout's NONE_LIST."""
    f = Path(cwd) / NONE_LIST
    if not f.is_file():
        return {}, []
    ok, bad = {}, []
    for line in f.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):  # a whole-line comment; a reason may cite `#N`
            continue
        num, _, reason = line.partition(" ")
        if num.isdigit() and reason.strip():
            ok[int(num)] = reason.strip()
        else:
            bad.append(line)
    return ok, bad


def audit(cwd, since, until, use_gh, included=None):
    """Rows (label, title, [shipped paths]) for uncovered groups, and how many
    otherwise-flagged groups were skipped as docs-only."""
    if included is None:
        included = GATE_TIER.included_markdown()
    cs = commits(cwd, since, until)
    prs = pr_lookup([c[0] for c in cs], use_gh)
    groups = {}
    for sha, subject, msg, paths in cs:
        key = ("pr", prs[sha][0]) if sha in prs else ("commit", sha)
        g = groups.setdefault(key, {"title": prs[sha][1] if sha in prs else subject,
                                    "paths": [], "covered": False})
        g["paths"] += [p for p in paths if p not in g["paths"]]
        if any(p.startswith(FRAGMENTS) for p in paths) or TRAILER.search(msg):
            g["covered"] = True
    cited = cited_prs(cwd)
    nones, bad = none_list(cwd)
    rows, skipped, credit = [], 0, {"cited": 0, "listed": 0, "bad": bad}
    for (kind, ident), g in groups.items():
        ship = [p for p in g["paths"] if shipped(p)]
        if ship and not g["covered"]:
            if docs_only(g["paths"], included):
                skipped += 1
                continue
            if kind == "pr" and ident in cited:
                credit["cited"] += 1
                continue
            if kind == "pr" and ident in nones:
                credit["listed"] += 1
                continue
            rows.append((f"#{ident}" if kind == "pr" else ident[:10], g["title"], ship))
    return rows, skipped, len(cs), len(groups), bool(prs), credit


def render(rows, skipped, since, until, n_commits, n_groups, by_pr, credit=None):
    credit = credit or {"cited": 0, "listed": 0, "bad": []}
    unit = "pull request(s)" if by_pr else "commit(s), no PR lookup"
    after = (f"{credit['cited']} more credited by a fragment linking `/pull/<N>`, "
             f"{credit['listed']} by {NONE_LIST}")
    head = (f"changelog-audit: {len(rows)} of {n_groups} {unit} merged in {since}..{until} "
            f"({n_commits} commits) touch shipped paths with no changelog.d fragment "
            "and no `Changelog: none` trailer (report only, issue 2612); "
            f"{skipped} more skipped as docs-only (gate-tier's docs predicate); {after}")
    lines = [head]
    for b in credit["bad"]:
        lines.append(f"  {NONE_LIST}: `{b}` credits nothing (want `<PR number> <reason>`)")
    for label, title, paths in rows:
        shown = ", ".join(paths[:6]) + (f", … (+{len(paths) - 6})" if len(paths) > 6 else "")
        lines.append(f"  {label}  {title}\n      {shown}")
    md = [f"### Changes with no changelog entry ({since}..{until})", "",
          f"Report only (issue 2612): {len(rows)} of {n_groups} {unit}; "
          f"{skipped} more skipped as docs-only; {after}. "
          "Each row should be in the release's notes page or carry `Changelog: none`.", ""]
    if rows:
        md += ["| change | title | shipped paths |", "| --- | --- | --- |"]
        for label, title, paths in rows:
            shown = ", ".join(f"`{p}`" for p in paths[:6]) + (f" (+{len(paths) - 6})" if len(paths) > 6 else "")
            md.append(f"| {label} | {title.replace('|', '/')} | {shown} |")
    return "\n".join(lines), "\n".join(md) + "\n"


# ------------------------------------------------------------------ self-test

def self_test():
    env = nros_clear_inherited_git_env(dict(os.environ))
    env.update(GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@t", GIT_COMMITTER_NAME="t",
               GIT_COMMITTER_EMAIL="t@t", GIT_CONFIG_NOSYSTEM="1")
    saved = dict(os.environ)
    os.environ.clear()
    os.environ.update(env)
    fails = []
    try:
        with tempfile.TemporaryDirectory() as t:
            r = Path(t)
            git(["init", "-q", "-b", "main", str(r)], r)
            n = [0]

            def commit(paths, msg=None):
                for p in paths:
                    f = r / p
                    f.parent.mkdir(parents=True, exist_ok=True)
                    n[0] += 1
                    f.write_text(str(n[0]))
                git(["add", *paths], r)
                git(["commit", "-q", "-m", msg or f"c{n[0]}: {paths[0]}"], r)
                return git(["rev-parse", "HEAD"], r).strip()

            commit(["packages/old.rs"])
            git(["tag", "nros-0.1.0-a"], r)
            commit(["packages/before-second-tag.rs"])
            git(["tag", "nros-0.1.0-b"], r)
            want = {}
            s = commit(["packages/core/a.rs"]); want[s] = True                       # plain, uncovered
            s = commit(["packages/core/b.rs", "changelog.d/+b.fix.md"]); want[s] = False  # fragment
            s = commit(["packages/core/c.rs"], "c: x\n\nChangelog: none\n"); want[s] = False  # trailer
            s = commit(["packages/core/c2.rs"], "c2: x\n\nchangelog: NONE\n"); want[s] = False  # trailer, any case
            s = commit(["packages/core/c3.rs"], "c3: Changelog: none is not a trailer here\n"); want[s] = True
            s = commit(["packages/testing/t.rs"]); want[s] = False                   # test harness
            s = commit(["book/src/migrating/releases/0.1.0-b.md"]); want[s] = False  # a notes page
            s = commit(["scripts/x.sh", ".github/workflows/w.yml"]); want[s] = False  # not shipped
            s = commit(["book/src/SUMMARY.md", "book/page-map.toml",
                        "book/src/migrating/releases/0.1.0-c.md"]); want[s] = False  # a notes page + nav
            s = commit(["book/src/SUMMARY.md", "book/src/guide/new.md"]); want[s] = False  # docs-only: skipped
            s = commit(["cmake/a.cmake"]); want[s] = True
            s = commit(["book/src/guide/g.md"]); want[s] = False                     # docs-only: skipped
            s = commit(["book/src/guide/h.md", "packages/core/h.rs"]); want[s] = True  # book + code: flagged
            s = commit(["book/src/guide/read-by-rust.md"]); want[s] = True         # a Rust source reads it
            s = commit(["book/src/guide/i.md", "docs/issues/x.md", "README.md"]); want[s] = False  # docs-only
            s = commit(["packages/testing/u.rs", "packages/core/d.rs"]); want[s] = True  # mixed: shipped half counts
            # one PR, two commits, fragment in the later one -> covered
            p1 = commit(["packages/core/e.rs"]); p2 = commit(["changelog.d/+e.fix.md"])
            # one PR, two commits, neither covered -> ONE row, paths unioned
            q1 = commit(["packages/core/f.rs"]); q2 = commit(["cmake/f.cmake"])
            # merged with neither; credited after the fact
            c1 = commit(["packages/core/cited.rs"])     # PR 103: a later fragment links /pull/103
            c2 = commit(["packages/core/listed.rs"])    # PR 104: on the none-list, with a reason
            c3 = commit(["packages/core/noreason.rs"])  # PR 105: on the none-list, no reason
            c4 = commit(["packages/core/issue.rs"])     # PR 106: cited only as #106 and /issues/106
            (r / "changelog.d").mkdir(exist_ok=True)
            (r / "changelog.d" / "+late.fix.md").write_text(
                "x ([#103](https://github.com/o/n/pull/103)); y ([#106](https://github.com/o/n/issues/106))\n")
            (r / "changelog.d" / "106.fix.md").write_text("issue-named, not PR-named\n")
            (r / NONE_LIST).parent.mkdir(parents=True, exist_ok=True)
            (r / NONE_LIST).write_text("# PR reason\n104 test-only, cites #9 in its reason\n105\n")
            git(["add", "changelog.d", NONE_LIST], r)
            git(["commit", "-q", "-m", "late credit"], r)

            os.environ["NROS_CHANGELOG_AUDIT_PRS"] = json.dumps({p1: [101, "e"], p2: [101, "e"],
                                                                 q1: [102, "f"], q2: [102, "f"],
                                                                 c1: [103, "c"], c2: [104, "l"],
                                                                 c3: [105, "n"], c4: [106, "i"]})
            tag = last_tag(str(r), "HEAD")
            if tag != "nros-0.1.0-b":
                fails.append(f"last tag: want nros-0.1.0-b, got {tag}")
            rows, skipped, _, _, _, credit = audit(str(r), tag, "HEAD", use_gh=False,
                                                   included={"book/src/guide/read-by-rust.md"})
            if skipped != 3:
                fails.append(f"docs-only skipped count: want 3, got {skipped}")
            got = {lab for lab, _, _ in rows}
            for sha, flagged in want.items():
                if (sha[:10] in got) != flagged:
                    fails.append(f"{git(['show', '-s', '--format=%s', sha], r).strip()}: "
                                 f"want {'flagged' if flagged else 'quiet'}")
            if "#101" in got:
                fails.append("a PR whose later commit adds the fragment was flagged")
            q = [x for x in rows if x[0] == "#102"]
            if len(q) != 1 or set(q[0][2]) != {"packages/core/f.rs", "cmake/f.cmake"}:
                fails.append(f"PR #102: want one row over both commits' paths, got {q}")
            mixed = [x for x in rows if "packages/core/d.rs" in x[2]]
            if mixed and "packages/testing/u.rs" in mixed[0][2]:
                fails.append("a row lists a non-shipped path")
            if any("old.rs" in p or "before-second-tag" in p for x in rows for p in x[2]):
                fails.append("a commit from before the last tag was reported")
            for pr, want_row in ((103, False), (104, False), (105, True), (106, True)):
                if (f"#{pr}" in got) != want_row:
                    fails.append(f"PR #{pr}: want {'flagged' if want_row else 'credited'}")
            if (credit["cited"], credit["listed"], credit["bad"]) != (1, 1, ["105"]):
                fails.append(f"credit: want 1 cited, 1 listed, ['105'] malformed; got {credit}")
            text, md = render(rows, skipped, tag, "HEAD", 1, 1, True, credit)
            if "1 more credited by a fragment" not in text or "`105` credits nothing" not in text:
                fails.append("the report does not state the after-the-fact credit and the bad entry")
            if "#102" not in text or "| #102 |" not in md:
                fails.append("the report does not name a flagged PR")
            if "3 more skipped as docs-only" not in text or "3 more skipped as docs-only" not in md:
                fails.append("the report does not state the docs-only skipped count")
            # The batched lookup: one query per GQL_BATCH commits, aliases map back.
            many = [f"{i:040x}" for i in range(GQL_BATCH * 2 + 1)]
            if [len(b) for b in chunks(many, GQL_BATCH)] != [GQL_BATCH, GQL_BATCH, 1]:
                fails.append("batching: 201 commits do not split 100/100/1")
            qry = graphql_query("o", "n", many[:3])
            if qry.count("object(oid:") != 3 or 'c2: object(oid: "' + many[2] not in qry:
                fails.append("graphql query does not alias each commit")
            resp = {"data": {"repository": {
                "c0": {"associatedPullRequests": {"nodes": [{"number": 7, "title": "t7", "merged": True}]}},
                "c1": {"associatedPullRequests": {"nodes": [{"number": 8, "title": "open", "merged": False}]}},
                "c2": None}}}
            if parse_graphql(resp, many[:3]) != {many[0]: (7, "t7")}:
                fails.append("graphql parse: want only the merged PR of c0")
    finally:
        os.environ.clear()
        os.environ.update(saved)
    for f in fails:
        print(f"changelog-audit self-test FAIL: {f}", file=sys.stderr)
    if fails:
        return 1
    print("changelog-audit self-test: OK (tag choice, fragment, trailer, exclusions, book navigation, docs-only skip, PR grouping, batched lookup, /pull/ citation, none-list)")
    return 0


def quiet(fn):
    import contextlib
    import io
    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
        return fn()


def main(argv):
    # The selector's cases run on every path (check-gate-selftests). A report
    # whose selector is broken would mislead, so it is withheld — but this is a
    # REPORT, so the status stays 0 either way; only --gate fails on a red.
    gate = "--gate" in argv or "--self-test" in argv
    rc = self_test() if gate else quiet(self_test)
    if gate:
        return rc
    if rc != 0:
        print("changelog-audit: NOT RUN, its self-test fails (just check changelog-audit); "
              "audit the release by hand")
        return 0
    nros_clear_inherited_git_env()
    cwd = os.getcwd()
    until = argv[argv.index("--until") + 1] if "--until" in argv else "HEAD"
    try:
        since = argv[argv.index("--since") + 1] if "--since" in argv else last_tag(cwd, until)
        if not since:
            print(f"changelog-audit: no `{TAG_GLOB}` tag is reachable from {until}; nothing to compare")
            return 0
        rows, skipped, nc, ng, by_pr, credit = audit(cwd, since, until, use_gh="--no-gh" not in argv)
    except (RuntimeError, OSError) as e:
        print(f"changelog-audit: could not audit ({e}); report only, so this is not a failure")
        return 0
    text, md = render(rows, skipped, since, until, nc, ng, by_pr, credit)
    print(text)
    if "--summary" in argv:
        try:
            with open(argv[argv.index("--summary") + 1], "a") as f:
                f.write(md)
        except OSError as e:
            print(f"changelog-audit: summary not written ({e})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
