// env_compat.hpp — reading the environment where there may not be one.
//
// phase-370 W4. Deliberately dependency-free: `<cstddef>` and, on hosted
// targets, `<stdlib.h>`. It is included by `topic_prefix.hpp`, which is
// otherwise light enough to be used from a test TU that has no CycloneDDS
// headers on its include path — putting this in `internal.hpp` (the obvious
// home) pulled `dds/dds.h` into those TUs and broke `check-rmw-cyclonedds`.

#ifndef NROS_RMW_CYCLONEDDS_ENV_COMPAT_HPP
#define NROS_RMW_CYCLONEDDS_ENV_COMPAT_HPP

#if !defined(__STDC_HOSTED__) || __STDC_HOSTED__ != 0
#include <stdlib.h>
#endif

namespace nros_rmw_cyclonedds {

/// Read an environment variable, or `nullptr` where there is no environment.
///
/// The three env lookups in this backend (`CYCLONEDDS_URI`,
/// `NROS_RMW_CYCLONEDDS_SKIP_PREFIX`, and `service.cpp`'s `env_u64`) all wrote
/// `std::getenv`, which does not compile on the arm-none-eabi cross: that build
/// passes `-ffreestanding`, so `__STDC_HOSTED__` is 0 and newlib's
/// `<stdlib.h>` declares no `getenv` in either namespace.
///
/// The answer is not another spelling. A freestanding image HAS no environment,
/// which every one of those call sites already assumed — `env_u64`'s comment
/// says "getenv returns null on RTOS targets with no environment, so the
/// defaults apply there". This makes that assumption the code rather than a
/// remark, and gives the three sites one spelling instead of three.
///
/// `__STDC_HOSTED__` and not `NROS_CPP_STD`: the question here is whether the C
/// LIBRARY provides `getenv`, which is exactly what that macro answers. (The
/// nros-cpp rule to prefer `NROS_CPP_STD` is about C++ STD HEADERS, where a
/// hosted compiler running `-nostdinc++` makes `__STDC_HOSTED__` the wrong
/// test — issue 0112. Different question, different macro.)
inline const char* env_lookup(const char* name) {
#if defined(__STDC_HOSTED__) && __STDC_HOSTED__ == 0
    (void) name;
    return nullptr;
#else
    return ::getenv(name);
#endif
}

}  // namespace nros_rmw_cyclonedds

#endif  // NROS_RMW_CYCLONEDDS_ENV_COMPAT_HPP
