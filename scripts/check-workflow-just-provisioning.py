#!/usr/bin/env python3
"""A job that runs a `just` recipe must OBTAIN `just`. Phase-466 W2.

## The failure this exists for

`probe.yml`'s first scheduled run, 2026-09-22, died in both jobs inside a
second:

    /home/runner/work/_temp/....sh: line 2: just: command not found
    ##[error]Process completed with exit code 127.

The step was `source ./activate.sh` then `just probe checkout`. `probe` is the
one lane that CANNOT run inside `ghcr.io/newslabntu/nano-ros-ci` — it drives
`docker run` itself — so it is on a bare `ubuntu-latest`, where `just` is not
installed. Every other lane gets `just` from that image and nobody had to think
about it, which is exactly why the one lane that had to think about it did not.

## Why a gate, and why a STATIC one

The repo's idiom for a runtime precondition is `just doctor`. It cannot help
here, and no successor to it can: **`just doctor` needs `just`.** The bootstrap
tool is the one prerequisite the repo's own provisioning cannot assert, because
every assertion is written as a `just` recipe. `just setup` cannot install
`just`; `runner-doctor.sh` runs under `bash` and checks west, qemu,
`arm-none-eabi-gcc`, rustup and zenohd — not the tool whose absence stops every
step after it.

`activate.sh` does say so, through `scripts/sdk-env.sh`:

    nano-ros sdk-env: `just` not found — RTOS SDK path defaults … not loaded.
      Harmless for the native/host flow. Needed for embedded builds and every
      `just` recipe: cargo install just

That warning is CORRECT and was printed one line above the fatal error, and it
was ignored, because it calls itself harmless — which is the right thing for a
SOURCED file to say (it cannot know whether a `just` recipe follows, and
`check-activate-shells` requires it to reach its last line rather than abort a
developer's interactive shell). A diagnostic that must hedge is not where this
class gets caught. It gets caught here, before the lane ships.

The sibling `check-workflow-repo-env` (issue 0933) asks whether a step SOURCES
the environment. `probe.yml` did source it and still died: sourcing the
environment and having the tool are two facts, and the gate for the first one
reads green on a job that fails the second. That is issue 0196's shape one gate
over.

## What counts as providing `just`

Three arms, and they do NOT carry the same evidence — the gate reports which
one answered, for the reason issue 1043 records about FAIL / NOT VERIFIED / OK:

  * **container** — CHECKED. The image's Dockerfile is read, and the mapping
    from image name to Dockerfile is DERIVED from `images.yml` (its `IMAGE:`
    env against its `docker/build-push-action` `file:`/`context:`), never
    authored here, so a new image cannot drift the map toward OK.

  * **in-job install step** — CHECKED. A step in the same job (or in a local
    composite action the job `uses:`) that runs the canonical installer, before
    the first `just` invocation. `.github/actions/setup-qemu-patched` had this
    step long before `probe.yml` needed it.

  * **self-hosted** — NOT VERIFIABLE FROM THE TREE. The fleet's `just` lives in
    the runner's `~/.local/bin` persistent volume (`runner-container.sh` puts it
    on PATH), and `runner-provision.sh` cannot have run without it, since its
    plan is `just setup base`. Nothing in the repository proves a given machine
    provisioned, so the gate names these rather than counting them as checked.

## Reach: composite actions, not just workflows

`.github/actions/setup-nros-cli/action.yml` runs `just setup-launch-resolve`
and installs nothing. Its four consumers are all in containers today, so it is
latent — but a fifth consumer on a bare runner reproduces `probe.yml` exactly,
and the action's own file gives a reader no hint. So a `uses: ./.github/...`
step is EXPANDED in place: the action's invocations count as the job's, and the
action's install step counts as a provider for what follows it.

Ordering is checked, for the reason `check-workflow-doctor-after-setup` checks
it: an install step after the first invocation provisions nothing.

Run:  python3 scripts/check-workflow-just-provisioning.py [--self-test]
"""

import argparse
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))

from workflow_commands import REPO, command_lines, invoke_re, load_workflows  # noqa: E402

JUST = invoke_re(("just",))

# How `just` gets onto a machine. Both Dockerfiles and
# `.github/actions/setup-qemu-patched` use the first; the second is what
# `scripts/sdk-env.sh` tells a human to type. A third-party action whose name
# ends in `setup-just` is the community convention.
INSTALLERS = ("just.systems/install.sh", "cargo install just")
INSTALL_ACTION = re.compile(r"setup-just(?:@|$)")


