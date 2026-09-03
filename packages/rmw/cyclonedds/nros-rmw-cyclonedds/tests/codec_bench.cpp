// Issue 0969 — what the removed CDR round trip COSTS, with nothing else in frame.
//
// The end-to-end harnesses could not answer this. `call_blocking` polls with a
// 5 ms sleep, so a client's instruction count tracks how long a reply took, not
// what the codec did: two runs of the identical 100-exchange workload collected
// 3,853,244 and 7,686,322 instructions. And `dds_stream_write_sample` never
// appears in `callgrind_annotate` — it is reached through Cyclone's static,
// inlined cdrstream, so its cost folds into callers that annotate as `???`.
//
// So this measures the codec DIRECTLY: no session, no network, no reader, no
// poll. One prepared wire buffer, decoded and re-encoded in a tight loop, which
// is exactly the work `take_typed_wire` and `write_typed` used to do per message
// and no longer do. The NEW path's equivalent — `ddsi_serdata_to_ser`, a memcpy
// — is timed beside it as the baseline the round trip is being compared against.
//
//   NROS_BENCH_ITERS   loop count (default 100000)
//   NROS_BENCH_SEQ_LEN elements in the reply sequence (default 1)
//
// Reports nanoseconds per operation. Under callgrind it is also clean to
// annotate, because the loop is the only thing running.

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>

#include <dds/dds.h>
#include <dds/ddsi/ddsi_cdrstream.h>
#include <dds/ddsrt/heap.h>

#include "../src/sertype_min.hpp"

extern "C" const dds_topic_descriptor_t nros_test_srv_dds__SumSeq_Response__desc;

namespace {

void put_le64(uint8_t* p, int64_t v) {
    for (int i = 0; i < 8; ++i) p[i] = static_cast<uint8_t>((v >> (8 * i)) & 0xFF);
}

long env_long(const char* name, long dflt) {
    if (const char* s = std::getenv(name)) {
        const long v = std::strtol(s, nullptr, 10);
        if (v > 0) return v;
    }
    return dflt;
}

} // namespace

