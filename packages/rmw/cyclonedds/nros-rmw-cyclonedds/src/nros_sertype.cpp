// Issue 0970 — the CDR-blob sertype. See `nros_sertype.hpp` for why it exists
// and what it deliberately does not do.
//
// Modelled on `rmw_cyclonedds_cpp/src/serdata.cpp`, minus everything that
// serves a typesupport: no key extraction (keyless types only), no
// deserializer, no on-demand serialization (the bytes are always already
// there). What remains is the op table Cyclone requires plus a buffer.

#include "nros_sertype.hpp"

#include <dds/ddsi/ddsi_serdata.h>
#include <dds/ddsi/ddsi_keyhash.h>
#include <dds/ddsi/q_radmin.h>
#include <dds/ddsrt/heap.h>

// Issue 0942 — `<cstdio>` is only required to declare these in `std`, and a
// freestanding libstdc++ does the reverse, so a board toolchain can fail on
// `std::snprintf` where every hosted target passed. Unqualified, from
// `<stdio.h>`, as `descriptors.cpp` does.
#include <stdio.h>

// NO <memory>, NO <string> — issue 1014.
//
// This TU is compiled `-ffreestanding -nostdinc++` for
// nros-board-threadx-qemu-riscv64 against a ten-header `cxx-compat` shim that
// has neither, so including them was a hard error and the Cyclone backend had
// never built for that board. Same class as archived issue 0112, and the
// sibling `descriptors.cpp` already carries the lesson in its own include
// block. `<cstring>` IS in the shim, and it exports `std::strcmp`/`std::strlen`
// alongside the `std::memcpy` this file already uses.
#include <cstring>
#include <new>

namespace nros_rmw_cyclonedds {

namespace {

/// The `std::unique_ptr` this file used, minus the header — issue 1014.
///
/// The three `serdata_from_*` entry points allocate, bail on any failure, and
/// hand ownership to Cyclone on success. That is exactly `unique_ptr`'s shape
/// and the call sites are unchanged: `!d`, `d.get()`, `d->field`, `d.release()`.
/// Deleting a null pointer is a defined no-op, so the early returns need no
/// guard.
///
/// DIRECT-initialised at the call sites (`OwnPtr<T> d(p);`), not
/// `auto d = OwnPtr<T>(p);`. This TU is `-std=c++14`, where that spelling is
/// copy-initialisation and needs an accessible copy or move constructor —
/// guaranteed elision is C++17. `unique_ptr` got away with it by having a move
/// constructor; giving this one a move it never uses would be machinery for a
/// spelling, so the spelling changed instead. Caught by compiling, not by
/// reading.
template <typename T>
class OwnPtr {
public:
    explicit OwnPtr(T* p) noexcept : p_(p) {}
    ~OwnPtr() { delete p_; }
    OwnPtr(const OwnPtr&) = delete;
    OwnPtr& operator=(const OwnPtr&) = delete;
    explicit operator bool() const noexcept { return p_ != nullptr; }
    T* get() const noexcept { return p_; }
    T* operator->() const noexcept { return p_; }
    T* release() noexcept {
        T* t = p_;
        p_ = nullptr;
        return t;
    }

private:
    T* p_;
};

/// No members: the type name lives in the BASE.
///
/// This used to carry a `std::string type_name`, which was a redundant second
/// copy — `ddsi_sertype_init_flags` does `tp->type_name = ddsrt_strdup(name)`
/// (ddsi_sertype.c:176), so Cyclone already owns a heap copy in
/// `ddsi_sertype::type_name`, freed by `ddsi_sertype_fini` in `sertype_free`.
/// Dropping it also settles a lifetime question rather than answering it: it no
/// longer matters whether `desc->m_typename` outlives the sertype, because the
/// only copy that survives the call is the one Cyclone made.
struct NrosSertype : ddsi_sertype {};

struct NrosSerdata : ddsi_serdata {
    size_t size{0};
    uint8_t* data{nullptr};

