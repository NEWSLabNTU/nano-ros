// nros-cpp: type-erased out-of-line state block
// Freestanding C++ — no exceptions, no STL required

/**
 * @file hosted_block.hpp
 * @ingroup grp_support
 * @brief `nros::detail::HostedBlockBase` — the one shape state takes when it
 *        has to live behind a pointer.
 */

#ifndef NROS_CPP_HOSTED_BLOCK_HPP
#define NROS_CPP_HOSTED_BLOCK_HPP

namespace nros {
namespace detail {

/// Type-erased head of an out-of-line state block.
///
/// A capability probe may gate a METHOD. It may never change `sizeof` — two
/// translation units of one image are allowed to disagree about a capability
/// (`examples/px4/cpp/bridge/.../CMakeLists.txt` sets `-DNROS_CPP_STD=1` on one
/// module of a larger image, deliberately), so a member behind a probe is a
/// silent one-definition hazard. Issue 0135's class; issues 1225 and 1204 are
/// the two most recent sightings.
///
/// State that EXCEEDS a pointer therefore hides behind one unconditional
/// `void*`. But the pointee's type is capability-dependent, and the owner's
/// destructor is compiled in BOTH configurations, so that destructor cannot
/// name it: a `#if`-gated `delete` would give the two translation units two
/// different inline destructors, which is the ODR half of the same defect.
///
/// So the block carries its own destroyer. The owner's destructor is
/// byte-identical in every configuration — it calls through this function
/// pointer when the pointer is non-null, and a configuration that cannot
/// allocate the block never sets it.
///
/// A block stores the address of its BASE subobject in the owner's `void*`
/// (the cast is written out at the allocation site), so the round trip is
/// exact.
struct HostedBlockBase {
    void (*destroy)(void*);
};

/// Destroy the block `p` points at, if any, and null `p`.
///
/// Nulls BEFORE calling the destroyer: the destroyer runs arbitrary code, and
/// the owner must not be reachable holding a pointer to a half-destroyed
/// block.
inline void destroy_hosted_block(void*& p) {
    if (p != nullptr) {
        HostedBlockBase* block = static_cast<HostedBlockBase*>(p);
        p = nullptr;
        block->destroy(block);
    }
}

} // namespace detail
} // namespace nros

#endif // NROS_CPP_HOSTED_BLOCK_HPP
