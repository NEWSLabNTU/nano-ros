//! What a metadata probe learns about the descriptor prose the code writes.
//!
//! Two knobs size the prose in every parameter store slot:
//! `NROS_PARAMS_MAX_DESCRIPTION_BYTES` for a descriptor's `description` and
//! `NROS_PARAMS_MAX_CONSTRAINTS_BYTES` for its `additional_constraints`. The
//! contract's `params:` rows state a type and nothing else, so the only source
//! for how long those texts are is the code itself, run by the probe.
//!
//! Every route that gives a parameter prose ends in
//! [`ParameterDescriptor::set_description`](crate::ParameterDescriptor::set_description)
//! or
//! [`ParameterDescriptor::set_additional_constraints`](crate::ParameterDescriptor::set_additional_constraints):
//! the typed builder's `.description(..)` / `.constraints(..)`, a descriptor
//! the code builds itself, and the C and C++ `add_parameter_description`
//! calls. So those two report the length the CODE asked for, before any
//! truncation, and nothing else has to.
//!
//! The facts are image-wide, like the knobs: the longest text decides the
//! buffer, whichever node wrote it, so no node is named.
//!
//! # Not compiled unless a probe asks for it
//!
//! Behind `descriptor-text-observer`, which only `nros`'s `metadata-mode`
//! turns on (through `nros-node/registration-observer`). A shipped image
//! carries neither the global nor the call.

use core::sync::atomic::{AtomicPtr, Ordering};

// Issue 2686 -- defined beside the descriptor, where the truncation report
// uses it too.
/// Which descriptor text was set. Each has its own knob.
pub use crate::types::DescriptorText;

/// A sink a metadata probe installs to hear the byte length of every
/// description or constraints text the code sets.
pub type DescriptorTextObserver = fn(DescriptorText, usize);

/// The installed sink, stored as `*mut ()` because a function pointer cannot
/// be an `AtomicPtr`'s pointee. Null is "nothing listening".
static OBSERVER: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Install the sink. The last caller wins. Only `nros::metadata_mode` calls
/// it: two sinks would be two answers about one text.
pub fn set_descriptor_text_observer(observer: DescriptorTextObserver) {
    OBSERVER.store(observer as *mut (), Ordering::Release);
}

/// Remove the sink.
pub fn clear_descriptor_text_observer() {
    OBSERVER.store(core::ptr::null_mut(), Ordering::Release);
}

/// Report one text's length, in bytes. A no-op when nothing is listening.
pub(crate) fn observe_descriptor_text(text: DescriptorText, len: usize) {
    let raw = OBSERVER.load(Ordering::Acquire);
    if raw.is_null() {
        return;
    }
    // SAFETY: the only non-null value ever stored is a `DescriptorTextObserver`
    // cast to `*mut ()` by `set_descriptor_text_observer`.
    let sink: DescriptorTextObserver = unsafe { core::mem::transmute(raw) };
    sink(text, len);
}
