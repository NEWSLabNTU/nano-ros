// nros-cpp: Stream<T> -- multi-shot message receiver
// Freestanding C++ -- no exceptions, no STL required

#ifndef NROS_CPP_STREAM_HPP
#define NROS_CPP_STREAM_HPP

#include <cstdint>
#include <cstddef>

#include "nros/result.hpp"

// FFI declarations
extern "C" {
typedef int nros_cpp_ret_t;
nros_cpp_ret_t nros_cpp_spin_once(void* handle, int32_t timeout_ms);
}

namespace nros {

/// Multi-shot message receiver for subscriptions and feedback streams.
///
/// Unlike Future<T> (single-shot, move-only), Stream<T> yields
/// multiple values over time. It wraps a non-blocking poll function
/// and adds wait_next() for blocking reception with executor spin.
///
/// Usage:
/// ```cpp
/// auto& stream = sub.stream();
/// MsgType msg;
/// NROS_TRY(stream.wait_next(executor.handle(), 5000, msg));
/// ```
template <typename T> class Stream {
  public:
    /// Try to receive the next value (non-blocking).
    ///
    /// @param out  Output object (filled on success).
    /// @return true if a value was received and deserialized.
    bool try_next(T& out) {
        if (!try_recv_fn_) return false;
        uint8_t buf[T::SERIALIZED_SIZE_MAX];
        size_t len = 0;
        nros_cpp_ret_t ret = try_recv_fn_(storage_, buf, sizeof(buf), &len);
        if (ret != 0 || len == 0) return false;
        return T::ffi_deserialize(buf, len, &out) == 0;
    }

    /// Block until the next value arrives, spinning the executor.
    ///
    /// @param executor_handle  Raw executor handle.
    /// @param timeout_ms       Maximum wait time in milliseconds.
    /// @param out              Output object (filled on success).
    /// @return Result::success(), ErrorCode::Timeout, or error.
    Result wait_next(void* executor_handle, uint32_t timeout_ms, T& out) {
        if (!try_recv_fn_) return Result(ErrorCode::NotInitialized);
        uint32_t elapsed = 0;
        while (elapsed < timeout_ms) {
            uint32_t step = 10;
            if (elapsed + step > timeout_ms) step = timeout_ms - elapsed;
            nros_cpp_spin_once(executor_handle, static_cast<int32_t>(step));
            if (try_next(out)) return Result::success();
            elapsed += step;
        }
        return Result(ErrorCode::Timeout);
    }

    /// Check if the stream is connected to a valid source.
    bool is_valid() const { return try_recv_fn_ != nullptr; }

    // Move semantics (non-copyable)
    Stream(Stream&& other) noexcept : storage_(other.storage_), try_recv_fn_(other.try_recv_fn_) {
        other.storage_ = nullptr;
        other.try_recv_fn_ = nullptr;
    }

    Stream& operator=(Stream&& other) noexcept {
        if (this != &other) {
            storage_ = other.storage_;
            try_recv_fn_ = other.try_recv_fn_;
            other.storage_ = nullptr;
            other.try_recv_fn_ = nullptr;
        }
        return *this;
    }

    /// Default constructor -- creates an unbound stream.
    Stream() : storage_(nullptr), try_recv_fn_(nullptr) {}

  private:
    Stream(const Stream&) = delete;
    Stream& operator=(const Stream&) = delete;

    template <typename M> friend class Subscription;
    template <typename A> friend class ActionClient;

    using TryRecvFn = nros_cpp_ret_t (*)(void*, uint8_t*, size_t, size_t*);

    Stream(void* storage, TryRecvFn fn) : storage_(storage), try_recv_fn_(fn) {}

    void bind(void* storage, TryRecvFn fn) {
        storage_ = storage;
        try_recv_fn_ = fn;
    }

    void* storage_;
    TryRecvFn try_recv_fn_;
};

} // namespace nros
#endif // NROS_CPP_STREAM_HPP
