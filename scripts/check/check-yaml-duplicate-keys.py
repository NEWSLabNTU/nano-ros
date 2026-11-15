#!/usr/bin/env python3
"""Every tracked YAML file parses with NO duplicate mapping key.

# The defect it was written for

On 2026-11-15 two pull requests each added a `permissions:` block to
`gate.yml`'s `changes` job (phase-489 W11 and W12). The rebase of the second
merged textually, so the job held two `permissions:` keys. GitHub REFUSES such
a workflow: the push run failed with zero jobs ("This run likely failed because
of a workflow file issue"), and the pull request showed no gate checks at all.
Every local check passed. PyYAML's `safe_load`, which every script here uses,
keeps the LAST duplicate silently, so a workflow GitHub will not run reads as
valid locally.

# The rule

A tracked `*.yml` / `*.yaml` file parses, and no mapping in it names a key
twice. That is the syntactic floor GitHub's parser enforces, and it is the
sibling of `check-manifests-parse.py` (issue 0722, a duplicate key in a
`Cargo.toml`). The scope is every tracked YAML file, not only
`.github/workflows`, because the loader is equally blind for every one of them.

Run: python3 scripts/check/check-yaml-duplicate-keys.py [--self-test]
"""

import subprocess
import sys

import yaml


class UniqueKeyLoader(yaml.SafeLoader):
    pass


def _construct_mapping(loader, node, deep=False):
    seen = {}
    for key_node, _ in node.value:
        if key_node.tag == "tag:yaml.org,2002:merge":
            continue
        key = loader.construct_object(key_node, deep=deep)
        try:
            hash(key)
        except TypeError:
            continue
        if key in seen:
            raise yaml.constructor.ConstructorError(
                "while constructing a mapping",
                node.start_mark,
                f"duplicate key {key!r} (first at line {seen[key] + 1})",
                key_node.start_mark,
            )
        seen[key] = key_node.start_mark.line
    return loader.construct_mapping(node, deep)


UniqueKeyLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _construct_mapping
)


def problem(text: str):
    try:
        for _ in yaml.load_all(text, Loader=UniqueKeyLoader):
            pass
    except yaml.YAMLError as exc:
        return str(exc).replace("\n", " ")
    return None


def self_test() -> int:
    cases = [
        ("a job with two `permissions:` keys", "jobs:\n  c:\n    permissions: {}\n    runs-on: x\n    permissions: {}\n", True),
        ("the same key in two different jobs", "jobs:\n  a:\n    permissions: {}\n  b:\n    permissions: {}\n", False),
        ("a merge key next to an override", "base: &b {x: 1}\nuse:\n  <<: *b\n  x: 2\n", False),
        ("two documents with one key each", "a: 1\n---\na: 2\n", False),
    ]
    bad = 0
    for name, text, want in cases:
        got = problem(text) is not None
        print(f"  {'ok  ' if got == want else 'FAIL'} {name}: {'refused' if got else 'accepted'}")
        bad += got != want
    if bad:
        print("yaml-duplicate-keys self-test: FAILED", file=sys.stderr)
        return 1
    return 0


def main(argv) -> int:
    if argv == ["--self-test"]:
        return self_test()
    if argv:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    if self_test():
        return 1
    listed = subprocess.run(
        ["git", "ls-files", "-z", "*.yml", "*.yaml"], capture_output=True, check=True
    ).stdout
    paths = [p for p in listed.decode().split("\0") if p]
    bad = []
    for path in paths:
        try:
            with open(path, encoding="utf-8") as fh:
                why = problem(fh.read())
        except FileNotFoundError:
            continue
        if why:
            bad.append((path, why))
    if bad:
        print("tracked YAML files a strict parser refuses:", file=sys.stderr)
        for path, why in bad:
            print(f"  {path}: {why}", file=sys.stderr)
        print(
            "\nPyYAML keeps the last duplicate key silently; GitHub refuses a workflow\n"
            "that has one, and the run fails with no jobs. Delete the extra key.",
            file=sys.stderr,
        )
        return 1
    print(f"yaml-duplicate-keys: OK ({len(paths)} files)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
