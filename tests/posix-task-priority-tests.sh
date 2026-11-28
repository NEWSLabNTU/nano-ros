#!/usr/bin/env bash
# Issue 2934: a task that states a priority STARTS on a host that may not
# request real-time scheduling. Compiles the REAL POSIX port
# (`nros-platform-posix/src/platform.c`) and drives `nros_platform_task_init`.
#
# The program first lowers RLIMIT_RTPRIO to 0, so an unprivileged process is
# refused SCHED_FIFO (EPERM), and PROVES the refusal with a plain pthread
# probe: a process that can still get SCHED_FIFO (root, CAP_SYS_NICE) cannot
# exercise the case, and the test fails as an unmet precondition rather than
# passing vacuously.
#
# Cases:
#   a. a task with a band priority (and a stack, so the port builds an
#      attribute) starts and runs, and the port reports the refused priority
#      once, naming EPERM;
#   b. a second such task starts too, and the refusal is NOT reported again;
#   c. a task that inherits (NROS_PLATFORM_PRIORITY_INHERIT) starts, as before;
#   d. (issue 3024, its own process) a task that has EXITED before its creator
#      returns from `pthread_create` still reports the refusal once, as EPERM,
#      and nothing reports another errno. The port is compiled with
#      `pthread_create` wrapped so the creator resumes only after the child is
#      gone: a port that applies the priority from the creator, to the child's
#      id, then asks about an exited thread every time instead of under load.
# Negative control: the same program against a port without the retry must
# fail case a, so "it started" cannot pass on a port that refuses to spawn.
set -uo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/lib/grep-q.sh
. "$root/scripts/lib/grep-q.sh"
# shellcheck source=scripts/lib/deadline.sh
. "$root/scripts/lib/deadline.sh"
cc="${CC:-cc}"
command -v "$cc" >/dev/null || { echo "posix-task-priority: no C compiler ($cc)" >&2; exit 1; }
fail() { echo "posix-task-priority: $*" >&2; exit 1; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
inc="$root/packages/platform/nros-platform-api/include"
src="$root/packages/platform/nros-platform-posix/src"

cat >"$tmp/main.c" <<'EOF'
#include <nros/platform.h>
#include <pthread.h>
#include <sched.h>
#include <stdio.h>
#include <string.h>
#include <sys/resource.h>
#include <time.h>

static int bad;
#define CHECK(name, cond) do { if (cond) printf("  ok   %s\n", name); \
    else { printf("  FAIL %s\n", name); bad = 1; } } while (0)

static volatile int ran[3];
static void *entry(void *arg) {
    ran[(int)(intptr_t)arg] = 1;
    return NULL;
}
static void *probe_entry(void *arg) { (void)arg; return NULL; }

/* Wait up to 2 s for `ran[i]`. */
static int waited(int i) {
    for (int n = 0; n < 200 && !ran[i]; n++) {
        struct timespec d = {0, 10 * 1000 * 1000};
        nanosleep(&d, NULL);
    }
    return ran[i];
}

static int8_t spawn(int i, int32_t priority) {
    static union { uint8_t b[NROS_PLATFORM_TASK_STORAGE_SIZE]; uint64_t u; void *p; } task[3];
    nros_platform_task_attr_t attr;
    nros_platform_task_attr_init(&attr);
    attr.name = "prio_test";
    attr.stack_bytes = 65536u;
    attr.priority = priority;
    attr.flags = NROS_PLATFORM_TASK_DETACHED;
    return nros_platform_task_init(task[i].b, &attr, entry, (void *)(intptr_t)i);
}

