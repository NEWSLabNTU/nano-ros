//! Issue 0778 — a client can learn which request a reply answers.
//!
//! `send_request` hands back the id the backend assigned and `take_response`
//! reports the id the reply belongs to. Without both halves a client with two
//! calls outstanding has nothing to correlate by, which is why each backend had
//! invented a policy: cyclonedds abandoned the older request, zenoh took the
//! first reply on the grounds that "a queryable is idempotent at the
//! application layer" — false for `send_goal` and `SetParameters`, which travel
//! this path.
#![cfg(feature = "alloc")]

use generated::{rmw_client_t, rmw_ret_t};
use nros_rmw_cffi::{NROS_RMW_RET_OK, generated};

/// A client that hands out ids 100, 101, … and answers the SECOND request
/// first — the interleaving the old ABI could not express.
mod backend {
    use super::*;
    use core::sync::atomic::{AtomicI64, Ordering};

    pub static NEXT: AtomicI64 = AtomicI64::new(100);
    pub static ANSWER: AtomicI64 = AtomicI64::new(-1);

    pub unsafe extern "C" fn send_request(
        _c: *const rmw_client_t,
        _d: *const u8,
        _n: usize,
        sequence_id: *mut i64,
    ) -> rmw_ret_t {
        let seq = NEXT.fetch_add(1, Ordering::SeqCst);
        unsafe { *sequence_id = seq };
        NROS_RMW_RET_OK
    }

    pub unsafe extern "C" fn take_response(
        _c: *const rmw_client_t,
        buf: *mut u8,
        cap: usize,
        seq_out: *mut i64,
        out_len: *mut usize,
        taken: *mut bool,
    ) -> rmw_ret_t {
        let answer = ANSWER.swap(-1, Ordering::SeqCst);
        if answer < 0 {
            unsafe { *taken = false };
            return NROS_RMW_RET_OK;
        }
        assert!(cap >= 4, "test buffer too small");
        unsafe {
            core::ptr::write_bytes(buf, 0, 4);
            *out_len = 4;
            *seq_out = answer;
            *taken = true;
        }
        NROS_RMW_RET_OK
    }
}

#[test]
fn a_reply_names_the_request_it_answers() {
    use core::sync::atomic::Ordering;

    // Drive the vtable slots directly: this is an ABI contract test, and going
    // through a session would only add a registry the contract does not
    // involve.
    let send = backend::send_request;
    let take = backend::take_response;

    let client = rmw_client_t {
        service_name: c"/add".as_ptr(),
        type_name: c"example_interfaces/srv/AddTwoInts".as_ptr(),
        _reserved: [0u8; 8],
        backend_data: core::ptr::dangling_mut::<core::ffi::c_void>(),
    };

    let mut first: i64 = -1;
    let mut second: i64 = -1;
    let rc = unsafe { send(&client, [0u8; 4].as_ptr(), 4, &mut first) };
    assert_eq!(rc, NROS_RMW_RET_OK);
    let rc = unsafe { send(&client, [0u8; 4].as_ptr(), 4, &mut second) };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert_ne!(
        first, second,
        "two sends must get two ids — one id for both is the bug"
    );

    // The SECOND request answers first. Under the old ABI the caller saw only
    // "a reply arrived" and had to assume it belonged to the older call.
    backend::ANSWER.store(second, Ordering::SeqCst);
    let mut buf = [0u8; 64];
    let mut got_seq: i64 = -1;
    let mut len = 0usize;
    let mut taken = false;
    let rc = unsafe {
        take(
            &client,
            buf.as_mut_ptr(),
            buf.len(),
            &mut got_seq,
            &mut len,
            &mut taken,
        )
    };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert!(taken);
    assert_eq!(
        got_seq, second,
        "the reply must name the request it answers, not the oldest outstanding one"
    );
    assert_ne!(got_seq, first);

    // Nothing outstanding: taken = false with OK, not an error.
    let rc = unsafe {
        take(
            &client,
            buf.as_mut_ptr(),
            buf.len(),
            &mut got_seq,
            &mut len,
            &mut taken,
        )
    };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert!(!taken);
}
