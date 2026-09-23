#!/usr/bin/env python3
"""Every CI image installs the shared apt closure, and nothing restates it.

Phase-466. Issues 1359 (no `unzip` -> all 22 nightly `zephyr *` jobs die in
`_setup-common`) and 1364 (no TOML parser -> every repo script that reads a
manifest dies in the image).

WHAT WENT WRONG

`ci/docker/ci-base/Dockerfile` and `ci/docker/zephyr-ros/Dockerfile` are both
`FROM ros:humble-ros-base` and neither inherits from the other. Each carried its
own hand-written `apt-get install` list. The two drifted by exactly two
packages, and each absence emptied a whole lane of its verdict for weeks.

The FIX is not the two packages -- it is that there is now ONE list,
`ci/docker/apt-packages.txt`, which every image COPYs and installs. For anything
named there, drift is not detected, it is unrepresentable.

WHAT THIS GATE ADDS ON TOP OF THAT

Four things one file cannot enforce by itself:

  1. CONSUMPTION. An image that quietly stops COPYing/installing the shared
     list is back where it started, and nothing else would notice until a lane
     went red. Checked over `ci/docker/*/Dockerfile`, by glob -- a THIRD image
     added later is covered without editing this gate, which is the reach gap
     the 2026-07-28 audit found in four gates at once.

  2. NO PRIVATE RESTATEMENT. A package in the shared list must not also appear
     in an image's own apt list. That is how two lists come back: the second
     copy looks harmless, and then somebody edits one of them.

  3. THE SHARED LIST COVERS WHAT `_setup-common` NEEDS, derived from the index
     rather than asserted here. `_setup-common` runs `just setup-clang-format`
     in EVERY image, and `[tool.clang-format] system = [..]` in
     `nros-sdk-index.toml` names the OS prereq keys that tool needs;
     `[prereq.<key>].apt` names the packages. This is the rule that, had it
     existed, would have refused issue 1359 at authoring time.

  4. THE SHARED LIST CARRIES A TOML PARSER, derived from the TREE. The repo's
     scripts open `import tomllib` with an `import tomli` fallback; the images
     are jammy / Python 3.10, where `tomllib` does not exist. So as long as any
     script uses that chain, the images owe it `python3-tomli`. Issue 1364.

  5. THE PUBLISHED TAG AND ITS CONSUMERS AGREE. `images.yml` declares each
     image's `TAG`; four `container:` lines across the workflows spell it out
     by hand. Four hand-copied literals of one fact is the same defect shape one
     level up, and bumping the tag without moving them deadlocks every lane on
     `manifest unknown`.

WHY REGEX AND NOT A TOML PARSER

Because of 4. This gate must run on a host that has no TOML parser -- that is
the state it exists to describe, and asking a parser to prove a parser is
missing is a bootstrap loop. `scripts/dev/clang-format.sh` reads the same file
with `sed` for the same reason: the index stays the SSoT, this reads it.

Usage::

    check-ci-image-apt-packages.py [--self-test] [--list-shared]
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

SHARED = os.path.join("ci", "docker", "apt-packages.txt")
IMAGE_DIR = os.path.join("ci", "docker")
INDEX = "nros-sdk-index.toml"
WORKFLOWS = os.path.join(".github", "workflows")
IMAGES_YML = os.path.join(WORKFLOWS, "images.yml")

# The tool `_setup-common` provisions in EVERY image, whatever the scope. Its
# OS prereqs are what the shared list must carry; the names come from the index.
SETUP_COMMON_TOOLS = ("clang-format",)

# The apt package that satisfies the `import tomli` arm of the repo's TOML
# chain on a Debian/Ubuntu base. Named here because Debian's spelling is not
# derivable from the module name; the REQUIREMENT is derived (see `toml_sites`).
TOML_PARSER_PACKAGE = "python3-tomli"

# Not packages: apt's own words, and the shell around them.
_NOT_A_PACKAGE = {"apt-get", "install", "update", "sed", "xargs", "rm", "true"}


# --------------------------------------------------------------------------
# reading


def read(root, rel):
    with open(os.path.join(root, rel), "r", encoding="utf-8") as fh:
        return fh.read()


def shared_packages(text):
    """The shared list, in file order. `#` comments, blank lines ignored."""
    out = []
    for line in text.splitlines():
        line = line.split("#", 1)[0].strip()
        if line:
            out.append(line)
    return out


def logical_lines(dockerfile_text):
    """Dockerfile instructions with `\\` continuations joined.

    The backtick-comment idiom (`` `# ...` ``) the Dockerfiles use to annotate
    individual packages is stripped, so a word inside a comment is never read as
    a package name.
    """
    text = re.sub(r"`#[^`]*`", " ", dockerfile_text)
    joined = re.sub(r"\\\s*\n", " ", text)
    out = []
    for line in joined.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        out.append(re.sub(r"\s+", " ", line))
    return out


def private_apt_packages(dockerfile_text):
    """Package names an image asks for in its OWN `apt-get install` lines.

    A line that installs FROM the shared list is not one of these -- it is the
    shared install, and its names live in the file.
    """
    out = []
    for line in logical_lines(dockerfile_text):
        if "apt-get install" not in line:
            continue
        if "apt-packages.txt" in line:
            continue
        tail = line.split("apt-get install", 1)[1]
        # Stop at the next shell command in the same RUN.
        tail = re.split(r"&&|\|\||;|\|", tail)[0]
        for tok in tail.split():
            if tok.startswith("-") or tok in _NOT_A_PACKAGE:
                continue
            if re.fullmatch(r"[a-z0-9][a-z0-9+.:-]*", tok):
                out.append(tok)
    return out


def consumes_shared(dockerfile_text):
    """(copies, installs) -- does this image COPY the shared list and use it?"""
    lines = logical_lines(dockerfile_text)
    copies = any(
        line.startswith("COPY") and "ci/docker/apt-packages.txt" in line
        for line in lines
    )
    installs = any(
        line.startswith("RUN")
        and "apt-packages.txt" in line
        and "apt-get install" in line
        for line in lines
    )
    return copies, installs


def image_dockerfiles(root):
    """`ci/docker/*/Dockerfile`, by glob -- a new image is covered for free."""
    base = os.path.join(root, IMAGE_DIR)
    out = []
    for name in sorted(os.listdir(base)):
        path = os.path.join(base, name, "Dockerfile")
        if os.path.isfile(path):
            out.append((name, os.path.join(IMAGE_DIR, name, "Dockerfile")))
    return out


# --------------------------------------------------------------------------
# the index (regex -- see the module docstring on why)


def _section(index_text, header):
    """The body of one `[header]` table, up to the next top-level table."""
    pat = re.compile(
        r"^\[" + re.escape(header) + r"\]\s*$(.*?)(?=^\[|\Z)", re.M | re.S
    )
    m = pat.search(index_text)
    return m.group(1) if m else None


def _string_list(body, key):
    m = re.search(r"^%s\s*=\s*\[([^\]]*)\]" % re.escape(key), body or "", re.M)
    if not m:
        return []
    return re.findall(r'"([^"]+)"', m.group(1))


def index_apt_packages_for_tool(index_text, tool):
    """apt packages `[tool.<tool>] system = [..]` demands, via `[prereq.*]`.

    Raises if the tool or one of its prereq keys is missing -- a silent empty
    answer would make this gate pass by knowing nothing, which is the shape the
    `prereq-packages.py` header refuses for the same reason.
    """
    body = _section(index_text, "tool." + tool)
    if body is None:
        raise KeyError("no [tool.%s] in %s" % (tool, INDEX))
    out = []
    for key in _string_list(body, "system"):
        prereq = _section(index_text, "prereq." + key)
        if prereq is None:
            raise KeyError(
                "[tool.%s] names system prereq %r, but there is no [prereq.%s]"
                % (tool, key, key)
            )
        pkgs = _string_list(prereq, "apt")
        if not pkgs:
            raise KeyError("[prereq.%s] declares no `apt` packages" % key)
        out.extend(pkgs)
    return out


# --------------------------------------------------------------------------
# the tree


def toml_sites(root):
    """Repo scripts that open with the `tomllib` -> `tomli` import chain."""
    hits = []
    skip = {".git", "third-party", "target", "build", "node_modules"}
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in skip]
        for fn in filenames:
            if not fn.endswith(".py"):
                continue
            path = os.path.join(dirpath, fn)
            try:
                with open(path, "r", encoding="utf-8", errors="replace") as fh:
                    text = fh.read()
            except OSError:
                continue
            if re.search(r"^\s*import\s+tomli\b", text, re.M):
                hits.append(os.path.relpath(path, root))
    return sorted(hits)


# --------------------------------------------------------------------------
# tags


def declared_tags(images_yml_text):
    """{image ref: tag} as `images.yml` declares them, per job `env:` block."""
    out = {}
    image = None
    for line in images_yml_text.splitlines():
        m = re.match(r"\s*IMAGE:\s*(\S+)\s*$", line)
        if m:
            image = m.group(1)
            continue
        m = re.match(r"\s*TAG:\s*(\S+)\s*$", line)
        if m and image:
            out[image] = m.group(1)
            image = None
    return out


def tag_references(root, tags):
    """[(workflow, image, tag)] for every `<image>:<tag>` in the workflows.

    `images.yml`'s own `${{ env.* }}` lines are not references -- they are the
    declaration.
    """
    out = []
    wf_dir = os.path.join(root, WORKFLOWS)
    for fn in sorted(os.listdir(wf_dir)):
        if not fn.endswith((".yml", ".yaml")):
            continue
        rel = os.path.join(WORKFLOWS, fn)
        text = read(root, rel)
        for image in tags:
            for m in re.finditer(re.escape(image) + r":([A-Za-z0-9._-]+)", text):
                tag = m.group(1)
                if "{" in tag:
                    continue
                out.append((rel, image, tag))
    return out


# --------------------------------------------------------------------------
# the check


def check(root):
    """[] when the tree holds; a list of human-readable failures otherwise."""
    failures = []

    shared_text = read(root, SHARED)
    shared = shared_packages(shared_text)
    shared_set = set(shared)
    if not shared:
        failures.append(
            "%s is empty -- a gate that reports OK over nothing is the defect,"
            " not the fix." % SHARED
        )

    images = image_dockerfiles(root)
    if not images:
        failures.append("no ci/docker/*/Dockerfile found -- refusing to pass.")

    for name, rel in images:
        text = read(root, rel)
        copies, installs = consumes_shared(text)
        if not copies:
            failures.append(
                "%s does not `COPY ci/docker/apt-packages.txt`.\n"
                "  Every CI image installs the shared apt closure; an image that"
                " opts out is a second\n"
                "  hand-written list, which is issues 1359 and 1364." % rel
            )
        if not installs:
            failures.append(
                "%s COPYs apt-packages.txt but no RUN line installs from it.\n"
                "  Expected a `RUN ... apt-packages.txt | xargs apt-get install"
                " ...` line." % rel
            )
        restated = sorted(set(private_apt_packages(text)) & shared_set)
        if restated:
            failures.append(
                "%s restates shared package(s) in its own apt list: %s\n"
                "  Remove them from the Dockerfile. They are already installed"
                " from %s, and a\n"
                "  second copy is how two lists came back last time."
                % (rel, ", ".join(restated), SHARED)
            )

    index_text = read(root, INDEX)
    for tool in SETUP_COMMON_TOOLS:
        try:
            needed = index_apt_packages_for_tool(index_text, tool)
        except KeyError as exc:
            failures.append("%s: %s" % (INDEX, exc))
            continue
        missing = [p for p in needed if p not in shared_set]
        if missing:
            failures.append(
                "%s is missing %s, which `[tool.%s] system = [..]` demands.\n"
                "  `_setup-common` runs `just setup-%s` in EVERY image, whatever"
                " the scope, so\n"
                "  every image needs it. Without it that recipe fails and takes"
                " the whole setup\n"
                "  step with it -- issue 1359, 22 nightly jobs."
                % (SHARED, ", ".join(missing), tool, tool)
            )

    sites = toml_sites(root)
    if sites and TOML_PARSER_PACKAGE not in shared_set:
        failures.append(
            "%s is missing %s, and %d script(s) open with the\n"
            "  `import tomllib` -> `import tomli` chain (e.g. %s).\n"
            "  The CI images are Ubuntu jammy / Python 3.10, which has no"
            " `tomllib`, so without\n"
            "  the fallback package none of those scripts can read their own"
            " manifests in the\n"
            "  image -- issue 1364."
            % (SHARED, TOML_PARSER_PACKAGE, len(sites), sites[0])
        )

    tags = declared_tags(read(root, IMAGES_YML))
    if not tags:
        failures.append("%s declares no IMAGE/TAG pairs -- refusing to pass." % IMAGES_YML)
    for rel, image, tag in tag_references(root, tags):
        if tag != tags[image]:
            failures.append(
                "%s references %s:%s, but %s publishes :%s.\n"
                "  A consumer left behind pins an image the workflow no longer"
                " builds; a consumer\n"
                "  moved ahead deadlocks the lane on `manifest unknown`. Move"
                " them together."
                % (rel, image, tag, IMAGES_YML, tags[image])
            )

    return failures


# --------------------------------------------------------------------------
# self-test


def _write(root, rel, text):
    path = os.path.join(root, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(text)


_FAKE_INDEX = """
[tool.clang-format]
version = "17.0.6-nros1"
system = ["unzip"]

