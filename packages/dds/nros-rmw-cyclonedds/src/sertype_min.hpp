#ifndef NROS_RMW_CYCLONEDDS_SERTYPE_MIN_HPP
#define NROS_RMW_CYCLONEDDS_SERTYPE_MIN_HPP

// Phase 117.6.B — minimal `ddsi_sertype_default` builder.
//
// Cyclone's `dds_writecdr` / `dds_takecdr` raw-CDR API needs a real
// `ddsi_sertype *` linked to a `ddsi_domaingv`, which our backend
// can't get to without reaching into Cyclone's private struct
// layout. We sidestep that path entirely:
//
//   publish_raw  : CDR bytes →  dds_stream_read_sample → typed buf
//                  → dds_write (Cyclone re-serialises) → wire
//   try_recv_raw : wire → dds_take (typed buf)
//                  → dds_stream_write_sample → CDR bytes → caller
//
// `dds_stream_read_sample` / `dds_stream_write_sample` (public
// `dds/ddsi/ddsi_cdrstream.h`) take `const struct ddsi_sertype_default *`
// and only read a small subset of the struct: `type.size`,
// `type.flagset`, `type.ops`, `type.keys`, plus `opt_size_xcdr1/2`.
// They never dereference `serpool` or other gv-derived fields.
//
// We populate exactly those fields from a `dds_topic_descriptor_t`
// (which Cyclone's idlc emits, no internal access needed) and zero
// everything else.
//
// Cost: a 2× CDR roundtrip per publish + per recv. Acceptable for
// an in-tree smoke; low-throughput control loops on Cortex-A/R
// safety MCUs run well under the headroom. A future zero-copy fast
// path can replace this once Cyclone exposes
// `dds_writer_lookup_serdatatype` upstream.

#include <dds/dds.h>
#include <dds/ddsi/ddsi_cdrstream.h>
#include <dds/ddsi/ddsi_serdata_default.h>
#include <dds/ddsi/ddsi_sertype.h>

namespace nros_rmw_cyclonedds {

/**
 * Minimum-effort `ddsi_sertype_default` builder.
 *
 * Owns a heap-allocated copy of the descriptor's `m_ops` array (the
 * destructor frees it) but borrows everything else from the
 * descriptor. Caller keeps the descriptor alive for the builder's
 * lifetime.
 */
class SertypeMin {
  public:
    explicit SertypeMin(const dds_topic_descriptor_t *desc);
    ~SertypeMin();

    SertypeMin(const SertypeMin&) = delete;
    SertypeMin& operator=(const SertypeMin&) = delete;
    SertypeMin(SertypeMin&&) = delete;
    SertypeMin& operator=(SertypeMin&&) = delete;

    /** Sertype suitable for `dds_stream_read_sample` /
     *  `dds_stream_write_sample`. */
    const struct ddsi_sertype_default *as_sertype() const { return &st_; }

    /** Underlying descriptor, for `m_size` / `m_ops` lookups. */
    const dds_topic_descriptor_t *descriptor() const { return desc_; }

  private:
    const dds_topic_descriptor_t *desc_;
    struct ddsi_sertype_default   st_{};
    uint32_t                     *ops_copy_{nullptr};
};

} // namespace nros_rmw_cyclonedds

#endif // NROS_RMW_CYCLONEDDS_SERTYPE_MIN_HPP