int main(void) {
    struct rlimit rl = {0, 0};
    (void)setrlimit(RLIMIT_RTPRIO, &rl);
    /* Precondition: SCHED_FIFO is refused here. */
    pthread_attr_t pa;
    pthread_attr_init(&pa);
    pthread_attr_setinheritsched(&pa, PTHREAD_EXPLICIT_SCHED);
    pthread_attr_setschedpolicy(&pa, SCHED_FIFO);
    struct sched_param sp;
    memset(&sp, 0, sizeof sp);
    sp.sched_priority = sched_get_priority_min(SCHED_FIFO);
    pthread_attr_setschedparam(&pa, &sp);
    pthread_t probe;
    if (pthread_create(&probe, &pa, probe_entry, NULL) == 0) {
        pthread_join(probe, NULL);
        printf("[UNMET PRECONDITION] this process may still request SCHED_FIFO (root or "
               "CAP_SYS_NICE), so the refusal this test is about cannot happen here\n");
        return 2;
    }
    pthread_attr_destroy(&pa);

    CHECK("a a band-priority task spawns", spawn(0, NROS_PLATFORM_PRIORITY_MAX / 2) == 0);
    CHECK("a ...and runs", waited(0));
    CHECK("b a second one spawns", spawn(1, NROS_PLATFORM_PRIORITY_MAX / 2) == 0);
    CHECK("b ...and runs", waited(1));
    CHECK("c an inheriting task spawns", spawn(2, NROS_PLATFORM_PRIORITY_INHERIT) == 0);
    CHECK("c ...and runs", waited(2));
    return bad;
}
EOF

cat >"$tmp/exited.c" <<'EOF'
#include <nros/platform.h>
#include <pthread.h>
#include <semaphore.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <time.h>

/* The port calls this instead of `pthread_create` (-D on its compile). It
 * returns only after the thread it created has run to completion, so the
 * creator's next statement always meets an exited thread. */
static sem_t gone;
struct wrap { void *(*f)(void *); void *a; };
static void *wrapped(void *raw) {
    struct wrap w = *(struct wrap *) raw;
    free(raw);
    void *r = w.f(w.a);
    sem_post(&gone);
    return r;
}
int nros_test_pthread_create(pthread_t *t, const pthread_attr_t *at, void *(*f)(void *), void *a);
int nros_test_pthread_create(pthread_t *t, const pthread_attr_t *at, void *(*f)(void *), void *a) {
    struct wrap *w = malloc(sizeof *w);
    if (w == NULL) return 11;
    w->f = f;
    w->a = a;
    int rc = pthread_create(t, at, wrapped, w);
    if (rc != 0) { free(w); return rc; }
    sem_wait(&gone);
    struct timespec d = {0, 50 * 1000 * 1000}; /* let the thread finish exiting */
    nanosleep(&d, NULL);
    return 0;
}

static volatile int ran;
static void *entry(void *arg) { (void) arg; ran = 1; return NULL; }

int main(void) {
    struct rlimit rl = {0, 0};
    (void) setrlimit(RLIMIT_RTPRIO, &rl);
    sem_init(&gone, 0, 0);
    static union { uint8_t b[NROS_PLATFORM_TASK_STORAGE_SIZE]; uint64_t u; void *p; } task;
    nros_platform_task_attr_t attr;
    nros_platform_task_attr_init(&attr);
    attr.name = "prio_exited";
    attr.stack_bytes = 65536u;
    attr.priority = NROS_PLATFORM_PRIORITY_MAX / 2;
    attr.flags = NROS_PLATFORM_TASK_DETACHED;
    int8_t rc = nros_platform_task_init(task.b, &attr, entry, NULL);
    printf("  %s d a task that exits before its creator resumes spawns\n", rc == 0 ? "ok  " : "FAIL");
    printf("  %s d ...and ran\n", ran ? "ok  " : "FAIL");
    return (rc == 0 && ran) ? 0 : 1;
}
EOF

