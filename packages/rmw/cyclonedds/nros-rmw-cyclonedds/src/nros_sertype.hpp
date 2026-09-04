#ifndef NROS_RMW_CYCLONEDDS_NROS_SERTYPE_HPP
#define NROS_RMW_CYCLONEDDS_NROS_SERTYPE_HPP

// Issue 0970 — a sertype that speaks CDR bytes, so neither direction of the
// data path builds a typed C struct.
//
// Before this, the backend registered its topics with `dds_create_topic(desc)`,
// which installs Cyclone's OWN sertype. That sertype's notion of a sample is a
// typed C struct, so every publish had to deserialize the caller's CDR into one
// (`dds_stream_read_sample` → `dds_write`) and — until issue 0969 — every take
// had to serialize one back out. `sertype_min.hpp` recorded that round trip as
// blocked on Cyclone exposing `dds_writer_lookup_serdatatype`.
//
// It was never blocked on that. `dds_writer_lookup_serdatatype` recovers a
// sertype you do not own; registering your own makes the question moot, which
// is what `rmw_cyclonedds_cpp` has always done (`create_sertype` →
// `dds_create_topic_sertype`, `rmw_node.cpp:1993`). This is that, reduced to
// what nano-ros needs: nothing above this layer ever wants a typed struct, so
// there is no typesupport here and no deserializer — the "sample" is a
// `NrosCdrBlob`, a pointer and a length, and the serdata holds the bytes.
//
// Scope, deliberately narrow:
//
//   * KEYLESS TYPES ONLY. `create_nros_sertype` refuses a descriptor that
//     declares keys. ROS 2 `.msg` types have none (rosidl emits no `@key`, and
//     neither does `scripts/cyclonedds/msg_to_cyclone_idl.py`), and a keyless
//     type collapses instance handling to a single instance — which is why the
//     key half of upstream's serdata has no counterpart here. A keyed type
//     would need real key extraction and would silently alias every instance
//     into one without it, so it is refused rather than approximated.
//
//   * The TYPE NAME comes from the descriptor, so what goes out over SEDP is
//     byte-for-byte what `dds_create_topic(desc)` published. Discovery and
//     remote type matching are unchanged by this file.
//
//   * `service.cpp` still creates its topics from descriptors and still uses
//     `sertype_min.hpp`. Its request/reply path has type-specific fallbacks
//     that read the typed sample, and moving it belongs with the work that
//     retires those, not here.

#include <dds/dds.h>
#include <dds/ddsi/ddsi_sertype.h>

#include <cstddef>
#include <cstdint>

namespace nros_rmw_cyclonedds {

/// The application sample this sertype speaks: a borrowed span of CDR,
/// encapsulation header included.
///
/// `dds_write(writer, &blob)` reaches `from_sample`, which copies the span into
/// a serdata. The span only has to outlive that call.
///
/// **No default member initialisers, deliberately (issue 1011).** A class with
/// NSDMIs is not an aggregate in C++11 -- C++14 relaxed exactly that rule -- and
/// the Zephyr lane compiles this TU with `-std=c++11`. With `{nullptr}` / `{0}`
/// here, `NrosCdrBlob{data, len}` stops being aggregate initialisation there and
/// the compiler looks for a two-argument constructor that does not exist:
///
///     publisher.cpp:287: error: no matching function for call to
///       'nros_rmw_cyclonedds::NrosCdrBlob::NrosCdrBlob(<brace-enclosed initializer list>)'
///
/// Every use site either brace-initialises both members or casts from `void*`;
/// nothing default-constructs one, so the initialisers bought nothing and cost
/// the whole Zephyr Cyclone lane. Keeping them out also keeps the type
/// trivially default-constructible on top of trivially copyable, which is what
/// `sertype_zero_samples` / `sertype_realloc_samples` rely on when they `memset`
/// and `dds_realloc` this as raw storage.
struct NrosCdrBlob {
    const uint8_t* data;
    size_t size;
};

/// Build a sertype for @p desc's type name, to be handed to
/// `dds_create_topic_sertype`.
///
/// Returns nullptr if @p desc is null or declares keys (see the scope note
/// above). Ownership transfers to the domain on a successful
/// `dds_create_topic_sertype`; on failure the caller frees it with
/// `ddsi_sertype_unref`.
struct ddsi_sertype* create_nros_sertype(const dds_topic_descriptor_t* desc);

} // namespace nros_rmw_cyclonedds

#endif // NROS_RMW_CYCLONEDDS_NROS_SERTYPE_HPP
