#!/usr/bin/env python3
"""issue 0560 — a lock whose dep versions are decided by a SUBMODULE must resolve.

## The failure this catches

`packages/cli/nros-launch-resolve` path-deps into the `play_launch` submodule,
whose `ros-launch-resolve/Cargo.toml` git-deps `ros-launch-manifest` by TAG. So
that leaf's lock is pinned by a manifest living outside its own tree: advance the
submodule pointer and the lock is stale, with nothing relating the two halves.

That happened. The submodule moved to rlm v0.1.6 while the lock still pinned
v0.1.4, and `--locked` (injected project-wide by `scripts/bin/cargo`) made the
leaf unbuildable:

    error: cannot update the lock file … because --locked was passed

It survived on main because the only consumer, `just setup-launch-resolve`, is a
dependency of `build-test-fixtures` and nothing else — so the break waited for
whoever next ran the ~40-minute fixture lane, rather than failing its author.

## Why `cargo metadata`, not a build

Resolution is what broke, so resolution is what to check. `cargo metadata
--locked --offline` reproduces the failure in seconds without compiling
anything, and `--offline` keeps this gate off the network: a correct lock needs
no fetch, and an incorrect one fails on the lock rather than on connectivity.

Both directions were verified against the real pre-fix lock (`567101c43~1`)
before this gate was written: rc=101 on the broken lock, rc=0 on the fixed one,
offline in both cases.

## The leaf set is DERIVED, not listed

A hardcoded path would go stale the first time another leaf grew a submodule
dep — the exact class of drift this repo keeps paying for. A leaf qualifies when
it has a tracked `Cargo.lock` AND its manifest carries a `path = …` dependency
resolving inside a path registered in `.gitmodules`. Today that is one leaf; the
rule is what matters.
"""
import configparser
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
PATH_DEP = re.compile(r'path\s*=\s*"([^"]+)"')


def submodule_paths():
    """Every registered submodule path, from `.gitmodules`."""
    gm = REPO / ".gitmodules"
    if not gm.is_file():
        return []
    cp = configparser.ConfigParser()
    # .gitmodules section headers are `[submodule "name"]`, which configparser
    # handles; values are indented, which it also handles.
    cp.read_string(gm.read_text())
    out = []
    for section in cp.sections():
        p = cp[section].get("path")
        if p:
            out.append((REPO / p).resolve())
    return out


def exposed_leaves():
    """Leaves whose lock is pinned by a manifest outside their own tree."""
    subs = submodule_paths()
    if not subs:
        return []
    locks = subprocess.run(
        ["git", "ls-files", "Cargo.lock", "*/Cargo.lock"],
        cwd=REPO, capture_output=True, text=True, check=True,
    ).stdout.split()

    found = []
    for lock in locks:
        leaf = (REPO / lock).parent
        manifest = leaf / "Cargo.toml"
        if not manifest.is_file():
            continue
        for rel in PATH_DEP.findall(manifest.read_text()):
            target = (leaf / rel).resolve()
            if any(target == s or s in target.parents for s in subs):
                found.append((leaf, target))
                break
    return found


# issue 0600 — cargo's own words separate the two causes. An `--offline` run
# that needs a crate it has not cached says so explicitly; a lock that does not
# satisfy its manifest fails during RESOLUTION and never mentions the network.
# Matched on both halves of the sentence so a message that merely contains the
# word "offline" (a crate named `offline`, a path with it) is not misread.
def _is_offline_cache_miss(stderr):
    """True when the failure is a property of THIS HOST's registry cache.

    issue 0863 — the first version matched only the DOWNLOAD wordings, and cargo
    has a third shape that carries no download at all: when the crate is absent
    from the cache entirely, resolution fails during SELECTION.

        error: no matching package named `clap` found
        location searched: crates.io index
        required by package `nros-launch-resolve v0.5.0 (…)`
        note: offline mode (via `--offline`) can sometimes cause surprising
              resolution failures

    Reproduced locally with an empty `CARGO_HOME`, and identical line-for-line
    to the CI red. Classified as a MISMATCH, it told an operator to
    `lock-update` a byte-correct lock — the exact churn 0600 exists to prevent,
    in the imperative, to a reader with no reason to doubt.

    The last line is cargo ITSELF saying the verdict may be an offline artifact,
    so key on it rather than on guessing which nouns appear. A genuine
    `--locked` mismatch is disjoint: it says the lock file needs updating and
    never mentions offline mode.
    """
    text = stderr or ""
    # cargo's own hedge about the whole verdict. Present in every offline-caused
    # resolution failure and in no lock mismatch.
    if "offline mode (via `--offline`)" in text:
        return True
    if "--offline was specified" in text and "HTTP request" in text:
        return True
    # cargo words the pure cache miss differently in some versions; require the
    # offline mention either way, so a real mismatch is never absorbed here.
    return "--offline" in text and "failed to download" in text