def image_dockerfiles():
    """image name (no tag) -> Dockerfile path, DERIVED from `images.yml`.

    Authoring this map would make it the thing that goes stale: a new image
    absent from the table reads as "not provided" rather than as "unknown",
    which is the safe direction, but a RENAMED one would read as provided
    forever. Reading the workflow that builds them cannot drift.
    """
    import yaml

    doc = yaml.safe_load((REPO / ".github" / "workflows" / "images.yml").read_text())
    out = {}
    for job in (doc.get("jobs") or {}).values():
        image = (job.get("env") or {}).get("IMAGE")
        if not image:
            continue
        for step in job.get("steps") or []:
            if "build-push-action" not in (step.get("uses") or ""):
                continue
            with_ = step.get("with") or {}
            df = with_.get("file")
            if not df:
                df = str(Path(with_.get("context", ".")) / "Dockerfile")
            out[image] = df
    return out


def image_provides_just(image, dockerfiles):
    """(verdict, detail) for a `container.image`.

    `True` only when the Dockerfile that builds it is in the tree AND installs
    `just`. An image we do not build is `None` — unknown, not fine.
    """
    name = image.split("@")[0].rsplit(":", 1)[0]
    df = dockerfiles.get(name)
    if df is None:
        return None, f"{image} is not built by images.yml — cannot verify"
    path = REPO / df
    if not path.is_file():
        return None, f"{df} (named by images.yml) is missing — cannot verify"
    text = path.read_text()
    if any(marker in text for marker in INSTALLERS):
        return True, df
    return False, f"{df} does not install just"


def is_self_hosted(runs_on):
    if isinstance(runs_on, str):
        return runs_on == "self-hosted"
    if isinstance(runs_on, (list, tuple)):
        return "self-hosted" in runs_on
    if isinstance(runs_on, dict):  # runs-on: {group:, labels:}
        return is_self_hosted(runs_on.get("labels")) or bool(runs_on.get("group"))
    return False


def classify_step(step):
    """(installs_just, invoking_lines) for one step of a job or an action."""
    uses = step.get("uses") or ""
    if uses and INSTALL_ACTION.search(uses):
        return True, []
    run = step.get("run") or ""
    if not run:
        return False, []
    lines = command_lines(run)
    installs = any(marker in line for line in lines for marker in INSTALLERS)
    return installs, [l.strip() for l in lines if JUST.search(l)]


def local_action_steps(uses, cache):
    """The steps of a local composite action, or `[]`.

    A `uses: ./.github/actions/<x>` step is expanded in place so the action's
    `just` invocations and its installer are attributed to the job that reaches
    them — the `setup-nros-cli` case in this file's header.
    """
    import yaml

    if not uses.startswith("./"):
        return []
    if uses in cache:
        return cache[uses]
    base = REPO / uses[2:]
    steps = []
    for candidate in (base / "action.yml", base / "action.yaml"):
        if candidate.is_file():
            doc = yaml.safe_load(candidate.read_text()) or {}
            steps = ((doc.get("runs") or {}).get("steps") or [])
            break
    cache[uses] = steps
    return steps


def audit(docs):
    """(failures, self_hosted_jobs, checked_jobs)."""
    dockerfiles = image_dockerfiles()
    cache = {}
    failures, self_hosted, checked = [], [], []

    for path, doc in docs:
        for job_name, job in (doc.get("jobs") or {}).items():
            # Flatten the job's steps, expanding local composite actions.
            flat = []
            for step in job.get("steps") or []:
                uses = step.get("uses") or ""
                inner = local_action_steps(uses, cache)
                if inner:
                    for s in inner:
                        flat.append((f"{step.get('name') or uses} -> {uses}", s))
                else:
                    flat.append((step.get("name") or uses or "(unnamed)", step))

            invocations = []
            installed_before = False
            first_install = None
            for label, step in flat:
                installs, hits = classify_step(step)
                for hit in hits:
                    invocations.append((label, hit, installed_before))
                if installs and not installed_before:
                    installed_before = True
                    first_install = label

            if not invocations:
                continue

            container = job.get("container") or {}
            image = container.get("image") if isinstance(container, dict) else container

            if image:
                verdict, detail = image_provides_just(image, dockerfiles)
                if verdict is True:
                    checked.append((path, job_name, f"container: {detail}"))
                else:
                    failures.append(
                        (path, job_name, invocations[0][0], invocations[0][1], detail)
                    )
                continue

            if is_self_hosted(job.get("runs-on")):
                self_hosted.append((path, job_name))
                continue

            late = [v for v in invocations if not v[2]]
            if not late:
                checked.append((path, job_name, f"install step: {first_install}"))
                continue

            if first_install is not None:
                why = (
                    f"the install step ({first_install!r}) runs AFTER this "
                    "invocation — it provisions nothing for it"
                )
            else:
                why = (
                    f"runs-on: {job.get('runs-on')!r} with no container and no "
                    "step that installs just"
                )
            failures.append((path, job_name, late[0][0], late[0][1], why))

    return failures, self_hosted, checked


