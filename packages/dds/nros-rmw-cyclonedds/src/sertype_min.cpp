// Phase 117.6.B — minimal `ddsi_sertype_default` builder.

#include "sertype_min.hpp"

#include <cstdlib>
#include <cstring>

namespace nros_rmw_cyclonedds {

SertypeMin::SertypeMin(const dds_topic_descriptor_t *desc) : desc_(desc) {
    std::memset(&st_, 0, sizeof(st_));
    if (desc == nullptr) {
        return;
    }

    // dds_stream_countops walks `desc->m_ops` until it sees DDS_OP_RTS
    // and returns the total word count, including any nested keys.
    uint32_t nops = dds_stream_countops(desc->m_ops, desc->m_nkeys, desc->m_keys);
    ops_copy_ = static_cast<uint32_t *>(
        std::malloc(static_cast<size_t>(nops) * sizeof(uint32_t)));
    if (ops_copy_ != nullptr) {
        std::memcpy(ops_copy_, desc->m_ops, nops * sizeof(uint32_t));
    }

    st_.type.size    = desc->m_size;
    st_.type.align   = desc->m_align;
    st_.type.flagset = desc->m_flagset;
    st_.type.ops.nops = nops;
    st_.type.ops.ops  = ops_copy_;
    // Keys: not strictly required by `dds_stream_read_sample` /
    // `dds_stream_write_sample` (they walk `ops` only), but
    // populating them keeps the struct internally consistent if a
    // future code path peeks at the keys.
    st_.type.keys.nkeys = 0;
    st_.type.keys.keys  = nullptr;

    // `opt_size_xcdr1/2` is the fast-path "memcpy struct directly to
    // CDR" hint when the layout is identical. Compute it the same way
    // `ddsi_sertype_default_init` does so the read/write fast paths
    // engage when applicable.
    st_.opt_size_xcdr1 = dds_stream_check_optimize(&st_.type, 1);
    st_.opt_size_xcdr2 = dds_stream_check_optimize(&st_.type, 2);
}

SertypeMin::~SertypeMin() {
    if (ops_copy_ != nullptr) {
        std::free(ops_copy_);
        ops_copy_ = nullptr;
    }
}

} // namespace nros_rmw_cyclonedds
