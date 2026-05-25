// Phase 117.6.B — minimal `ddsi_sertype_default` builder.

#include "sertype_min.hpp"

#include <dds/ddsrt/heap.h>

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
        ddsrt_malloc(static_cast<size_t>(nops) * sizeof(uint32_t)));
    if (ops_copy_ != nullptr) {
        std::memcpy(ops_copy_, desc->m_ops, nops * sizeof(uint32_t));
    }

    enum ddsi_sertype_extensibility type_ext;
    if (dds_stream_extensibility(desc->m_ops, &type_ext)) {
        st_.encoding_format = ddsi_sertype_extensibility_enc_format(type_ext);
    } else {
        st_.encoding_format = CDR_ENC_FORMAT_PLAIN;
    }
    st_.write_encoding_version = CDR_ENC_VERSION_1;

    st_.type.size    = desc->m_size;
    st_.type.align   = desc->m_align;
    st_.type.flagset = desc->m_flagset;
    st_.type.ops.nops = nops;
    st_.type.ops.ops  = ops_copy_;
    st_.type.keys.nkeys = desc->m_nkeys;
    if (desc->m_nkeys > 0) {
        keys_copy_ = static_cast<ddsi_sertype_default_desc_key_t *>(
            ddsrt_malloc(static_cast<size_t>(desc->m_nkeys) * sizeof(*keys_copy_)));
        if (keys_copy_ != nullptr) {
            for (uint32_t i = 0; i < desc->m_nkeys; ++i) {
                keys_copy_[i].ops_offs = desc->m_keys[i].m_offset;
                keys_copy_[i].idx = desc->m_keys[i].m_idx;
            }
        }
    }
    st_.type.keys.keys = keys_copy_;

    // `opt_size_xcdr1/2` is the fast-path "memcpy struct directly to
    // CDR" hint when the layout is identical. ThreadX skips this optional
    // precompute because its embedded Cyclone path has tripped in the ops
    // walker; the normal stream encoder/decoder remains in use.
    //
    // Guard on `NROS_PLATFORM_THREADX` (set PRIVATE on this target in
    // CMakeLists.txt), not Cyclone's internal `DDSRT_WITH_THREADX`: the
    // latter only reaches this TU via a transitive `dds/config.h` include,
    // so a `#if` on it fails open (re-enables the trapping fast path with
    // no error) if that header ever drops out of the chain.
#if defined(NROS_PLATFORM_THREADX)
    st_.opt_size_xcdr1 = 0;
    st_.opt_size_xcdr2 = 0;
#else
    st_.opt_size_xcdr1 = dds_stream_check_optimize(&st_.type, 1);
    st_.opt_size_xcdr2 = dds_stream_check_optimize(&st_.type, 2);
#endif
}

SertypeMin::~SertypeMin() {
    if (ops_copy_ != nullptr) {
        ddsrt_free(ops_copy_);
        ops_copy_ = nullptr;
    }
    if (keys_copy_ != nullptr) {
        ddsrt_free(keys_copy_);
        keys_copy_ = nullptr;
    }
}

} // namespace nros_rmw_cyclonedds