build() { # <out> <platform.c> [main.c] [extra port flags...]
    local out="$1" port="$2" main="${3:-$tmp/main.c}"
    shift 2
    [ $# -gt 0 ] && shift
    local flags=(-std=c11 -Wall -Wextra -Werror -D_POSIX_C_SOURCE=200809L -I"$inc")
    "$cc" "${flags[@]}" "$@" -c "$port" -o "$out.platform.o" 2>"$out.err" &&
        "$cc" "${flags[@]}" -c "$src/termination.c" -o "$out.termination.o" 2>>"$out.err" &&
        "$cc" "${flags[@]}" -c "$main" -o "$out.main.o" 2>>"$out.err" &&
        "$cc" "$out.main.o" "$out.platform.o" "$out.termination.o" -o "$out" -lpthread 2>>"$out.err"
}

build "$tmp/real" "$src/platform.c" || { cat "$tmp/real.err" >&2; fail "the test did not build"; }
echo "posix-task-priority: against the POSIX port, SCHED_FIFO refused"
rc=0; out="$("${NROS_DEADLINE[@]}" 20 "$tmp/real" 2>"$tmp/real.stderr")" || rc=$?
printf '%s\n' "$out"
[ "$rc" -ne 2 ] || fail "unmet precondition (see above): run it as an unprivileged user"
[ "$rc" -eq 0 ] || { cat "$tmp/real.stderr" >&2; fail "FAILED (exit $rc)"; }
n="$(grep -c 'REFUSED (EPERM)' "$tmp/real.stderr")" || true
[ "$n" = 1 ] || { cat "$tmp/real.stderr" >&2; fail "the refused priority was reported $n times, not once"; }
echo "  ok   the refusal was reported once, naming EPERM"

# Case d (issue 3024), a process of its own so its once-only report is fresh.
build "$tmp/exited" "$src/platform.c" "$tmp/exited.c" -Dpthread_create=nros_test_pthread_create ||
    { cat "$tmp/exited.err" >&2; fail "case d did not build"; }
rc=0; out="$("${NROS_DEADLINE[@]}" 20 "$tmp/exited" 2>"$tmp/exited.stderr")" || rc=$?
printf '%s\n' "$out"
[ "$rc" -eq 0 ] || { cat "$tmp/exited.stderr" >&2; fail "case d FAILED (exit $rc)"; }
n="$(grep -c 'REFUSED (EPERM)' "$tmp/exited.stderr")" || true
[ "$n" = 1 ] || { cat "$tmp/exited.stderr" >&2; fail "case d: the refusal was reported $n times, not once"; }
rc=0; nros_grep_q -F 'was refused (rc=' "$tmp/exited.stderr" || rc=$?
case "$rc" in
    1) ;;
    0) cat "$tmp/exited.stderr" >&2; fail "case d: a refusal other than EPERM was reported (the priority was set on an exited thread)" ;;
    *) fail "case d: could not read its stderr (grep exit $rc)" ;;
esac
echo "  ok   d a task gone before its creator resumes reports EPERM once, and nothing else"

# Negative control: a port without the EPERM retry fails case a.
python3 - "$src/platform.c" "$tmp/platform_noretry.c" <<'PY'
import sys
s = open(sys.argv[1]).read()
n = "    if (rc == EPERM && explicit_sched) {"
if s.count(n) != 1:
    sys.exit("the EPERM retry was not found")
open(sys.argv[2], "w").write(s.replace(n, "    if (0 && rc == EPERM && explicit_sched) {"))
PY
[ -s "$tmp/platform_noretry.c" ] || fail "negative control: could not build the mutant"
build "$tmp/noretry" "$tmp/platform_noretry.c" || { cat "$tmp/noretry.err" >&2; fail "the mutant did not build"; }
rc=0; out="$("${NROS_DEADLINE[@]}" 20 "$tmp/noretry" 2>&1)" || rc=$?
[ "$rc" -ne 0 ] || { printf '%s\n' "$out" >&2; fail "negative control: a port that refuses to spawn passed"; }
nros_grep_q -F 'FAIL a a band-priority task spawns' <<<"$out"; m=$?
[ "$m" -eq 0 ] || { printf '%s\n' "$out" >&2; fail "negative control: the mutant failed, but not on the spawn"; }
echo "posix-task-priority: OK (negative control: a port without the retry fails to spawn)"
