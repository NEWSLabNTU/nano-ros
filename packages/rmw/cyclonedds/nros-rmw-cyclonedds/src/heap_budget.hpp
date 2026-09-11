#ifndef NROS_RMW_CYCLONEDDS_HEAP_BUDGET_HPP
#define NROS_RMW_CYCLONEDDS_HEAP_BUDGET_HPP

// phase-454 W6.c — RFC-0100 D11: CycloneDDS gets a heap budget and asserts it at
// boot.
//
// # Why this backend has no pool to size, and what it has instead
//
// Every other backend in this tree answers the sizing campaign with a static
// pool. Cyclone answers with `ddsrt_malloc`, and `subscriber.cpp:96-99` says so
// outright:
//
//   "a Cyclone consumer can set the hint, do everything the sizing campaign asks,
//    and correctly observe nothing change in this backend. What DOES change is
//    the executor's arena ... measure the arena, not the backend."
//
// So the demand lands one layer up, and D11 is what makes this backend a
// first-class consumer anyway: derive a required-heap number from the same facts
// the model already states, and have the image check its configured heap against
// it at boot. No static pool is invented, and nothing here allocates.
//
// # What the number IS, and — more importantly — what it is NOT
//
// It is a **floor**: bytes this image is CERTAIN to need from the ddsrt heap
// before it can publish anything. It is not an accounting of Cyclone's heap use,
// which depends on the graph it discovers, the samples in flight and the peers
// it meets — none of which a build can know. Every term below is chosen so that
// a real image needs AT LEAST this much:
//
//   * the `<Sizing>` receive buffers, which `cyclone_config.hpp` bakes as
//     literals into the domain config on every platform that has a baseline.
//     Cyclone allocates them when the participant comes up;
//   * one `dds_topic_descriptor_t` and one mangled type name PER REGISTERED
//     TYPE. `dynamic_type_builder.cpp:1172-1189` allocates exactly three blocks
//     per type — the ops array, the name, the descriptor — and never frees them,
//     because the registry memoises the result for the process lifetime;
//   * the ops array of the LARGEST type, once. Every type's array is at least
//     `kMinOpsWordsPerKind` words per flattened kind, and at least one type has
//     `max_kinds` of them.
//
// Being a floor is what makes it usable as a hard boot check: it can report a
// heap that is too small, and it can NEVER refuse an image that would have
// worked. A ceiling would have the opposite and much worse property.
//
// The facts come from `[types]` in the sizing descriptor (RFC-0100 D5 — Cyclone
// reads `[types]` and `[target].heap_budget_bytes`, and nothing else), delivered
// as compile definitions by `nros-rmw-cyclonedds-sys`'s build script. An image
// that states none keeps every default here and compiles exactly as it did
// before this wave (D6).

#include <stddef.h>

#include "dds/dds.h"

// The distinct DDS types this image registers -- the same knob that sizes
// `descriptors.cpp`'s static table, and the same count the Rust registry's
// `NROS_CYCLONEDDS_MAX_TYPES` comes from.
#ifndef NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES
#define NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES 256
#endif

// The flattened kind count of the image's LARGEST schema. Mirrors
// `dynamic_type.rs`'s `MAX_KINDS`, which is the Rust half of the same knob.
#ifndef NROS_CYCLONEDDS_MAX_KINDS
#define NROS_CYCLONEDDS_MAX_KINDS 256
#endif

namespace nros_rmw_cyclonedds {

/// `<Sizing><ReceiveBufferSize>` in `cyclone_config.hpp`'s baked baseline.
///
/// A mirror, and the only honest kind available: that block is XML inside a
/// string literal, so no compiler can read it and no gate can compare the two
/// spellings. `cyclone_config_compose.cpp` asserts the literal exists; this
/// number and that string move together by hand.
constexpr size_t kReceiveBufferBytes = 64u * 1024u;

/// `<Sizing><ReceiveBufferChunkSize>`, same mirror.
constexpr size_t kReceiveBufferChunkBytes = 16u * 1024u;

/// The smallest ops-array contribution one flattened kind can make.
///
/// Every `DDS_OP_ADR` in `dynamic_type_builder.cpp` pushes at least an opcode
/// word and an offset word (`:762-763` is the shortest arm; the array, sequence
/// and bounded-string arms all push more). Two is therefore a lower bound over
/// every kind, which is what keeps the total a floor.
constexpr size_t kMinOpsWordsPerKind = 2;

/// The smallest allocation `mangle_type_name` can make.
///
/// It allocates `strlen(raw) + 16` (`dynamic_type_builder.cpp:260-261`), so 16
/// bytes is a lower bound for any name at all, including the empty one.
constexpr size_t kMinTypeNameBytes = 16;

/// Bytes this image is certain to need from the ddsrt heap. See the header
/// comment for why this is a floor and why that is the point.
///
/// `constexpr`, so a caller may use it in a `static_assert` as well as at boot.
constexpr size_t required_heap_bytes(size_t type_count, size_t max_kinds) {
    return kReceiveBufferBytes + kReceiveBufferChunkBytes +
           type_count * (sizeof(dds_topic_descriptor_t) + kMinTypeNameBytes) +
           max_kinds * kMinOpsWordsPerKind * sizeof(uint32_t);
}

/// This image's own floor, from the knobs the model derived.
constexpr size_t kRequiredHeapBytes =
    required_heap_bytes(NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES, NROS_CYCLONEDDS_MAX_KINDS);

/// Does this image state a heap budget at all?
///
/// `[target].heap_budget_bytes` reaches here only when the board states
/// `[board.knobs.memory] heap_bytes` AND the descriptor was written. Absent, the
/// check below is a no-op — a consumer with no declaration keeps its own
/// behaviour, which here means "do not judge a number nobody gave you" (D6).
#ifdef NROS_CYCLONEDDS_HEAP_BUDGET_BYTES
constexpr bool kHeapBudgetStated = true;
constexpr size_t kHeapBudgetBytes = NROS_CYCLONEDDS_HEAP_BUDGET_BYTES;
#else
constexpr bool kHeapBudgetStated = false;
constexpr size_t kHeapBudgetBytes = 0;
#endif

/// Is `budget` short of what an image of this shape is certain to need?
///
/// `stated` carries the THIRD state the reader has (`Fact::Absent`): a budget
/// nobody gave is not a budget that is too small, and conflating the two is what
/// RFC-0100 D6 forbids one layer up. Parameterised rather than folded into the
/// image's own constants so a test can exercise the predicate across values --
/// the constants below are fixed by the compile line, so a header that only
/// exposed them could be tested at exactly one point.
constexpr bool budget_is_short(bool stated, size_t budget, size_t type_count, size_t max_kinds) {
    return stated && budget < required_heap_bytes(type_count, max_kinds);
}

/// True when THIS image's stated budget cannot cover THIS image's floor.
constexpr bool heap_budget_is_short() {
    return budget_is_short(kHeapBudgetStated, kHeapBudgetBytes,
                           NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES, NROS_CYCLONEDDS_MAX_KINDS);
}

} // namespace nros_rmw_cyclonedds

#endif // NROS_RMW_CYCLONEDDS_HEAP_BUDGET_HPP
