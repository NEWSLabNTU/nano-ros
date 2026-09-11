/*
 * NEGATIVE probe — phase-417 stage 3, `cpp:Executor::spin_once`.
 *
 * Upstream is `void spin_once(std::chrono::nanoseconds timeout = -1)`: BLOCK
 * INDEFINITELY, execute one ready item. Ours takes a millisecond budget, and
 * used to default it to 10 — so a ported argument-free `exec.spin_once()`
 * compiled and returned after 10 ms where upstream was still waiting. RFC-0089:
 * a contract that differs must fail to COMPILE, never compile and differ.
 *
 * `just check cpp` requires this TU to FAIL and requires the failure text to
 * carry `REFUSED by nano-ros` — exit code alone would also be produced by a
 * typo in an include path, which would make the check prove nothing.
 *
 * The POSITIVE half is `spin_verbs.cpp`, which proves the BUDGETED form still
 * compiles and still returns `nros::Result`. Compile that one first: an
 * expected-failure compile cannot tell "the refusal fired" from "the file is
 * not there".
 *
 * The VALUE half of the same refusal — `spin_once(-1)`, which used to be
 * clamped to a 0 ms poll — is not knowable at compile time, so it is refused at
 * the call and measured by `ros2_loudness_runtime.cpp` instead.
 */

#include <nros/nros.hpp>

int ros2_refuse_unbounded_spin_probe();
int ros2_refuse_unbounded_spin_probe() {
    nros::Executor exec;
    // The line upstream's own tutorials write.
    (void)exec.spin_once();
    return 0;
}
