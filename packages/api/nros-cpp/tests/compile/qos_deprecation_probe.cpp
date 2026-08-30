/*
 * Phase 379 W5 — the NEGATIVE half of `qos_policy_accessors.cpp`.
 *
 * That file proves the old `QoS::*_raw()` / `QoS::*_ms()` spellings still
 * COMPILE. This one proves they still WARN. A deprecation nobody is told about
 * is just an alias, and the maintainer's policy for this campaign is
 * `[[deprecated]]` now and removal later — so "the attribute reaches callers"
 * is the thing worth pinning, not an implementation detail.
 *
 * `just check cpp` compiles this with `-Werror=deprecated-declarations` and
 * requires it to FAIL. It is a normal, valid TU otherwise; only that flag turns
 * the warnings into errors. Written as an expected failure because a clean
 * compile is exactly what a silently-dropped attribute looks like.
 *
 * Same shape as the C half, `packages/api/nros-c/tests/compile/
 * param_deprecation_probe.c`. C++ gets one thing C could not: the deprecated
 * TYPE alias `QoS::Liveliness` warns here, where C's `typedef` aliases cannot.
 */

#include "nros/qos.hpp"

int qos_deprecation_probe();
int qos_deprecation_probe() {
    nros::QoS qos;
    qos.deadline_ms(100);
    qos.lifespan_ms(200);
    qos.liveliness_lease_ms(300);
    nros::QoS::Liveliness kind = nros::QoS::LivelinessAutomatic;
    return qos.reliability_raw() + qos.durability_raw() + qos.history_raw() + qos.liveliness_raw() +
           static_cast<int>(qos.deadline_ms()) + static_cast<int>(qos.lifespan_ms()) +
           static_cast<int>(qos.liveliness_lease_ms()) + static_cast<int>(kind);
}