def main():
    leaves = exposed_leaves()
    if not leaves:
        print("submodule-pinned locks: none (no leaf path-deps into a submodule)")
        return 0

    failures = []
    checked = 0
    for leaf, target in leaves:
        rel = leaf.relative_to(REPO)
        # A leaf whose submodule is not initialised cannot be checked, and must
        # not fail: `just setup-launch-resolve` self-gates on the same condition
        # and says so. Silence here would be wrong too, hence the note.
        if not target.exists():
            print(f"  SKIP {rel} — submodule not initialised at {target.relative_to(REPO)}")
            continue
        checked += 1
        proc = subprocess.run(
            ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
            cwd=leaf, capture_output=True, text=True,
        )
        if proc.returncode != 0:
            # issue 0600 — TWO conditions reach this branch and they have
            # different causes and opposite remedies. Cargo distinguishes them
            # for us: an `--offline` download failure is a property of THIS
            # HOST's registry cache and says nothing about the lock, while a
            # genuine mismatch is the moved-pointer case this gate exists for.
            # Reporting both as 0560 told an operator to `lock-update` a
            # byte-correct lock, which is precisely the churn 0359/0378 exist to
            # prevent — in the imperative, to a reader with no reason to doubt.
            kind = "cold-cache" if _is_offline_cache_miss(proc.stderr) else "mismatch"
            failures.append((rel, proc.stderr.strip().splitlines(), kind))
        else:
            print(f"  ok   {rel} resolves under --locked")

    if failures:
        mismatched = [f for f in failures if f[2] == "mismatch"]
        cold = [f for f in failures if f[2] == "cold-cache"]
        print("", file=sys.stderr)

        if cold:
            print(
                f"[UNVERIFIED] {len(cold)} lock(s) name a crate this host has not cached "
                f"— NOT a failure, and NOT a pass:",
                file=sys.stderr,
            )
            for rel, err, _ in cold:
                print(f"\n  {rel}", file=sys.stderr)
                # HEAD, not tail: `no matching package named X` is the first
                # line and names the crate. Printing `err[-4:]` discarded it,
                # which is why the CI red could not be classified from its log.
                for line in err[:6]:
                    print(f"      {line}", file=sys.stderr)
            print(
                "\n  This is a HOST state, not a lock defect — the lock names the crate\n"
                "  correctly and this gate resolves `--offline`. Populate the cache; the\n"
                "  lock is not touched:\n"
                "      (cd <leaf-dir> && cargo fetch --locked)\n"
                "  Do NOT run `lock-update` for this — re-resolving a correct lock is the\n"
                "  churn issues 0359/0378 exist to prevent (issue 0600).",
                file=sys.stderr,
            )
            print(
                "\n  This did NOT verify those leaf/leaves — it is reported, not passed\n"
                "  over. A cold cache is a host state, so failing on it makes the gate\n"
                "  flaky (issue 0863) while telling the operator to edit a correct lock,\n"
                "  which is worse than either outcome alone.",
                file=sys.stderr,
            )

        if mismatched:
            print(
                f"\n[FAIL] {len(mismatched)} lock(s) pinned by a submodule manifest no "
                f"longer resolve:", file=sys.stderr,
            )
            for rel, err, _ in mismatched:
                print(f"\n  {rel}", file=sys.stderr)
                for line in err[:6]:
                    print(f"      {line}", file=sys.stderr)
            print(
                "\n  The submodule pointer moved and the lock did not follow (issue 0560).\n"
                "  Update it the sanctioned way — never a bare `cargo generate-lockfile`:\n"
                "      just lock-update \"\" \"\" <leaf-dir>\n"
                "  then REVIEW the diff: added/removed packages are a dependency change,\n"
                "  which is expected when a pinned tag moves, but should be seen.",
                file=sys.stderr,
            )
        # Only a MISMATCH is a defect in this repo. A cold cache says nothing
        # about the lock, so it cannot be a red — but it is not a pass either,
        # and the line above says so rather than letting it read as coverage.
        if mismatched:
            return 1

    verified = checked - len([f for f in failures if f[2] == "cold-cache"])
    if verified != checked:
        print(
            f"submodule-pinned locks: {verified}/{checked} leaf/leaves verified — "
            f"{checked - verified} could NOT be checked (cold cache, see above)"
        )
        return 0

    print(f"submodule-pinned locks: OK ({checked} leaf/leaves resolve under --locked)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