    NrosSerdata(const struct ddsi_sertype* tp, enum ddsi_serdata_kind k) {
        ddsi_serdata_init(this, tp, k);
    }
    ~NrosSerdata() { ddsrt_free(data); }

    NrosSerdata(const NrosSerdata&) = delete;
    NrosSerdata& operator=(const NrosSerdata&) = delete;
};

/// Give @p d a zeroed buffer for `n` bytes of CDR.
///
/// Rounded up to 4, because `to_ser` may be asked for up to `alignup4(size)` —
/// the protocol treats the pad as undefined, but reading it has to stay in
/// bounds. Zeroed so the pad is deterministic: these bytes reach a consumer's
/// buffer, and leaving them as whatever the allocator returned would make two
/// otherwise-identical messages compare unequal.
///
/// `ddsrt_malloc`, not `new[]`. Phase 177.26.RX.2's rule applies even though
/// nothing hands this buffer to Cyclone to free: on ThreadX and FreeRTOS the
/// libc heap is separate from — and may be unconfigured relative to — the ddsrt
/// heap Cyclone itself was given, so `new[]` can return null for every message
/// on a board where `ddsrt_malloc` works. It is also where Cyclone's own
/// `serdata_default` payloads come from, so the receive path stays on one heap
/// for the memory campaign's ledger to reason about.
///
/// A free function rather than a method so `scripts/rmw-alloc-sites.py` can
/// name it: that scanner identifies a definition by the last identifier before
/// the parameter list ON A LINE STARTING IN COLUMN 0, so an indented member
/// function is attributed to `<file scope>` and the ledger cannot say which
/// call site it is explaining.
bool serdata_alloc(NrosSerdata* d, size_t n) {
    const size_t padded = (n + 3u) & ~static_cast<size_t>(3u);
    const size_t bytes = padded > 0 ? padded : 4;
    d->data = static_cast<uint8_t*>(ddsrt_malloc(bytes));
    if (d->data == nullptr) {
        return false;
    }
    std::memset(d->data, 0, bytes);
    d->size = n;
    return true;
}

// --------------------------------------------------------------------------
// serdata ops
// --------------------------------------------------------------------------

bool serdata_eqkey(const struct ddsi_serdata* /*a*/, const struct ddsi_serdata* /*b*/) {
    // Keyless: every sample belongs to the one instance, so all keys are equal.
    // `create_nros_sertype` refuses keyed descriptors, which is what makes this
    // a fact rather than an approximation.
    return true;
}

uint32_t serdata_get_size(const struct ddsi_serdata* dcmn) {
    return static_cast<uint32_t>(static_cast<const NrosSerdata*>(dcmn)->size);
}

void serdata_free(struct ddsi_serdata* dcmn) {
    delete static_cast<NrosSerdata*>(dcmn);
}

struct ddsi_serdata* serdata_from_ser(const struct ddsi_sertype* type,
                                      enum ddsi_serdata_kind kind,
                                      const struct nn_rdata* fragchain, size_t size) {
    OwnPtr<NrosSerdata> d(new (std::nothrow) NrosSerdata(type, kind));
    if (!d || !serdata_alloc(d.get(), size)) {
        return nullptr;
    }

    // Gather the fragment chain. Fragments may overlap, so advance by the
    // high-water mark rather than by each fragment's length — this is
    // upstream's loop (`serdata_rmw_from_ser`) and the reason the naive
    // "memcpy each fragment in turn" version is wrong.
    uint32_t off = 0;
    uint8_t* cursor = d->data;
    while (fragchain != nullptr) {
        if (fragchain->maxp1 > off) {
            const unsigned char* payload =
                NN_RMSG_PAYLOADOFF(fragchain->rmsg, NN_RDATA_PAYLOAD_OFF(fragchain));
            const unsigned char* src = payload + off - fragchain->min;
            const uint32_t n_bytes = fragchain->maxp1 - off;
            if (static_cast<size_t>(off) + n_bytes > size) {
                return nullptr;
            }
            std::memcpy(cursor, src, n_bytes);
            cursor += n_bytes;
            off = fragchain->maxp1;
        }
        fragchain = fragchain->nextfrag;
    }
    return d.release();
}

struct ddsi_serdata* serdata_from_ser_iov(const struct ddsi_sertype* type,
                                          enum ddsi_serdata_kind kind, ddsrt_msg_iovlen_t niov,
                                          const ddsrt_iovec_t* iov, size_t size) {
    OwnPtr<NrosSerdata> d(new (std::nothrow) NrosSerdata(type, kind));
    if (!d || !serdata_alloc(d.get(), size)) {
        return nullptr;
    }
    uint8_t* cursor = d->data;
    size_t written = 0;
    for (ddsrt_msg_iovlen_t i = 0; i < niov; i++) {
        if (written + iov[i].iov_len > size) {
            return nullptr;
        }
        std::memcpy(cursor, iov[i].iov_base, iov[i].iov_len);
        cursor += iov[i].iov_len;
        written += iov[i].iov_len;
    }
    return d.release();
}

struct ddsi_serdata* serdata_from_keyhash(const struct ddsi_sertype* /*type*/,
                                          const struct ddsi_keyhash* /*keyhash*/) {
    // Keyless: there is no key value to reconstruct. Upstream declines this one
    // too ("not even really needed for RTI compatibility anymore").
    return nullptr;
}

struct ddsi_serdata* serdata_from_sample(const struct ddsi_sertype* type,
                                         enum ddsi_serdata_kind kind, const void* sample) {
    OwnPtr<NrosSerdata> d(new (std::nothrow) NrosSerdata(type, kind));
    if (!d) {
        return nullptr;
    }
    // SDK_KEY on a keyless type carries no bytes: dispose/unregister still
    // needs a serdata, but there is nothing to put in it.
    if (kind != SDK_DATA) {
        return serdata_alloc(d.get(), 0) ? d.release() : nullptr;
    }
    const auto* blob = static_cast<const NrosCdrBlob*>(sample);
    if (blob == nullptr || blob->data == nullptr || blob->size == 0) {
        return nullptr;
    }
    if (!serdata_alloc(d.get(), blob->size)) {
        return nullptr;
    }
    std::memcpy(d->data, blob->data, blob->size);
    return d.release();
}

void serdata_to_ser(const struct ddsi_serdata* dcmn, size_t off, size_t sz, void* buf) {
    const auto* d = static_cast<const NrosSerdata*>(dcmn);
    std::memcpy(buf, d->data + off, sz);
}

struct ddsi_serdata* serdata_to_ser_ref(const struct ddsi_serdata* dcmn, size_t off, size_t sz,
                                        ddsrt_iovec_t* ref) {
    auto* d = const_cast<NrosSerdata*>(static_cast<const NrosSerdata*>(dcmn));
    ref->iov_base = d->data + off;
    ref->iov_len = static_cast<ddsrt_iov_len_t>(sz);
    return ddsi_serdata_ref(d);
}

void serdata_to_ser_unref(struct ddsi_serdata* dcmn, const ddsrt_iovec_t* /*ref*/) {
    ddsi_serdata_unref(static_cast<NrosSerdata*>(dcmn));
}

/// Hand the bytes back as a borrowed `NrosCdrBlob`.
///
/// Nothing in this backend reads samples through `dds_take`/`dds_read` — the
/// receive path is `dds_takecdr` (issue 0969). This op is filled so the type is
/// complete rather than because it is on a hot path, and it BORROWS: the span
/// is valid while the serdata is.
bool serdata_to_sample(const struct ddsi_serdata* dcmn, void* sample, void** /*bufptr*/,
                       void* /*buflim*/) {
    const auto* d = static_cast<const NrosSerdata*>(dcmn);
    auto* blob = static_cast<NrosCdrBlob*>(sample);
    if (blob == nullptr) {
        return false;
    }
    blob->data = d->data;
    blob->size = d->size;
    return true;
}

struct ddsi_serdata* serdata_to_untyped(const struct ddsi_serdata* dcmn) {
    // Used to map a key value onto an instance id. Keyless, so the untyped form
    // is an empty SDK_KEY serdata and every sample maps to the same instance.
    const auto* d = static_cast<const NrosSerdata*>(dcmn);
    auto* d1 = new (std::nothrow) NrosSerdata(d->type, SDK_KEY);
    if (d1 == nullptr) {
        return nullptr;
    }
    if (!serdata_alloc(d1, 0)) {
        delete d1;
        return nullptr;
    }
    d1->type = nullptr;
    return d1;
}

bool serdata_untyped_to_sample(const struct ddsi_sertype* /*type*/,
                               const struct ddsi_serdata* /*dcmn*/, void* sample,
                               void** /*bufptr*/, void* /*buflim*/) {
    // Keyless: an untyped serdata carries no key fields, so there is nothing to
    // write into the sample. Report success with the sample emptied rather than
    // leaving the caller's memory untouched and indeterminate.
    auto* blob = static_cast<NrosCdrBlob*>(sample);
    if (blob != nullptr) {
        blob->data = nullptr;
        blob->size = 0;
    }
    return true;
}

size_t serdata_print(const struct ddsi_sertype* /*type*/, const struct ddsi_serdata* dcmn,
                     char* buf, size_t bufsize) {
    const auto* d = static_cast<const NrosSerdata*>(dcmn);
    const int n = snprintf(buf, bufsize, "cdr:%zu", d->size);
    if (n < 0) {
        return 0;
    }
    return static_cast<size_t>(n) < bufsize ? static_cast<size_t>(n) : bufsize;
}

void serdata_get_keyhash(const struct ddsi_serdata* /*d*/, struct ddsi_keyhash* buf,
                         bool /*force_md5*/) {
    // Keyless: the keyhash of the single instance is all-zero.
    std::memset(buf, 0, sizeof(*buf));
}

const struct ddsi_serdata_ops nros_serdata_ops = {
    serdata_eqkey,
    serdata_get_size,
    serdata_from_ser,
    serdata_from_ser_iov,
    serdata_from_keyhash,
    serdata_from_sample,
    serdata_to_ser,
    serdata_to_ser_ref,
    serdata_to_ser_unref,
    serdata_to_sample,
    serdata_to_untyped,
    serdata_untyped_to_sample,
    serdata_free,
    serdata_print,
    serdata_get_keyhash,
};

// --------------------------------------------------------------------------
// sertype ops
// --------------------------------------------------------------------------

void sertype_free(struct ddsi_sertype* tpcmn) {
    auto* tp = static_cast<NrosSertype*>(tpcmn);
    ddsi_sertype_fini(tpcmn);
    delete tp;
}

// Sample management. Nothing in this backend takes this route — the data path
// is `dds_takecdr` on the way in and `dds_write` of a blob on the way out — so
// these three exist for the paths that reach a sertype without asking us:
// loans, dispose/unregister by instance handle, content filters. Upstream
// throws `std::logic_error` from `realloc_samples` for exactly that reason.
//
// They are implemented rather than stubbed anyway. A stub that leaves `ptrs`
// untouched hands the caller uninitialised pointers, which is a fault at some
// later and unrelated line; and this type's sample is a trivially copyable
// two-word struct, so doing it properly is the same amount of code as
// explaining why it was not done. Mirrors
// `ddsi_sertype_default`'s versions with `size = sizeof(NrosCdrBlob)`.

void sertype_zero_samples(const struct ddsi_sertype* /*d*/, void* samples, size_t count) {
    std::memset(samples, 0, sizeof(NrosCdrBlob) * count);
}

void sertype_realloc_samples(void** ptrs, const struct ddsi_sertype* /*d*/, void* old,
                             size_t oldcount, size_t count) {
    constexpr size_t size = sizeof(NrosCdrBlob);
    char* buf = static_cast<char*>((oldcount == count) ? old : dds_realloc(old, size * count));
    if (buf == nullptr) {
        return;
    }
    if (count > oldcount) {
        std::memset(buf + size * oldcount, 0, size * (count - oldcount));
    }
    for (size_t i = 0; i < count; i++) {
        ptrs[i] = buf + i * size;
    }
}

void sertype_free_samples(const struct ddsi_sertype* /*d*/, void** ptrs, size_t count,
                          dds_free_op_t op) {
    // The blob BORROWS its bytes from the serdata, so there is nothing per
    // sample to release — only the contiguous block itself, and only when the
    // caller asks for it.
    if (count > 0 && (op & DDS_FREE_ALL_BIT)) {
        dds_free(ptrs[0]);
    }
}

bool sertype_equal(const struct ddsi_sertype* acmn, const struct ddsi_sertype* bcmn) {
    // `ddsi_sertype::type_name` — Cyclone's own strdup'd copy. Same bytes the
    // derived `std::string` used to hold, so this compares what it compared.
    return std::strcmp(acmn->type_name, bcmn->type_name) == 0;
}

uint32_t sertype_hash(const struct ddsi_sertype* tpcmn) {
    // FNV-1a over the type name. The name is the whole identity of this type —
    // there is no typesupport to fold in — so it is what the hash must cover.
    //
    // Over `ddsi_sertype::type_name` now rather than a derived `std::string`.
    // Identical bytes and identical order, so the hash values are unchanged —
    // which matters, because a sertype hash that shifted would silently stop
    // matching remote types.
    uint32_t h = 2166136261u;
    for (const char* c = tpcmn->type_name; *c != '\0'; ++c) {
        h ^= static_cast<uint8_t>(*c);
        h *= 16777619u;
    }
    return h;
}

const struct ddsi_sertype_ops nros_sertype_ops = {
    ddsi_sertype_v0,
    nullptr,
    sertype_free,
    sertype_zero_samples,
    sertype_realloc_samples,
    sertype_free_samples,
    sertype_equal,
    sertype_hash,
    // type id / type map / type info / derive: no XTypes type object, the same
    // choice upstream makes when Cyclone is built without the type library.
    // Remote matching is by type NAME, which is unchanged from
    // `dds_create_topic(desc)`.
    nullptr,
    nullptr,
    nullptr,
    nullptr,
    // get_serialized_size / serialize_into: these exist for Cyclone's own
    // shared-memory and durability paths, which take a typed sample. Ours is
    // already serialized; nothing that runs here calls them.
    nullptr,
    nullptr,
};

} // namespace

struct ddsi_sertype* create_nros_sertype(const dds_topic_descriptor_t* desc) {
    if (desc == nullptr || desc->m_typename == nullptr) {
        return nullptr;
    }
    // Keyed types are refused rather than approximated — see the header. A
    // keyless sertype under a keyed type would alias every instance into one,
    // and would do it silently.
    if (desc->m_nkeys != 0) {
        return nullptr;
    }

    auto* st = new (std::nothrow) NrosSertype();
    if (st == nullptr) {
        return nullptr;
    }
    // Straight from the descriptor: `ddsi_sertype_init_flags` strdups it, so
    // nothing here needs to own or outlive the string.
    ddsi_sertype_init_flags(static_cast<struct ddsi_sertype*>(st), desc->m_typename,
                            &nros_sertype_ops, &nros_serdata_ops,
                            DDSI_SERTYPE_FLAG_TOPICKIND_NO_KEY);
    return static_cast<struct ddsi_sertype*>(st);
}

} // namespace nros_rmw_cyclonedds
