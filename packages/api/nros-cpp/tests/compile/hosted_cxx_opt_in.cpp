// The hosted C++ a library such as Eigen needs, compiled in a directory that
// called `nano_ros_hosted_cxx()` (cmake/NanoRosPortedCxx.cmake).
//
// `check-hosted-cxx-opt-in` builds this under the real FreeRTOS toolchain file
// twice: in a directory that opted in, where it must compile, and in one that
// did not, where libstdc++ must refuse it for being freestanding. The include
// list is the unconditional standard-library set of Eigen 3.4's `Eigen/Core`,
// so "Eigen compiles here" is what the probe stands for without vendoring
// Eigen into the tree.
#include <algorithm>
#include <array>
#include <cassert>
#include <climits>
#include <cmath>
#include <complex>
#include <cstddef>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <iosfwd>
#include <limits>
#include <memory>
#include <new>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

static_assert(__STDC_HOSTED__ == 1, "nano_ros_hosted_cxx() left this directory freestanding");

float hosted_cxx_opt_in_probe(float x) {
    std::vector<float> v{x, 2.0f * x};
    const std::complex<float> z(v[0], v[1]);
    return std::sqrt(std::norm(z)) + std::numeric_limits<float>::epsilon();
}