def report(failures, self_hosted, checked, quiet=False):
    if failures:
        print("check-workflow-just-provisioning: job(s) that run `just` without obtaining it:")
        for path, job, step, line, why in failures:
            print(f"  {path}  [{job}]  step {step!r}")
            print(f"      runs: {line[:88]}")
            print(f"      {why}")
        print()
        print("  A bare GitHub-hosted runner has no `just`, and nothing in this repo")
        print("  can tell you so at run time: every precondition we own is a `just`")
        print("  recipe, and `just doctor` needs `just`. Give the job one of:")
        print("    * container: ghcr.io/newslabntu/nano-ros-ci:humble  (bakes it), or")
        print("    * a step BEFORE the first invocation, copying the one in")
        print("      .github/actions/setup-qemu-patched: `command -v just` guard,")
        print("      https://just.systems/install.sh --to \"$HOME/.local/bin\",")
        print("      then append that dir to $GITHUB_PATH.")
        return 1
    if quiet:
        return 0
    print(
        f"check-workflow-just-provisioning: OK — {len(checked)} job(s) CHECKED "
        f"(container Dockerfile or in-job install step), "
        f"{len(self_hosted)} NOT VERIFIED (self-hosted; `just` comes from the "
        f"runner's own store, which the tree cannot see)."
    )
    for path, job in self_hosted:
        print(f"    not verified: {path}  [{job}]")
    return 0


