#!/usr/bin/env python3
"""Every file a CI image COPYs from the tree must be an INPUT of that image.

Issues 1201, 1760, 1868. A CI Dockerfile built with `context: .` may `COPY` a
repository file (the shared apt list, the Rust target and toolchain lists, the
`just` installer). Each such file has to be declared in FOUR places, and each
omission has its own silent failure:

  1. `<Dockerfile>.dockerignore` must un-ignore it (`!path`). Otherwise the
     build fails in under a minute at "failed to calculate checksum of ref: not
     found" — and nothing consumes that failure: the floating tag keeps serving
     the previous content (issue 1201, for which the image went eight weeks
     without the `clang` a gate needed).
  2. `images.yml`'s `on.push.paths` must list it, or an edit to it never
     triggers a rebuild on merge.
  3. The `changes` job's filter for THAT image must list it, or the rebuild is
     triggered and then skipped.
  4. The image's "Derive the content tag" step must hash it, or a rebuild that
     changes what is baked publishes under the same content tag — "same tag,
     different toolchain", the defect that tag exists to end.

Issue 1868 added three such files to ci-base at once (and 1760 one to both
images); this gate is what keeps the next one from needing a reader to
remember all four.

Images and their Dockerfiles are DERIVED from `images.yml` (a job's `IMAGE:`
env against its `docker/build-push-action` `file:`), never listed here.

Run:  python3 scripts/check-ci-image-inputs.py
"""

import fnmatch
import os
import re
import sys

import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
IMAGES_YML = os.path.join(".github", "workflows", "images.yml")


def copy_sources(dockerfile_text):
    """Repository paths a Dockerfile COPYs (not `COPY --from=` stage copies)."""
    out = []
    for line in dockerfile_text.splitlines():
        m = re.match(r"\s*COPY\s+(.*)$", line)
        if not m:
            continue
        words = [w for w in m.group(1).split() if not w.startswith("--")]
        if "--from" in m.group(1) or len(words) < 2:
            continue
        out.extend(words[:-1])
    return out


def _covered(path, patterns):
    for pat in patterns:
        if pat == path or fnmatch.fnmatch(path, pat):
            return True
        if pat.endswith("/**") and path.startswith(pat[:-2]):
            return True
    return False


def dockerignore_allows(path, ignore_text):
    allowed = [l[1:].strip() for l in ignore_text.splitlines() if l.startswith("!")]
    for a in allowed:
        if a == path or (a.endswith("/") and path.startswith(a)):
            return True
    return False


def audit(read):
    """Failure strings. `read(relpath)` returns a file's text or None."""
    doc = yaml.safe_load(read(IMAGES_YML))
    on = doc.get("on") or doc.get(True) or {}
    push_paths = ((on.get("push") or {}).get("paths")) or []
    jobs = doc.get("jobs") or {}

    filters = {}
    for step in (jobs.get("changes") or {}).get("steps") or []:
        if "paths-filter" in (step.get("uses") or ""):
            filters = yaml.safe_load((step.get("with") or {}).get("filters") or "") or {}

    failures, images = [], 0
    for name, job in jobs.items():
        image = (job.get("env") or {}).get("IMAGE")
        steps = job.get("steps") or []
        build = [s for s in steps if "build-push-action" in (s.get("uses") or "")]
        if not image or not build:
            continue
        images += 1
        dockerfile = (build[0].get("with") or {}).get("file")
        df_text = read(dockerfile) if dockerfile else None
        if df_text is None:
            failures.append("%s: job %r builds %r, which does not exist" % (IMAGES_YML, name, dockerfile))
            continue
        ignore = read(dockerfile + ".dockerignore") or ""
        m = re.search(r"outputs\.([a-z_]+)", str(job.get("if") or ""))
        key = m.group(1) if m else None
        image_filter = filters.get(key) or []
        tag_step = next((s.get("run") or "" for s in steps
                         if "content tag" in (s.get("name") or "").lower()), "")
        for src in copy_sources(df_text):
            where = "%s COPYs %s" % (dockerfile, src)
            if not dockerignore_allows(src, ignore):
                failures.append("%s, but %s.dockerignore does not un-ignore it (`!%s`)"
                                % (where, dockerfile, src))
            if not _covered(src, push_paths):
                failures.append("%s, but images.yml `on.push.paths` does not list it — "
                                "an edit to it never rebuilds the image" % where)
            if not _covered(src, image_filter):
                failures.append("%s, but the `changes` filter %r does not list it — "
                                "the rebuild is triggered and then skipped" % (where, key))
            if src not in tag_step:
                failures.append("%s, but job %r's content-tag step does not hash it — "
                                "new content would publish under the old tag" % (where, name))
    if images == 0:
        failures.append("%s: found no image-building job — the gate examined nothing" % IMAGES_YML)
    return failures


def disk_reader(rel):
    p = os.path.join(ROOT, rel)
    return open(p).read() if os.path.isfile(p) else None


def self_test():
    """Each of the four declarations, removed from the REAL tree, must go red."""
    base = audit(disk_reader)
    if base:
        return 0  # the real verdict names it; a mutation test over a red tree says nothing
    src = "config/rust-toolchains.txt"
    images_yml = disk_reader(IMAGES_YML)
    df = "ci/docker/ci-base/Dockerfile"
    mutations = {
        "dockerignore": {df + ".dockerignore": disk_reader(df + ".dockerignore").replace("!" + src + "\n", "")},
        "push paths": {IMAGES_YML: images_yml.replace('      - "%s"\n' % src, "", 1)},
        "changes filter": {IMAGES_YML: images_yml.replace("              - '%s'\n" % src, "", 1)},
        "content tag": {IMAGES_YML: images_yml.replace("config/rust-toolchains.txt scripts", "scripts", 1)},
    }
    bad = 0
    for what, overlay in mutations.items():
        for rel, text in overlay.items():
            if text == disk_reader(rel):
                print("  self-test FAIL: mutation %r changed nothing — its anchor moved" % what)
                bad += 1
        got = audit(lambda rel, o=overlay: o[rel] if rel in o else disk_reader(rel))
        if not any(src in f for f in got):
            print("  self-test FAIL: dropping %s from the %s did not go red" % (src, what))
            bad += 1
    return bad


def main():
    bad = self_test()
    failures = audit(disk_reader)
    for f in failures:
        print("check-ci-image-inputs: " + f)
    if failures or bad:
        return 1
    print("check-ci-image-inputs: OK — every COPY source is ignored-in, filtered, triggered and hashed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