[prereq.unzip]
role = "workspace"
apt = ["unzip"]

[prereq.other]
apt = ["other"]
"""

_FAKE_IMAGES_YML = """
jobs:
  a:
    env:
      IMAGE: ghcr.io/x/img-a
      TAG: t1
  b:
    env:
      IMAGE: ghcr.io/x/img-b
      TAG: t2
    steps:
      - run: echo ${{ env.IMAGE }}:${{ env.TAG }}
"""

_FAKE_CONSUMER_YML = """
jobs:
  j:
    container:
      image: ghcr.io/x/img-a:t1
    steps:
      - run: true
"""


def _fake_dockerfile(extra=""):
    return (
        "FROM base\n"
        "COPY ci/docker/apt-packages.txt /opt/nros-ssot/ci/docker/apt-packages.txt\n"
        "RUN apt-get update \\\n"
        "    && sed -e 's/#.*//' /opt/nros-ssot/ci/docker/apt-packages.txt \\\n"
        "       | xargs -r apt-get install -y --no-install-recommends \\\n"
        "    && rm -rf /var/lib/apt/lists/*\n"
        + extra
    )


_OWN_BLOCK = (
    "RUN apt-get update && apt-get install -y --no-install-recommends \\\n"
    "        aria2 \\\n"
    "        `# ^ unzip python3-tomli named only in a comment` \\\n"
    "    && rm -rf /var/lib/apt/lists/*\n"
)


def _fake_tree(tmp):
    _write(tmp, SHARED, "# shared\nunzip\npython3-tomli\ncmake\n")
    _write(tmp, os.path.join(IMAGE_DIR, "img-a", "Dockerfile"), _fake_dockerfile())
    _write(
        tmp,
        os.path.join(IMAGE_DIR, "img-b", "Dockerfile"),
        _fake_dockerfile(_OWN_BLOCK),
    )
    _write(tmp, INDEX, _FAKE_INDEX)
    _write(tmp, IMAGES_YML, _FAKE_IMAGES_YML)
    _write(tmp, os.path.join(WORKFLOWS, "consumer.yml"), _FAKE_CONSUMER_YML)
    _write(tmp, "uses_toml.py", "import tomllib\nimport tomli as tomllib\n")


def self_test(quiet=False):
    import shutil
    import tempfile

    failures = 0

    def case(why, mutate, want_fail):
        nonlocal failures
        tmp = tempfile.mkdtemp(prefix="nros-ci-apt-")
        try:
            _fake_tree(tmp)
            mutate(tmp)
            got = check(tmp)
            if want_fail and not got:
                print("  FAIL: %s -- expected a failure, got none" % why)
                failures += 1
            elif not want_fail and got:
                print("  FAIL: %s -- expected OK, got:\n    %s" % (why, got[0]))
                failures += 1
        finally:
            shutil.rmtree(tmp, ignore_errors=True)

    # The positive control: an un-drifted tree passes. A gate that can never
    # pass and a gate that can never fail are the same non-gate.
    case("a correct tree", lambda t: None, False)

    # MUTATION 1 -- issue 1359. Drop the package the index demands.
    case(
        "shared list loses `unzip`",
        lambda t: _write(t, SHARED, "python3-tomli\ncmake\n"),
        True,
    )

    # MUTATION 2 -- issue 1364. Drop the TOML parser while scripts still need it.
    case(
        "shared list loses the TOML parser",
        lambda t: _write(t, SHARED, "unzip\ncmake\n"),
        True,
    )
    # ...and it is a DERIVED requirement, not a constant: with no script using
    # the chain, the package is not owed.
    def _no_toml_sites(t):
        _write(t, SHARED, "unzip\ncmake\n")
        os.remove(os.path.join(t, "uses_toml.py"))

    case("no script imports tomli", _no_toml_sites, False)

    # MUTATION 3 -- the two-list shape coming back: an image restates a shared
    # package privately. This is the one that makes drift impossible rather
    # than merely absent.
    case(
        "an image restates `unzip` in its own apt list",
        lambda t: _write(
            t,
            os.path.join(IMAGE_DIR, "img-b", "Dockerfile"),
            _fake_dockerfile(
                "RUN apt-get install -y --no-install-recommends aria2 unzip\n"
            ),
        ),
        True,
    )

    # MUTATION 4 -- an image opts out of the shared list entirely.
    case(
        "an image stops COPYing the shared list",
        lambda t: _write(
            t,
            os.path.join(IMAGE_DIR, "img-b", "Dockerfile"),
            "FROM base\nRUN apt-get install -y cmake\n",
        ),
        True,
    )

    # MUTATION 5 -- a NEW image that never consumed the list. The glob is what
    # gives this gate a reach as wide as its rule.
    case(
        "a third image is added that ignores the shared list",
        lambda t: _write(
            t,
            os.path.join(IMAGE_DIR, "img-c", "Dockerfile"),
            "FROM base\nRUN apt-get install -y cmake\n",
        ),
        True,
    )

    # MUTATION 6 -- the tag is bumped and a consumer is left behind.
    case(
        "images.yml bumps a TAG without moving its consumer",
        lambda t: _write(t, IMAGES_YML, _FAKE_IMAGES_YML.replace("TAG: t1", "TAG: t9")),
        True,
    )

    # MUTATION 7 -- the index names a prereq key that does not exist. A silent
    # empty answer here would make rule 3 pass by knowing nothing.
    case(
        "the index names a prereq key that does not exist",
        lambda t: _write(t, INDEX, _FAKE_INDEX.replace('system = ["unzip"]', 'system = ["nope"]')),
        True,
    )

    # A package named only inside a backtick comment is NOT a restatement --
    # the Dockerfiles annotate individual packages that way, and reading a
    # comment as code would make this gate unusable on the real tree.
    case("a shared name appears only in a backtick comment", lambda t: None, False)

    if failures:
        print("check-ci-image-apt-packages self-test: %d case(s) FAILED" % failures)
        return 1
    if not quiet:
        print("check-ci-image-apt-packages self-test: OK (9 cases)")
    return 0


def main(argv):
    if "--self-test" in argv:
        return self_test()
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment, and every one of these cases is a defect that reached
    # `main` once already.
    if self_test(quiet=True) != 0:
        return 1
    if "--list-shared" in argv:
        print("\n".join(shared_packages(read(ROOT, SHARED))))
        return 0
    failures = check(ROOT)
    if failures:
        print("check-ci-image-apt-packages: FAILED\n")
        for f in failures:
            print("  " + f.replace("\n", "\n  "))
            print()
        print(
            "The shared apt closure is %s. Read its header before editing:\n"
            "  a package the REPO needs on any host belongs there; a package one"
            " image alone\n"
            "  needs belongs in that image's Dockerfile." % SHARED
        )
        return 1
    shared = shared_packages(read(ROOT, SHARED))
    images = [n for n, _ in image_dockerfiles(ROOT)]
    print(
        "check-ci-image-apt-packages: OK (%d shared package(s), %d image(s): %s)"
        % (len(shared), len(images), ", ".join(images))
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