def self_test():
    """Synthetic cases, plus two MUTATIONS of the real tree.

    "N jobs are fine" is also what a probe that can never fail prints
    (`check-reconfigure-stale`'s worry), so the negative controls run against
    the workflows as they actually are: delete the fix and the gate must go red,
    on the real file, naming the real job.
    """
    failures = 0

    def run(name, docs, expect_fail):
        nonlocal failures
        bad, _, _ = audit(docs)
        if bool(bad) != expect_fail:
            print(f"  self-test FAIL: {name}: expected failure={expect_fail}, got {bool(bad)}")
            failures += 1
            return None
        return bad

    P = Path("selftest.yml")
    cases = [
        ("bare runner, no install", {"runs-on": "ubuntu-latest", "steps": [
            {"name": "go", "run": "source ./activate.sh\njust probe checkout\n"}]}, True),
        ("bare runner, install first", {"runs-on": "ubuntu-latest", "steps": [
            {"name": "install", "run": "curl -sSf https://just.systems/install.sh | bash\n"},
            {"name": "go", "run": "just probe checkout\n"}]}, False),
        ("bare runner, install AFTER", {"runs-on": "ubuntu-latest", "steps": [
            {"name": "go", "run": "just probe checkout\n"},
            {"name": "install", "run": "curl -sSf https://just.systems/install.sh | bash\n"}]}, True),
        ("container that bakes just", {"runs-on": "ubuntu-22.04",
            "container": {"image": "ghcr.io/newslabntu/nano-ros-ci:humble"},
            "steps": [{"name": "go", "run": "just check fast\n"}]}, False),
        ("unknown container image", {"runs-on": "ubuntu-22.04",
            "container": {"image": "ubuntu:24.04"},
            "steps": [{"name": "go", "run": "just check fast\n"}]}, True),
        ("self-hosted", {"runs-on": ["self-hosted", "linux", "nros-big"],
            "steps": [{"name": "go", "run": "just ci matrix\n"}]}, False),
        ("no just at all", {"runs-on": "ubuntu-latest", "steps": [
            {"name": "go", "run": "cargo build --release\n"}]}, False),
        # The detector's own false positives, which decide whether this gate is
        # usable: `queue-notify.yml` posts a comment whose TEXT says `just ci l1`.
        ("heredoc text is not a command", {"runs-on": "ubuntu-latest", "steps": [
            {"name": "comment", "run": "body=\"$(cat <<MSG\nRun: just ci l1\nMSG\n)\"\ngh pr comment\n"}]}, False),
        ("comment prose is not a command", {"runs-on": "ubuntu-latest", "steps": [
            {"name": "note", "run": "# rebase and re-run just ci l1\ngh pr view\n"}]}, False),
        ("env-prefixed invocation still counts", {"runs-on": "ubuntu-latest", "steps": [
            {"name": "go", "run": "NROS_ZEPHYR_VERSION=3.7 just setup zephyr\n"}]}, True),
    ]
    for name, job, expect in cases:
        run(name, [(P, {"jobs": {"j": job}})], expect)

    # --- negative controls on the REAL tree -------------------------------
    #
    # "N jobs are fine" is also what a probe that can never fail prints
    # (`check-reconfigure-stale`'s worry), so each provider arm is disarmed on
    # the workflows AS THEY ARE and must go red.
    #
    # NOT an assertion that the tree is currently clean — that is the main
    # check's verdict, and duplicating it here would replace a diagnosis that
    # names the broken job with a self-test line that names nothing. Each arm
    # instead asserts it found a SUBJECT, so an arm that stops being exercised
    # fails loudly rather than passing over an empty set.
    import copy

    docs = load_workflows()

    def red_jobs(mutated):
        return {(str(p), j) for p, j, *_ in audit(mutated)[0]}

    base_red = red_jobs(docs)

    # Arm 1 — the in-job install step. Remove it wherever it is the provider.
    mutated = copy.deepcopy(docs)
    subjects = set()
    for path, doc in mutated:
        for job_name, job in (doc.get("jobs") or {}).items():
            if job.get("container") or is_self_hosted(job.get("runs-on")):
                continue
            kept = [
                s for s in (job.get("steps") or [])
                if not any(m in (s.get("run") or "") for m in INSTALLERS)
            ]
            if len(kept) != len(job.get("steps") or []):
                job["steps"] = kept
                subjects.add((str(path), job_name))
    if not subjects:
        print("  self-test FAIL: arm 1 found no job provided by an in-job install "
              "step — if the verdict above names one, that IS the missing subject")
        failures += 1
    elif not subjects <= red_jobs(mutated) - base_red:
        print(f"  self-test FAIL: arm 1 disarmed {sorted(subjects)} and they did not go red")
        failures += 1

    # Arm 2 — the container. Drop it wherever it is the provider.
    mutated = copy.deepcopy(docs)
    subjects = set()
    for path, doc in mutated:
        for job_name, job in (doc.get("jobs") or {}).items():
            if not job.get("container"):
                continue
            if not any(classify_step(s)[1] for s in (job.get("steps") or [])):
                continue
            job.pop("container")
            subjects.add((str(path), job_name))
    if not subjects:
        print("  self-test FAIL: arm 2 found no containerised job that runs just")
        failures += 1
    elif not subjects <= red_jobs(mutated) - base_red:
        print(f"  self-test FAIL: arm 2 disarmed {sorted(subjects)} and they did not go red")
        failures += 1

    # Arm 3 — the composite-action expansion. `setup-nros-cli` runs `just` and
    # installs nothing; strip the job's OWN `just` lines and its container, so
    # the only remaining invocation is the one inside the action. A gate that
    # read workflows alone would call this job clean.
    mutated = copy.deepcopy(docs)
    moved = None
    for path, doc in mutated:
        for job_name, job in (doc.get("jobs") or {}).items():
            uses = [s.get("uses") or "" for s in job.get("steps") or []]
            if not (any("setup-nros-cli" in u for u in uses) and job.get("container")):
                continue
            job.pop("container")
            job["steps"] = [
                s for s in job["steps"]
                if not JUST.search("\n".join(command_lines(s.get("run") or "")))
            ]
            moved = (str(path), job_name)
            break
        if moved:
            break
    if moved is None:
        print("  self-test FAIL: arm 3 found no containerised setup-nros-cli consumer")
        failures += 1
    elif moved not in red_jobs(mutated) - base_red:
        print(
            f"  self-test FAIL: arm 3 expected {moved} red from the composite "
            "action's own `just` call, which no workflow file mentions"
        )
        failures += 1

    if failures:
        print(f"check-workflow-just-provisioning self-test: {failures} case(s) FAILED")
        return 1
    print(
        f"check-workflow-just-provisioning self-test: OK "
        f"({len(cases)} synthetic + 3 real-tree provider arms disarmed)"
    )
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()

    if args.self_test:
        return self_test()

    # The VERDICT first, the self-test second — the reverse of this repo's usual
    # order, and deliberately. The self-test disarms each provider arm on the
    # real tree, so a tree that already HAS the defect leaves an arm with no
    # subject: run the self-test first and a genuinely broken workflow reports
    # as a self-test complaint about an empty set instead of naming the job.
    # A broken detector still cannot pass, because both run every time.
    verdict = report(*audit(load_workflows()))
    return 1 if (self_test() != 0 or verdict) else 0


if __name__ == "__main__":
    sys.exit(main())
