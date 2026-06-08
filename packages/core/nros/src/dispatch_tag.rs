//! Phase 216.A.4 — tag types Node authors hold on `Self::State` to
//! match against incoming [`Callback`](crate::Callback) in
//! [`ExecutableNode::on_callback`](crate::ExecutableNode::on_callback).
//!
//! Each tag is an opaque newtype around a `&'static str` (the stable
//! callback identifier shape — see
//! [`CallbackId`]). The three flavors
//! ([`SubscriptionTag`], [`ServiceTag`], [`ActionTag`]) keep the kind
//! distinct at the type level so a Node author can't accidentally match
//! a subscription tag against a service callback.
//!
//! Tag types support:
//!
//! - [`Into<CallbackId<'static>>`] — convert a tag into a borrowable
//!   `CallbackId` when handing off to the runtime.
//! - [`PartialEq<Callback<'_>>`](crate::Callback) — match directly against the
//!   [`Callback`](crate::Callback) delivered to
//!   [`ExecutableNode::on_callback`](crate::ExecutableNode::on_callback):
//!
//!   ```ignore
//!   if state.sub_chatter == cb { /* … */ }
//!   ```
//!
//! - [`placeholder`](SubscriptionTag::placeholder) — const-fn sentinel
//!   used by macro-emitted `init()` bodies before the real tag is
//!   resolved (the macro overwrites at register time).
//!
//! The companion `NodeContext::create_subscription_static` /
//! `_service_static` / `_action_static` methods that consume these tags
//! at register time land in a follow-up commit; this commit ships only
//! the types so the 216.B.5 + 216.C.5 example carving can declare them.

use crate::{node::Callback, node_metadata::CallbackId};

/// Tag identifying a subscription callback registered on a Node.
///
/// Stored on `Self::State` by macro-emitted `init()` bodies (or hand-
/// written equivalents) and matched against the [`Callback`] delivered to
/// delivered to
/// [`ExecutableNode::on_callback`](crate::ExecutableNode::on_callback).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubscriptionTag(&'static str);

impl SubscriptionTag {
    /// Sentinel used by macro-emitted `init()` bodies before the real
    /// tag is resolved. The macro overwrites at register time.
    pub const fn placeholder() -> Self {
        Self("")
    }

    /// Construct a tag with an explicit stable identifier.
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// Borrow the underlying identifier string.
    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

impl From<SubscriptionTag> for CallbackId<'static> {
    fn from(tag: SubscriptionTag) -> Self {
        CallbackId(tag.0)
    }
}

impl PartialEq<CallbackId<'_>> for SubscriptionTag {
    fn eq(&self, other: &CallbackId<'_>) -> bool {
        self.0 == other.0
    }
}

impl PartialEq<Callback<'_>> for SubscriptionTag {
    fn eq(&self, other: &Callback<'_>) -> bool {
        self.0 == other.as_str()
    }
}

/// Tag identifying a service-server callback registered on a Node.
///
/// See [`SubscriptionTag`] for the usage pattern.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServiceTag(&'static str);

impl ServiceTag {
    /// Sentinel used by macro-emitted `init()` bodies before the real
    /// tag is resolved.
    pub const fn placeholder() -> Self {
        Self("")
    }

    /// Construct a tag with an explicit stable identifier.
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// Borrow the underlying identifier string.
    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

impl From<ServiceTag> for CallbackId<'static> {
    fn from(tag: ServiceTag) -> Self {
        CallbackId(tag.0)
    }
}

impl PartialEq<CallbackId<'_>> for ServiceTag {
    fn eq(&self, other: &CallbackId<'_>) -> bool {
        self.0 == other.0
    }
}

impl PartialEq<Callback<'_>> for ServiceTag {
    fn eq(&self, other: &Callback<'_>) -> bool {
        self.0 == other.as_str()
    }
}

/// Tag identifying an action-server callback registered on a Node.
///
/// See [`SubscriptionTag`] for the usage pattern.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActionTag(&'static str);

impl ActionTag {
    /// Sentinel used by macro-emitted `init()` bodies before the real
    /// tag is resolved.
    pub const fn placeholder() -> Self {
        Self("")
    }

    /// Construct a tag with an explicit stable identifier.
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// Borrow the underlying identifier string.
    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

impl From<ActionTag> for CallbackId<'static> {
    fn from(tag: ActionTag) -> Self {
        CallbackId(tag.0)
    }
}

impl PartialEq<CallbackId<'_>> for ActionTag {
    fn eq(&self, other: &CallbackId<'_>) -> bool {
        self.0 == other.0
    }
}

impl PartialEq<Callback<'_>> for ActionTag {
    fn eq(&self, other: &Callback<'_>) -> bool {
        self.0 == other.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_returns_empty_str() {
        assert_eq!(SubscriptionTag::placeholder().as_str(), "");
        assert_eq!(ServiceTag::placeholder().as_str(), "");
        assert_eq!(ActionTag::placeholder().as_str(), "");
    }

    #[test]
    fn tag_into_callback_id_round_trips() {
        let sub = SubscriptionTag::new("sub_chatter");
        let svc = ServiceTag::new("svc_add_two_ints");
        let act = ActionTag::new("act_fibonacci");

        let sub_cb: CallbackId<'static> = sub.into();
        let svc_cb: CallbackId<'static> = svc.into();
        let act_cb: CallbackId<'static> = act.into();

        assert_eq!(sub_cb.as_str(), "sub_chatter");
        assert_eq!(svc_cb.as_str(), "svc_add_two_ints");
        assert_eq!(act_cb.as_str(), "act_fibonacci");
    }

    #[test]
    fn tag_eq_callback_id_matches() {
        let sub = SubscriptionTag::new("sub_chatter");
        let other = CallbackId::new("sub_chatter");
        let mismatch = CallbackId::new("sub_other");

        assert!(sub == other);
        assert!(!(sub == mismatch));

        let svc = ServiceTag::new("svc_add_two_ints");
        assert!(svc == CallbackId::new("svc_add_two_ints"));
        assert!(!(svc == CallbackId::new("svc_other")));

        let act = ActionTag::new("act_fibonacci");
        assert!(act == CallbackId::new("act_fibonacci"));
        assert!(!(act == CallbackId::new("act_other")));

        // Placeholder shouldn't match a real callback id.
        assert!(!(SubscriptionTag::placeholder() == CallbackId::new("sub_chatter")));
    }

    #[test]
    fn tag_eq_callback_event_matches() {
        let sub = SubscriptionTag::new("sub_chatter");
        let event = Callback::__from_id(CallbackId::new("sub_chatter"));
        let mismatch = Callback::__from_id(CallbackId::new("sub_other"));

        assert!(sub == event);
        assert!(!(sub == mismatch));
        assert!(ServiceTag::new("svc") == Callback::__from_id(CallbackId::new("svc")));
        assert!(ActionTag::new("act") == Callback::__from_id(CallbackId::new("act")));
    }
}