int main() {
    const long iters = env_long("NROS_BENCH_ITERS", 100000);
    const long seqlen = env_long("NROS_BENCH_SEQ_LEN", 1);

    const dds_topic_descriptor_t* desc = &nros_test_srv_dds__SumSeq_Response__desc;
    nros_rmw_cyclonedds::SertypeMin st(desc);

    // The wire buffer is PRODUCED BY THE CODEC, not hand-built.
    //
    // The first version laid the CDR out by hand — uint32 length then elements —
    // and the decode returned `_length = 0` every time, so the "decode+encode"
    // timing was the cost of decoding an EMPTY sequence and came out flat in
    // payload size. Hand-writing CDR means guessing at DHEADERs, alignment and
    // the type's extensibility; serialising a real sample cannot get those
    // wrong. The check below then proves the round trip is real before anything
    // is timed.
    // {uint32 _maximum; uint32 _length; T* _buffer; bool _release;} + tail pad
    const size_t kSeqStructSize = 24;

    std::vector<uint8_t> wire;
    {
        void* seed = ddsrt_calloc(1, desc->m_size);
        if (seed == nullptr) { std::fprintf(stderr, "calloc failed\n"); return 1; }
        // dds_sequence_t layout: {uint32 _maximum; uint32 _length; T* _buffer; bool _release;}
        // The reply sample is cdds_request_header_t (client GUID + seq, 16 B)
        // FOLLOWED BY the payload struct -- the sequence is NOT at offset 0.
        // Seeding at 0 writes into the header instead: the payload stays zeroed
        // (20 B serialised for any seq_len) while the decode reads the seeded
        // values back out of the header, so the "length matches" check passes
        // on a sample that carries no elements at all.
        const size_t seq_off = desc->m_size - kSeqStructSize;
        auto* seq = static_cast<uint8_t*>(seed) + seq_off;
        auto* buf = static_cast<int64_t*>(ddsrt_malloc(sizeof(int64_t) * static_cast<size_t>(seqlen)));
        if (buf == nullptr) { std::fprintf(stderr, "malloc failed\n"); return 1; }
        for (long i = 0; i < seqlen; ++i) buf[i] = i;
        *reinterpret_cast<uint32_t*>(seq + 0) = static_cast<uint32_t>(seqlen);  // _maximum
        *reinterpret_cast<uint32_t*>(seq + 4) = static_cast<uint32_t>(seqlen);  // _length
        std::memcpy(seq + 8, &buf, sizeof(buf));                                // _buffer
        seq[8 + sizeof(void*)] = 1;                                             // _release

        dds_ostream_t os;
        dds_ostream_init(&os, 0, 1 /*xcdr1*/);
        if (!dds_stream_write_sample(&os, seed, st.as_sertype())) {
            std::fprintf(stderr, "seed serialise failed\n"); return 1;
        }
        wire.resize(4 + os.m_index);
        wire[0] = 0x00; wire[1] = 0x01; wire[2] = 0x00; wire[3] = 0x00;
        std::memcpy(wire.data() + 4, os.m_buffer, os.m_index);
        dds_ostream_fini(&os);
        dds_stream_free_sample(seed, desc->m_ops);
        ddsrt_free(seed);
    }

    const uint32_t paylen = static_cast<uint32_t>(wire.size() - 4);

    // ---- the NEW path's per-message work: one copy out of the serdata.
    std::vector<uint8_t> out(wire.size());
    auto t0 = std::chrono::steady_clock::now();
    for (long i = 0; i < iters; ++i) {
        std::memcpy(out.data(), wire.data(), wire.size());
        // keep the compiler from eliding the copy
        asm volatile("" : : "r,m"(out.data()) : "memory");
    }
    auto t1 = std::chrono::steady_clock::now();
    const double copy_ns =
        std::chrono::duration<double, std::nano>(t1 - t0).count() / static_cast<double>(iters);

    // ---- the OLD path's per-message work: decode to a typed sample, then
    // re-encode it. This is the pair `take_typed_wire` ran on every reply.
    void* sample = ddsrt_calloc(1, desc->m_size);
    if (sample == nullptr) { std::fprintf(stderr, "calloc failed\n"); return 1; }

    // VERIFY the decode actually produced the elements before timing it. A
    // codec that bails on a malformed buffer costs a constant, and a constant
    // is exactly what a flat-looking result would show — so this checks rather
    // than assumes. (`_length` is the second word of a Cyclone dds_sequence_t.)
    {
        dds_istream_t is;
        dds_istream_init(&is, paylen, wire.data() + 4, 1);
        dds_stream_read_sample(&is, sample, st.as_sertype());
        dds_istream_fini(&is);
        const uint32_t got = *reinterpret_cast<const uint32_t*>(
            static_cast<const uint8_t*>(sample) + (desc->m_size - kSeqStructSize)
            + sizeof(uint32_t));
        std::printf("  decode check: sequence _length=%u (want %ld)%s\n",
                    got, seqlen, got == static_cast<uint32_t>(seqlen) ? "" : "   <-- MISMATCH");
        dds_stream_free_sample(sample, desc->m_ops);
        std::memset(sample, 0, desc->m_size);
    }

    auto t2 = std::chrono::steady_clock::now();
    for (long i = 0; i < iters; ++i) {
        dds_istream_t is;
        dds_istream_init(&is, paylen, wire.data() + 4, 1 /*xcdr1, as service.cpp spelled it*/);
        dds_stream_read_sample(&is, sample, st.as_sertype());
        dds_istream_fini(&is);

        dds_ostream_t os;
        dds_ostream_init(&os, 0, 1 /*xcdr1, as service.cpp spelled it*/);
        (void) dds_stream_write_sample(&os, sample, st.as_sertype());
        dds_ostream_fini(&os);

        dds_stream_free_sample(sample, desc->m_ops);
        std::memset(sample, 0, desc->m_size);
    }
    auto t3 = std::chrono::steady_clock::now();
    const double rt_ns =
        std::chrono::duration<double, std::nano>(t3 - t2).count() / static_cast<double>(iters);

    ddsrt_free(sample);

    std::printf("seq_len=%ld payload=%zuB iters=%ld\n", seqlen, wire.size(), iters);
    std::printf("  memcpy (new path)      %10.1f ns/msg\n", copy_ns);
    std::printf("  decode+encode (old)    %10.1f ns/msg\n", rt_ns);
    std::printf("  removed by the change  %10.1f ns/msg  (%.1fx)\n",
                rt_ns - copy_ns, copy_ns > 0 ? rt_ns / copy_ns : 0.0);
    return 0;
}
