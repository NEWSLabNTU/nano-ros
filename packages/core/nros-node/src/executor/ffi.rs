//! The executor's FFI surface (phase-515 R9, decision D4).
//!
//! Methods that exist for `nros-c` and `nros-cpp` -- the C-shaped entry points
//! (raw pointers, `extern "C"` callbacks, `unsafe` release paths) and the
//! plumbing those shims reach into -- are not part of the Rust user API. They
//! live here, behind a SEALED trait that the shims import and `nros` never
//! re-exports, so a Rust user neither sees them in the `nros` docs nor can
//! call them through `nros`. Each one is a crate-private inherent method of
//! [`Executor`]; this trait forwards to it, and nothing else implements it.
//!
//! Stage 2 does the same for the untyped action machinery: every method of
//! [`ActionServerCore`], [`ActionClientCore`], [`ActionServerRawHandle`] and
//! [`ActionClientRawHandle`] sits behind its own sealed `*Ffi` trait below. The
//! types stay public, because the sizes probe and the shims' storage name them.
//!
//! Not `#[doc(hidden)]`: that leaves a method callable and semver-visible.
#![allow(clippy::too_many_arguments, clippy::missing_safety_doc)]

use core::mem::MaybeUninit;

use nros_core::{GoalId, GoalStatus, Time};
use nros_rmw::QoSProfile;

use super::{
    action_core::{PendingCancelRequest, RawGoalRequest},
    *,
};
use crate::{session, timer::TimerDuration};

mod sealed {
    pub trait Sealed {}
}
impl sealed::Sealed for Executor<'_> {}

/// The executor's FFI entry points, for `nros-c` and `nros-cpp`. Sealed.
pub trait ExecutorFfi<'s>: sealed::Sealed + Sized {
    /// The FFI entry point; see `Executor::add_arena_subscription_c_callback` in `spin.rs`.
    #[allow(clippy::too_many_arguments)]
    fn add_arena_subscription_c_callback<const RX_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QoSProfile,
        callback: RawSubscriptionCallback,
        context: *mut core::ffi::c_void,
        group: Option<&str>,
        rx_buffer_hint: usize,
    ) -> Result<HandleId, NodeError>;

    /// The FFI entry point; see `Executor::add_arena_subscription_c_callback_with_capture` in `spin.rs`.
    #[allow(clippy::too_many_arguments)]
    fn add_arena_subscription_c_callback_with_capture<const RX_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QoSProfile,
        callback: RawSubscriptionCallback,
        context: *mut core::ffi::c_void,
        group: Option<&str>,
        rx_buffer_hint: usize,
        capture: Option<&[u8]>,
        capture_drop: Option<unsafe extern "C" fn(*mut core::ffi::c_void)>,
    ) -> Result<HandleId, NodeError>;

    /// The FFI entry point; see `Executor::add_arena_subscription_c_info_callback` in `spin.rs`.
    #[allow(clippy::too_many_arguments)]
    fn add_arena_subscription_c_info_callback<const RX_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QoSProfile,
        callback: RawSubscriptionInfoCallback,
        context: *mut core::ffi::c_void,
        rx_buffer_hint: usize,
    ) -> Result<HandleId, NodeError>;

    /// The FFI entry point; see `Executor::add_arena_subscription_c_typed_callback` in `spin.rs`.
    #[allow(clippy::too_many_arguments)]
    unsafe fn add_arena_subscription_c_typed_callback<const RX_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QoSProfile,
        msg: core::ptr::NonNull<core::ffi::c_void>,
        deserialize: RawMessageDeserializeFn,
        callback: TypedSubscriptionCallback,
        context: *mut core::ffi::c_void,
        group: Option<&str>,
        rx_buffer_hint: usize,
    ) -> Result<HandleId, NodeError>;

    /// The FFI entry point; see `Executor::add_arena_subscription_c_validated_callback` in `spin.rs`.
    #[cfg(feature = "safety-e2e")]
    #[allow(clippy::too_many_arguments)]
    fn add_arena_subscription_c_validated_callback<const RX_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QoSProfile,
        callback: super::types::RawSubscriptionSafetyCallback,
        context: *mut core::ffi::c_void,
        rx_buffer_hint: usize,
    ) -> Result<HandleId, NodeError>;

    /// The FFI entry point; see `Executor::add_on_shutdown_callback` in `spin.rs`.
    unsafe fn add_on_shutdown_callback(
        &mut self,
        callback: super::types::ShutdownCallbackFn,
        context: *mut core::ffi::c_void,
    ) -> Result<super::types::ShutdownCallbackHandle, NodeError>;

    /// The FFI entry point; see `Executor::add_pre_shutdown_callback` in `spin.rs`.
    unsafe fn add_pre_shutdown_callback(
        &mut self,
        callback: super::types::ShutdownCallbackFn,
        context: *mut core::ffi::c_void,
    ) -> Result<super::types::ShutdownCallbackHandle, NodeError>;

    /// The FFI entry point; see `Executor::remove_pre_shutdown_callback` in `spin.rs`.
    fn remove_pre_shutdown_callback(
        &mut self,
        handle: super::types::ShutdownCallbackHandle,
    ) -> bool;

    /// The FFI entry point; see `Executor::remove_on_shutdown_callback` in `spin.rs`.
    fn remove_on_shutdown_callback(&mut self, handle: super::types::ShutdownCallbackHandle)
    -> bool;

    /// The FFI entry point; see `Executor::from_session_ptr_in` in `spin.rs`.
    unsafe fn from_session_ptr_in(
        session_ptr: *mut session::ConcreteSession,
        backing: &'s mut [MaybeUninit<u64>],
        sizing: super::storage::ExecutorSizing,
    ) -> Self;

    /// The FFI entry point; see `Executor::open_multi_in` in `spin.rs`.
    #[cfg(feature = "rmw-cffi")]
    unsafe fn open_multi_in(
        specs: &[SessionSpec<'_>],
        backing: &'s mut [MaybeUninit<u64>],
        sizing: super::storage::ExecutorSizing,
    ) -> Result<Self, NodeError>;

    /// The FFI entry point; see `Executor::open_with_session_handle_in` in `spin.rs`.
    unsafe fn open_with_session_handle_in(
        handle: SessionHandle,
        backing: &'s mut [MaybeUninit<u64>],
        sizing: super::storage::ExecutorSizing,
    ) -> Self;

    /// The FFI entry point; see `Executor::register_service_client_raw_sized` in `spin.rs`.
    #[allow(clippy::too_many_arguments)]
    fn register_service_client_raw_sized<const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QoSProfile,
        callback: Option<RawResponseCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError>;

    /// The FFI entry point; see `Executor::register_service_raw_capturing` in `spin.rs`.
    #[allow(clippy::too_many_arguments)]
    fn register_service_raw_capturing<const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QoSProfile,
        callback: RawServiceCallback,
        capture: Option<&[u8]>,
        capture_drop: Option<unsafe extern "C" fn(*mut core::ffi::c_void)>,
    ) -> Result<HandleId, NodeError>;

    /// The FFI entry point; see `Executor::register_service_raw_sized` in `spin.rs`.
    #[allow(clippy::too_many_arguments)]
    fn register_service_raw_sized<const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QoSProfile,
        callback: RawServiceCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError>;

    /// The FFI entry point; see `Executor::register_timer_c_capturing` in `spin.rs`.
    fn register_timer_c_capturing(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        period: TimerDuration,
        source: TimerClockSource,
        callback: unsafe extern "C" fn(*mut core::ffi::c_void),
        capture: Option<&[u8]>,
        capture_drop: Option<unsafe extern "C" fn(*mut core::ffi::c_void)>,
    ) -> Result<HandleId, NodeError>;

    /// The FFI entry point; see `Executor::register_timer_c_capturing_with_info` in `spin.rs`.
    #[allow(clippy::too_many_arguments)]
    fn register_timer_c_capturing_with_info<I>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        period: TimerDuration,
        source: TimerClockSource,
        autostart: bool,
        group: Option<&str>,
        callback: unsafe extern "C" fn(*const I, *mut core::ffi::c_void),
        capture: Option<&[u8]>,
        capture_drop: Option<unsafe extern "C" fn(*mut core::ffi::c_void)>,
    ) -> Result<HandleId, NodeError>
    where
        I: From<TimerCallInfo> + 'static;

    /// The FFI entry point; see `Executor::release_action_client_raw` in `action.rs`.
    unsafe fn release_action_client_raw(&mut self, entry_index: usize) -> bool;

    /// The FFI entry point; see `Executor::release_action_server_raw` in `action.rs`.
    unsafe fn release_action_server_raw(&mut self, handle: &ActionServerRawHandle) -> bool;

    /// The FFI entry point; see `Executor::release_node` in `spin.rs`.
    unsafe fn release_node(&mut self, node: crate::executor::NodeId) -> usize;

    /// The FFI entry point; see `Executor::release_service` in `spin.rs`.
    unsafe fn release_service(&mut self, handle: HandleId) -> bool;

    /// The FFI entry point; see `Executor::release_service_client` in `spin.rs`.
    unsafe fn release_service_client(&mut self, handle: HandleId) -> bool;

    /// The FFI entry point; see `Executor::release_subscription` in `spin.rs`.
    unsafe fn release_subscription(&mut self, handle: HandleId) -> bool;

    /// The FFI entry point; see `Executor::release_timer` in `spin.rs`.
    unsafe fn release_timer(&mut self, handle: HandleId) -> bool;

    /// The FFI entry point; see `Executor::retarget_action_client_raw` in `action.rs`.
    unsafe fn retarget_action_client_raw(
        &mut self,
        entry_index: usize,
        context: *mut core::ffi::c_void,
    ) -> bool;

    /// The FFI entry point; see `Executor::retarget_action_server_raw` in `action.rs`.
    unsafe fn retarget_action_server_raw(
        &mut self,
        handle: &ActionServerRawHandle,
        context: *mut core::ffi::c_void,
    ) -> bool;

    /// The FFI entry point; see `Executor::service_client_handle` in `spin.rs`.
    #[doc(hidden)]
    unsafe fn service_client_handle(
        &self,
        entry_index: usize,
    ) -> Option<&session::RmwServiceClient>;

    /// The FFI entry point; see `Executor::service_server_handle` in `spin.rs`.
    #[doc(hidden)]
    unsafe fn service_server_handle(
        &self,
        entry_index: usize,
    ) -> Option<&session::RmwServiceServer>;

    /// The FFI entry point; see `Executor::set_park_primitive` in `spin.rs`.
    fn set_park_primitive(
        &mut self,
        park: ParkUntilFn,
        ctx: *mut core::ffi::c_void,
        granularity_us: u64,
    );

    /// The FFI entry point; see `Executor::subscription_handle` in `spin.rs`.
    #[doc(hidden)]
    unsafe fn subscription_handle(&self, entry_index: usize) -> Option<&session::RmwSubscriber>;

    /// The FFI entry point; see `Executor::subscription_handle_mut` in `spin.rs`.
    #[doc(hidden)]
    unsafe fn subscription_handle_mut(
        &mut self,
        entry_index: usize,
    ) -> Option<&mut session::RmwSubscriber>;

    /// The FFI entry point; see `Executor::wake_raw_ptr` in `spin.rs`.
    #[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
    fn wake_raw_ptr(&self) -> *mut core::ffi::c_void;

    /// The FFI entry point; see `Executor::enter_spin_loop` in `spin.rs`.
    fn enter_spin_loop(&self);

    /// The FFI entry point; see `Executor::exit_spin_loop` in `spin.rs`.
    fn exit_spin_loop(&self);

    /// The FFI entry point; see `Executor::exec_lock` in `spin.rs`.
    fn exec_lock(&self) -> Option<super::exec_lock::ExecLock>;

    /// The FFI entry point; see `Executor::node_session_mut` in `spin.rs`.
    fn node_session_mut(
        &mut self,
        node_id: super::node_record::NodeId,
    ) -> Option<&mut session::ConcreteSession>;

    /// The FFI entry point; see `Executor::register_action_client_raw` in `action.rs`.
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    fn register_action_client_raw(
        &mut self,
        spec: RawActionClientSpec<'_>,
    ) -> Result<ActionClientRawHandle, NodeError>;

    /// The FFI entry point; see `Executor::register_action_server_raw` in `action.rs`.
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    fn register_action_server_raw(
        &mut self,
        spec: RawActionServerSpec<'_>,
    ) -> Result<ActionServerRawHandle, NodeError>;

    /// The FFI entry point; see `Executor::install_wake_signal_on_primary` in `spin.rs`.
    #[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
    fn install_wake_signal_on_primary(&mut self);

    /// The FFI entry point; see `Executor::set_entry_owner` in `spin.rs`.
    fn set_entry_owner(&mut self, id: HandleId, node: crate::executor::NodeId) -> bool;

    /// The FFI entry point; see `Executor::set_primary_identity` in `spin.rs`.
    fn set_primary_identity(&mut self, rmw_name: &str, locator: &str);

    /// The FFI entry point; see `Executor::session_io_failures` in `spin.rs`.
    fn session_io_failures(&self) -> u32;

    /// The FFI entry point; see `Executor::timer_elapsed_us` in `spin.rs`.
    fn timer_elapsed_us(&self, id: HandleId) -> Option<u64>;

    /// The FFI entry point; see `Executor::timer_is_ready` in `spin.rs`.
    fn timer_is_ready(&self, id: HandleId) -> Option<bool>;

    /// The FFI entry point; see `Executor::exchange_timer_period_us` in `spin.rs`.
    fn exchange_timer_period_us(&mut self, id: HandleId, new_period_us: u64) -> Option<u64>;

    /// The FFI entry point; see `Executor::action_client_core_mut` in `spin.rs`.
    #[doc(hidden)]
    unsafe fn action_client_core_mut(
        &mut self,
        entry_index: usize,
    ) -> Option<&mut super::action_core::ActionClientCore>;

    /// The FFI entry point; see `Executor::register_service_client_raw_sized_on` in `spin.rs`.
    #[allow(clippy::too_many_arguments)]
    fn register_service_client_raw_sized_on<const REPLY_BUF: usize>(
        &mut self,
        node_id: super::node_record::NodeId,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QoSProfile,
        callback: Option<RawResponseCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError>;

    /// The FFI entry point; see `Executor::register_service_raw_sized_on` in `spin.rs`.
    #[allow(clippy::too_many_arguments)]
    fn register_service_raw_sized_on<const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        node_id: super::node_record::NodeId,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QoSProfile,
        callback: RawServiceCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError>;

    /// The FFI entry point; see `Executor::service_client_entry_mut` in `spin.rs`.
    unsafe fn service_client_entry_mut(
        &mut self,
        entry_index: usize,
    ) -> Option<&mut super::arena::ServiceClientRawArenaEntry<{ crate::config::DEFAULT_RX_BUF_SIZE }>>;

    /// The FFI entry point; see `Executor::from_session_ptr` in `spin.rs`.
    #[cfg(feature = "alloc")]
    unsafe fn from_session_ptr(session_ptr: *mut session::ConcreteSession) -> Executor<'static>;

    /// The FFI entry point; see `Executor::from_session_in` in `spin.rs`.
    unsafe fn from_session_in(
        session: session::ConcreteSession,
        backing: &'s mut [MaybeUninit<u64>],
        sizing: super::storage::ExecutorSizing,
    ) -> Self;

    /// The FFI entry point; see `Executor::open_with_session_in` in `spin.rs`.
    unsafe fn open_with_session_in(
        session: *mut session::ConcreteSession,
        backing: &'s mut [MaybeUninit<u64>],
        sizing: super::storage::ExecutorSizing,
    ) -> Self;

    /// The FFI entry point; see `Executor::open_with_session_slot` in `spin.rs`.
    #[cfg(feature = "alloc")]
    unsafe fn open_with_session_slot(
        session: *mut session::ConcreteSession,
        slot: &'s mut super::TierExecutorBackingSlot,
    ) -> Self;

    /// The FFI entry point; see `Executor::open_with_session_handle_slot` in `spin.rs`.
    #[cfg(feature = "alloc")]
    unsafe fn open_with_session_handle_slot(
        handle: SessionHandle,
        slot: &'s mut super::TierExecutorBackingSlot,
    ) -> Self;

    /// The FFI entry point; see `Executor::detach_action_client_raw` in `action.rs`.
    unsafe fn detach_action_client_raw(&mut self, entry_index: usize) -> bool;

    /// The FFI entry point; see `Executor::detach_action_server_raw` in `action.rs`.
    unsafe fn detach_action_server_raw(&mut self, handle: &ActionServerRawHandle) -> bool;

    /// The FFI entry point; see `Executor::detach_action_server_raw_sized` in `action.rs`.
    unsafe fn detach_action_server_raw_sized<
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
        const MAX_GOALS: usize,
    >(
        &mut self,
        entry_index: usize,
    ) -> bool;

    /// The FFI entry point; see `Executor::release_action_client_raw_sized` in `action.rs`.
    unsafe fn release_action_client_raw_sized<
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
    >(
        &mut self,
        entry_index: usize,
    ) -> bool;

    /// The FFI entry point; see `Executor::release_action_server_raw_sized` in `action.rs`.
    unsafe fn release_action_server_raw_sized<
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
        const MAX_GOALS: usize,
    >(
        &mut self,
        entry_index: usize,
    ) -> bool;

    /// The FFI entry point; see `Executor::register_service_raw` in `spin.rs`.
    fn register_service_raw(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        callback: RawServiceCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError>;

    /// The FFI entry point; see `Executor::register_service_client_raw` in `spin.rs`.
    fn register_service_client_raw(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        callback: Option<RawResponseCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError>;

    /// The FFI entry point; see `Executor::register_action_client_core` in `action.rs`.
    #[doc(hidden)]
    fn register_action_client_core<
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
    >(
        &mut self,
        core: ActionClientCore<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>,
        goal_response_callback: Option<RawGoalResponseCallback>,
        feedback_callback: Option<RawFeedbackCallback>,
        result_callback: Option<RawResultCallback>,
        cancel_response_callback: Option<RawCancelResponseCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<ActionClientRawHandle, NodeError>;

    /// The FFI entry point; see `Executor::release_guard_condition` in `spin.rs`.
    unsafe fn release_guard_condition(&mut self, handle: HandleId) -> bool;
}

impl<'s> ExecutorFfi<'s> for Executor<'s> {
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn add_arena_subscription_c_callback<const RX_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QoSProfile,
        callback: RawSubscriptionCallback,
        context: *mut core::ffi::c_void,
        group: Option<&str>,
        rx_buffer_hint: usize,
    ) -> Result<HandleId, NodeError> {
        Executor::add_arena_subscription_c_callback::<RX_BUF>(
            self,
            node_id,
            topic_name,
            type_name,
            type_hash,
            qos,
            callback,
            context,
            group,
            rx_buffer_hint,
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn add_arena_subscription_c_callback_with_capture<const RX_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QoSProfile,
        callback: RawSubscriptionCallback,
        context: *mut core::ffi::c_void,
        group: Option<&str>,
        rx_buffer_hint: usize,
        capture: Option<&[u8]>,
        capture_drop: Option<unsafe extern "C" fn(*mut core::ffi::c_void)>,
    ) -> Result<HandleId, NodeError> {
        Executor::add_arena_subscription_c_callback_with_capture::<RX_BUF>(
            self,
            node_id,
            topic_name,
            type_name,
            type_hash,
            qos,
            callback,
            context,
            group,
            rx_buffer_hint,
            capture,
            capture_drop,
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn add_arena_subscription_c_info_callback<const RX_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QoSProfile,
        callback: RawSubscriptionInfoCallback,
        context: *mut core::ffi::c_void,
        rx_buffer_hint: usize,
    ) -> Result<HandleId, NodeError> {
        Executor::add_arena_subscription_c_info_callback::<RX_BUF>(
            self,
            node_id,
            topic_name,
            type_name,
            type_hash,
            qos,
            callback,
            context,
            rx_buffer_hint,
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    unsafe fn add_arena_subscription_c_typed_callback<const RX_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QoSProfile,
        msg: core::ptr::NonNull<core::ffi::c_void>,
        deserialize: RawMessageDeserializeFn,
        callback: TypedSubscriptionCallback,
        context: *mut core::ffi::c_void,
        group: Option<&str>,
        rx_buffer_hint: usize,
    ) -> Result<HandleId, NodeError> {
        unsafe {
            Executor::add_arena_subscription_c_typed_callback::<RX_BUF>(
                self,
                node_id,
                topic_name,
                type_name,
                type_hash,
                qos,
                msg,
                deserialize,
                callback,
                context,
                group,
                rx_buffer_hint,
            )
        }
    }

    #[cfg(feature = "safety-e2e")]
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn add_arena_subscription_c_validated_callback<const RX_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        topic_name: &str,
        type_name: &str,
        type_hash: &str,
        qos: QoSProfile,
        callback: super::types::RawSubscriptionSafetyCallback,
        context: *mut core::ffi::c_void,
        rx_buffer_hint: usize,
    ) -> Result<HandleId, NodeError> {
        Executor::add_arena_subscription_c_validated_callback::<RX_BUF>(
            self,
            node_id,
            topic_name,
            type_name,
            type_hash,
            qos,
            callback,
            context,
            rx_buffer_hint,
        )
    }

    #[inline]
    unsafe fn add_on_shutdown_callback(
        &mut self,
        callback: super::types::ShutdownCallbackFn,
        context: *mut core::ffi::c_void,
    ) -> Result<super::types::ShutdownCallbackHandle, NodeError> {
        unsafe { Executor::add_on_shutdown_callback(self, callback, context) }
    }

    #[inline]
    unsafe fn add_pre_shutdown_callback(
        &mut self,
        callback: super::types::ShutdownCallbackFn,
        context: *mut core::ffi::c_void,
    ) -> Result<super::types::ShutdownCallbackHandle, NodeError> {
        unsafe { Executor::add_pre_shutdown_callback(self, callback, context) }
    }

    #[inline]
    fn remove_pre_shutdown_callback(
        &mut self,
        handle: super::types::ShutdownCallbackHandle,
    ) -> bool {
        Executor::remove_pre_shutdown_callback(self, handle)
    }

    #[inline]
    fn remove_on_shutdown_callback(
        &mut self,
        handle: super::types::ShutdownCallbackHandle,
    ) -> bool {
        Executor::remove_on_shutdown_callback(self, handle)
    }

    #[inline]
    unsafe fn from_session_ptr_in(
        session_ptr: *mut session::ConcreteSession,
        backing: &'s mut [MaybeUninit<u64>],
        sizing: super::storage::ExecutorSizing,
    ) -> Self {
        unsafe { Executor::from_session_ptr_in(session_ptr, backing, sizing) }
    }

    #[cfg(feature = "rmw-cffi")]
    #[inline]
    unsafe fn open_multi_in(
        specs: &[SessionSpec<'_>],
        backing: &'s mut [MaybeUninit<u64>],
        sizing: super::storage::ExecutorSizing,
    ) -> Result<Self, NodeError> {
        unsafe { Executor::open_multi_in(specs, backing, sizing) }
    }

    #[inline]
    unsafe fn open_with_session_handle_in(
        handle: SessionHandle,
        backing: &'s mut [MaybeUninit<u64>],
        sizing: super::storage::ExecutorSizing,
    ) -> Self {
        unsafe { Executor::open_with_session_handle_in(handle, backing, sizing) }
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn register_service_client_raw_sized<const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QoSProfile,
        callback: Option<RawResponseCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        Executor::register_service_client_raw_sized::<REPLY_BUF>(
            self,
            service_name,
            service_type,
            service_hash,
            qos,
            callback,
            context,
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn register_service_raw_capturing<const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QoSProfile,
        callback: RawServiceCallback,
        capture: Option<&[u8]>,
        capture_drop: Option<unsafe extern "C" fn(*mut core::ffi::c_void)>,
    ) -> Result<HandleId, NodeError> {
        Executor::register_service_raw_capturing::<REQ_BUF, REPLY_BUF>(
            self,
            node_id,
            service_name,
            service_type,
            service_hash,
            qos,
            callback,
            capture,
            capture_drop,
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn register_service_raw_sized<const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QoSProfile,
        callback: RawServiceCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        Executor::register_service_raw_sized::<REQ_BUF, REPLY_BUF>(
            self,
            service_name,
            service_type,
            service_hash,
            qos,
            callback,
            context,
        )
    }

    #[inline]
    fn register_timer_c_capturing(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        period: TimerDuration,
        source: TimerClockSource,
        callback: unsafe extern "C" fn(*mut core::ffi::c_void),
        capture: Option<&[u8]>,
        capture_drop: Option<unsafe extern "C" fn(*mut core::ffi::c_void)>,
    ) -> Result<HandleId, NodeError> {
        Executor::register_timer_c_capturing(
            self,
            node_id,
            period,
            source,
            callback,
            capture,
            capture_drop,
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn register_timer_c_capturing_with_info<I>(
        &mut self,
        node_id: Option<super::node_record::NodeId>,
        period: TimerDuration,
        source: TimerClockSource,
        autostart: bool,
        group: Option<&str>,
        callback: unsafe extern "C" fn(*const I, *mut core::ffi::c_void),
        capture: Option<&[u8]>,
        capture_drop: Option<unsafe extern "C" fn(*mut core::ffi::c_void)>,
    ) -> Result<HandleId, NodeError>
    where
        I: From<TimerCallInfo> + 'static,
    {
        Executor::register_timer_c_capturing_with_info::<I>(
            self,
            node_id,
            period,
            source,
            autostart,
            group,
            callback,
            capture,
            capture_drop,
        )
    }

    #[inline]
    unsafe fn release_action_client_raw(&mut self, entry_index: usize) -> bool {
        unsafe { Executor::release_action_client_raw(self, entry_index) }
    }

    #[inline]
    unsafe fn release_action_server_raw(&mut self, handle: &ActionServerRawHandle) -> bool {
        unsafe { Executor::release_action_server_raw(self, handle) }
    }

    #[inline]
    unsafe fn release_node(&mut self, node: crate::executor::NodeId) -> usize {
        unsafe { Executor::release_node(self, node) }
    }

    #[inline]
    unsafe fn release_service(&mut self, handle: HandleId) -> bool {
        unsafe { Executor::release_service(self, handle) }
    }

    #[inline]
    unsafe fn release_service_client(&mut self, handle: HandleId) -> bool {
        unsafe { Executor::release_service_client(self, handle) }
    }

    #[inline]
    unsafe fn release_subscription(&mut self, handle: HandleId) -> bool {
        unsafe { Executor::release_subscription(self, handle) }
    }

    #[inline]
    unsafe fn release_timer(&mut self, handle: HandleId) -> bool {
        unsafe { Executor::release_timer(self, handle) }
    }

    #[inline]
    unsafe fn retarget_action_client_raw(
        &mut self,
        entry_index: usize,
        context: *mut core::ffi::c_void,
    ) -> bool {
        unsafe { Executor::retarget_action_client_raw(self, entry_index, context) }
    }

    #[inline]
    unsafe fn retarget_action_server_raw(
        &mut self,
        handle: &ActionServerRawHandle,
        context: *mut core::ffi::c_void,
    ) -> bool {
        unsafe { Executor::retarget_action_server_raw(self, handle, context) }
    }

    #[doc(hidden)]
    #[inline]
    unsafe fn service_client_handle(
        &self,
        entry_index: usize,
    ) -> Option<&session::RmwServiceClient> {
        unsafe { Executor::service_client_handle(self, entry_index) }
    }

    #[doc(hidden)]
    #[inline]
    unsafe fn service_server_handle(
        &self,
        entry_index: usize,
    ) -> Option<&session::RmwServiceServer> {
        unsafe { Executor::service_server_handle(self, entry_index) }
    }

    #[inline]
    fn set_park_primitive(
        &mut self,
        park: ParkUntilFn,
        ctx: *mut core::ffi::c_void,
        granularity_us: u64,
    ) {
        Executor::set_park_primitive(self, park, ctx, granularity_us)
    }

    #[doc(hidden)]
    #[inline]
    unsafe fn subscription_handle(&self, entry_index: usize) -> Option<&session::RmwSubscriber> {
        unsafe { Executor::subscription_handle(self, entry_index) }
    }

    #[doc(hidden)]
    #[inline]
    unsafe fn subscription_handle_mut(
        &mut self,
        entry_index: usize,
    ) -> Option<&mut session::RmwSubscriber> {
        unsafe { Executor::subscription_handle_mut(self, entry_index) }
    }

    #[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
    #[inline]
    fn wake_raw_ptr(&self) -> *mut core::ffi::c_void {
        Executor::wake_raw_ptr(self)
    }

    #[inline]
    fn enter_spin_loop(&self) {
        Executor::enter_spin_loop(self)
    }

    #[inline]
    fn exit_spin_loop(&self) {
        Executor::exit_spin_loop(self)
    }

    #[inline]
    fn exec_lock(&self) -> Option<super::exec_lock::ExecLock> {
        Executor::exec_lock(self)
    }

    #[inline]
    fn node_session_mut(
        &mut self,
        node_id: super::node_record::NodeId,
    ) -> Option<&mut session::ConcreteSession> {
        Executor::node_session_mut(self, node_id)
    }

    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn register_action_client_raw(
        &mut self,
        spec: RawActionClientSpec<'_>,
    ) -> Result<ActionClientRawHandle, NodeError> {
        Executor::register_action_client_raw(self, spec)
    }

    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn register_action_server_raw(
        &mut self,
        spec: RawActionServerSpec<'_>,
    ) -> Result<ActionServerRawHandle, NodeError> {
        Executor::register_action_server_raw(self, spec)
    }

    #[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
    #[inline]
    fn install_wake_signal_on_primary(&mut self) {
        Executor::install_wake_signal_on_primary(self)
    }

    #[inline]
    fn set_entry_owner(&mut self, id: HandleId, node: crate::executor::NodeId) -> bool {
        Executor::set_entry_owner(self, id, node)
    }

    #[inline]
    fn set_primary_identity(&mut self, rmw_name: &str, locator: &str) {
        Executor::set_primary_identity(self, rmw_name, locator)
    }

    #[inline]
    fn session_io_failures(&self) -> u32 {
        Executor::session_io_failures(self)
    }

    #[inline]
    fn timer_elapsed_us(&self, id: HandleId) -> Option<u64> {
        Executor::timer_elapsed_us(self, id)
    }

    #[inline]
    fn timer_is_ready(&self, id: HandleId) -> Option<bool> {
        Executor::timer_is_ready(self, id)
    }

    #[inline]
    fn exchange_timer_period_us(&mut self, id: HandleId, new_period_us: u64) -> Option<u64> {
        Executor::exchange_timer_period_us(self, id, new_period_us)
    }

    #[doc(hidden)]
    #[inline]
    unsafe fn action_client_core_mut(
        &mut self,
        entry_index: usize,
    ) -> Option<&mut super::action_core::ActionClientCore> {
        unsafe { Executor::action_client_core_mut(self, entry_index) }
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn register_service_client_raw_sized_on<const REPLY_BUF: usize>(
        &mut self,
        node_id: super::node_record::NodeId,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QoSProfile,
        callback: Option<RawResponseCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        Executor::register_service_client_raw_sized_on::<REPLY_BUF>(
            self,
            node_id,
            service_name,
            service_type,
            service_hash,
            qos,
            callback,
            context,
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn register_service_raw_sized_on<const REQ_BUF: usize, const REPLY_BUF: usize>(
        &mut self,
        node_id: super::node_record::NodeId,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        qos: QoSProfile,
        callback: RawServiceCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        Executor::register_service_raw_sized_on::<REQ_BUF, REPLY_BUF>(
            self,
            node_id,
            service_name,
            service_type,
            service_hash,
            qos,
            callback,
            context,
        )
    }

    #[inline]
    unsafe fn service_client_entry_mut(
        &mut self,
        entry_index: usize,
    ) -> Option<&mut super::arena::ServiceClientRawArenaEntry<{ crate::config::DEFAULT_RX_BUF_SIZE }>>
    {
        unsafe { Executor::service_client_entry_mut(self, entry_index) }
    }

    #[cfg(feature = "alloc")]
    #[inline]
    unsafe fn from_session_ptr(session_ptr: *mut session::ConcreteSession) -> Executor<'static> {
        unsafe { Executor::<'static>::from_session_ptr(session_ptr) }
    }

    #[inline]
    unsafe fn from_session_in(
        session: session::ConcreteSession,
        backing: &'s mut [MaybeUninit<u64>],
        sizing: super::storage::ExecutorSizing,
    ) -> Self {
        unsafe { Executor::from_session_in(session, backing, sizing) }
    }

    #[inline]
    unsafe fn open_with_session_in(
        session: *mut session::ConcreteSession,
        backing: &'s mut [MaybeUninit<u64>],
        sizing: super::storage::ExecutorSizing,
    ) -> Self {
        unsafe { Executor::open_with_session_in(session, backing, sizing) }
    }

    #[cfg(feature = "alloc")]
    #[inline]
    unsafe fn open_with_session_slot(
        session: *mut session::ConcreteSession,
        slot: &'s mut super::TierExecutorBackingSlot,
    ) -> Self {
        unsafe { Executor::open_with_session_slot(session, slot) }
    }

    #[cfg(feature = "alloc")]
    #[inline]
    unsafe fn open_with_session_handle_slot(
        handle: SessionHandle,
        slot: &'s mut super::TierExecutorBackingSlot,
    ) -> Self {
        unsafe { Executor::open_with_session_handle_slot(handle, slot) }
    }

    #[inline]
    unsafe fn detach_action_client_raw(&mut self, entry_index: usize) -> bool {
        unsafe { Executor::detach_action_client_raw(self, entry_index) }
    }

    #[inline]
    unsafe fn detach_action_server_raw(&mut self, handle: &ActionServerRawHandle) -> bool {
        unsafe { Executor::detach_action_server_raw(self, handle) }
    }

    #[inline]
    unsafe fn detach_action_server_raw_sized<
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
        const MAX_GOALS: usize,
    >(
        &mut self,
        entry_index: usize,
    ) -> bool {
        unsafe {
            Executor::detach_action_server_raw_sized::<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>(
                self,
                entry_index,
            )
        }
    }

    #[inline]
    unsafe fn release_action_client_raw_sized<
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
    >(
        &mut self,
        entry_index: usize,
    ) -> bool {
        unsafe {
            Executor::release_action_client_raw_sized::<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>(
                self,
                entry_index,
            )
        }
    }

    #[inline]
    unsafe fn release_action_server_raw_sized<
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
        const MAX_GOALS: usize,
    >(
        &mut self,
        entry_index: usize,
    ) -> bool {
        unsafe {
            Executor::release_action_server_raw_sized::<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>(
                self,
                entry_index,
            )
        }
    }

    #[inline]
    fn register_service_raw(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        callback: RawServiceCallback,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        Executor::register_service_raw(
            self,
            service_name,
            service_type,
            service_hash,
            callback,
            context,
        )
    }

    #[inline]
    fn register_service_client_raw(
        &mut self,
        service_name: &str,
        service_type: &str,
        service_hash: &str,
        callback: Option<RawResponseCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<HandleId, NodeError> {
        Executor::register_service_client_raw(
            self,
            service_name,
            service_type,
            service_hash,
            callback,
            context,
        )
    }

    #[doc(hidden)]
    #[inline]
    fn register_action_client_core<
        const GOAL_BUF: usize,
        const RESULT_BUF: usize,
        const FEEDBACK_BUF: usize,
    >(
        &mut self,
        core: ActionClientCore<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>,
        goal_response_callback: Option<RawGoalResponseCallback>,
        feedback_callback: Option<RawFeedbackCallback>,
        result_callback: Option<RawResultCallback>,
        cancel_response_callback: Option<RawCancelResponseCallback>,
        context: *mut core::ffi::c_void,
    ) -> Result<ActionClientRawHandle, NodeError> {
        Executor::register_action_client_core::<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>(
            self,
            core,
            goal_response_callback,
            feedback_callback,
            result_callback,
            cancel_response_callback,
            context,
        )
    }

    #[inline]
    unsafe fn release_guard_condition(&mut self, handle: HandleId) -> bool {
        unsafe { Executor::release_guard_condition(self, handle) }
    }
}

// ── phase-515 R9 stage 2: the action cores and raw handles ─────────────

impl<
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
> sealed::Sealed for ActionServerCore<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>
{
}
impl<const GOAL_BUF: usize, const RESULT_BUF: usize, const FEEDBACK_BUF: usize> sealed::Sealed
    for ActionClientCore<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>
{
}
impl sealed::Sealed for ActionServerRawHandle {}
impl sealed::Sealed for ActionClientRawHandle {}

/// [`ActionServerCore`]'s methods, for `nros-c`, `nros-cpp` and the `nros` runtime. Sealed.
pub trait ActionServerCoreFfi: sealed::Sealed + Sized {
    /// The FFI entry point; see `ActionServerCore::from_channels` in `action_core.rs`.
    fn from_channels(
        send_goal_server: session::RmwServiceServer,
        cancel_goal_server: session::RmwServiceServer,
        get_result_server: session::RmwServiceServer,
        feedback_publisher: session::RmwPublisher,
        status_publisher: session::RmwPublisher,
    ) -> Self;

    /// The FFI entry point; see `ActionServerCore::try_recv_goal_request` in `action_core.rs`.
    fn try_recv_goal_request(&mut self) -> Result<Option<RawGoalRequest>, NodeError>;

    /// The FFI entry point; see `ActionServerCore::goal_buffer` in `action_core.rs`.
    fn goal_buffer(&self) -> &[u8];

    /// The FFI entry point; see `ActionServerCore::accept_goal` in `action_core.rs`.
    fn accept_goal(&mut self, goal_id: GoalId, seq: i64) -> Result<(), NodeError>;

    /// The FFI entry point; see `ActionServerCore::reject_goal` in `action_core.rs`.
    fn reject_goal(&mut self, seq: i64) -> Result<(), NodeError>;

    /// The FFI entry point; see `ActionServerCore::publish_feedback_raw` in `action_core.rs`.
    fn publish_feedback_raw(
        &mut self,
        goal_id: &GoalId,
        feedback_cdr: &[u8],
    ) -> Result<(), NodeError>;

    /// The FFI entry point; see `ActionServerCore::set_goal_status` in `action_core.rs`.
    fn set_goal_status(&mut self, goal_id: &GoalId, status: GoalStatus);

    /// The FFI entry point; see `ActionServerCore::expire_completed_results` in `action_core.rs`.
    fn expire_completed_results(&mut self) -> usize;

    /// The FFI entry point; see `ActionServerCore::completed_result_count` in `action_core.rs`.
    fn completed_result_count(&self) -> usize;

    /// The FFI entry point; see `ActionServerCore::result_slab_used` in `action_core.rs`.
    fn result_slab_used(&self) -> usize;

    /// The FFI entry point; see `ActionServerCore::goal_exists` in `action_core.rs`.
    fn goal_exists(&self, goal_id: &GoalId) -> bool;

    /// The FFI entry point; see `ActionServerCore::has_completed_result` in `action_core.rs`.
    fn has_completed_result(&self, goal_id: &GoalId) -> bool;

    /// The FFI entry point; see `ActionServerCore::complete_goal_raw` in `action_core.rs`.
    fn complete_goal_raw(
        &mut self,
        goal_id: &GoalId,
        status: GoalStatus,
        result_cdr: &[u8],
    ) -> Result<(), NodeError>;

    /// The FFI entry point; see `ActionServerCore::register_goal_waker` in `action_core.rs`.
    fn register_goal_waker(&self, waker: &core::task::Waker) -> bool;

    /// The FFI entry point; see `ActionServerCore::register_cancel_waker` in `action_core.rs`.
    fn register_cancel_waker(&self, waker: &core::task::Waker) -> bool;

    /// The FFI entry point; see `ActionServerCore::register_get_result_waker` in `action_core.rs`.
    fn register_get_result_waker(&self, waker: &core::task::Waker) -> bool;

    /// The FFI entry point; see `ActionServerCore::try_recv_cancel_request` in `action_core.rs`.
    fn try_recv_cancel_request(&mut self) -> Result<Option<PendingCancelRequest>, NodeError>;

    /// The FFI entry point; see `ActionServerCore::send_cancel_reply` in `action_core.rs`.
    fn send_cancel_reply(
        &mut self,
        sequence_number: i64,
        return_code: nros_core::CancelReturnCode,
        accepted: &[GoalId],
    ) -> Result<(), NodeError>;

    /// The FFI entry point; see `ActionServerCore::try_handle_cancel` in `action_core.rs`.
    fn try_handle_cancel(
        &mut self,
        cancel_handler: impl FnMut(&GoalId, GoalStatus) -> nros_core::CancelResponse,
    ) -> Result<Option<(GoalId, nros_core::CancelResponse)>, NodeError>;

    /// The FFI entry point; see `ActionServerCore::try_handle_get_result_raw` in `action_core.rs`.
    fn try_handle_get_result_raw(
        &mut self,
        default_result_cdr: &[u8],
    ) -> Result<Option<GoalId>, NodeError>;

    /// The FFI entry point; see `ActionServerCore::active_goal_count` in `action_core.rs`.
    fn active_goal_count(&self) -> usize;

    /// The FFI entry point; see `ActionServerCore::active_goals` in `action_core.rs`.
    fn active_goals(&self) -> &[RawActiveGoal];

    /// The FFI entry point; see `ActionServerCore::goal_stamp` in `action_core.rs`.
    fn goal_stamp(&self, goal_id: &GoalId) -> Time;

    /// The FFI entry point; see `ActionServerCore::find_goal_status` in `action_core.rs`.
    fn find_goal_status(&self, goal_id: &GoalId) -> GoalStatus;

    /// The FFI entry point; see `ActionServerCore::publish_status_array` in `action_core.rs`.
    fn publish_status_array(&self) -> Result<(), NodeError>;
}

impl<
    const GOAL_BUF: usize,
    const RESULT_BUF: usize,
    const FEEDBACK_BUF: usize,
    const MAX_GOALS: usize,
> ActionServerCoreFfi for ActionServerCore<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, MAX_GOALS>
{
    #[inline]
    fn from_channels(
        send_goal_server: session::RmwServiceServer,
        cancel_goal_server: session::RmwServiceServer,
        get_result_server: session::RmwServiceServer,
        feedback_publisher: session::RmwPublisher,
        status_publisher: session::RmwPublisher,
    ) -> Self {
        Self::from_channels(
            send_goal_server,
            cancel_goal_server,
            get_result_server,
            feedback_publisher,
            status_publisher,
        )
    }

    #[inline]
    fn try_recv_goal_request(&mut self) -> Result<Option<RawGoalRequest>, NodeError> {
        Self::try_recv_goal_request(self)
    }

    #[inline]
    fn goal_buffer(&self) -> &[u8] {
        Self::goal_buffer(self)
    }

    #[inline]
    fn accept_goal(&mut self, goal_id: GoalId, seq: i64) -> Result<(), NodeError> {
        Self::accept_goal(self, goal_id, seq)
    }

    #[inline]
    fn reject_goal(&mut self, seq: i64) -> Result<(), NodeError> {
        Self::reject_goal(self, seq)
    }

    #[inline]
    fn publish_feedback_raw(
        &mut self,
        goal_id: &GoalId,
        feedback_cdr: &[u8],
    ) -> Result<(), NodeError> {
        Self::publish_feedback_raw(self, goal_id, feedback_cdr)
    }

    #[inline]
    fn set_goal_status(&mut self, goal_id: &GoalId, status: GoalStatus) {
        Self::set_goal_status(self, goal_id, status)
    }

    #[inline]
    fn expire_completed_results(&mut self) -> usize {
        Self::expire_completed_results(self)
    }

    #[inline]
    fn completed_result_count(&self) -> usize {
        Self::completed_result_count(self)
    }

    #[inline]
    fn result_slab_used(&self) -> usize {
        Self::result_slab_used(self)
    }

    #[inline]
    fn goal_exists(&self, goal_id: &GoalId) -> bool {
        Self::goal_exists(self, goal_id)
    }

    #[inline]
    fn has_completed_result(&self, goal_id: &GoalId) -> bool {
        Self::has_completed_result(self, goal_id)
    }

    #[inline]
    fn complete_goal_raw(
        &mut self,
        goal_id: &GoalId,
        status: GoalStatus,
        result_cdr: &[u8],
    ) -> Result<(), NodeError> {
        Self::complete_goal_raw(self, goal_id, status, result_cdr)
    }

    #[inline]
    fn register_goal_waker(&self, waker: &core::task::Waker) -> bool {
        Self::register_goal_waker(self, waker)
    }

    #[inline]
    fn register_cancel_waker(&self, waker: &core::task::Waker) -> bool {
        Self::register_cancel_waker(self, waker)
    }

    #[inline]
    fn register_get_result_waker(&self, waker: &core::task::Waker) -> bool {
        Self::register_get_result_waker(self, waker)
    }

    #[inline]
    fn try_recv_cancel_request(&mut self) -> Result<Option<PendingCancelRequest>, NodeError> {
        Self::try_recv_cancel_request(self)
    }

    #[inline]
    fn send_cancel_reply(
        &mut self,
        sequence_number: i64,
        return_code: nros_core::CancelReturnCode,
        accepted: &[GoalId],
    ) -> Result<(), NodeError> {
        Self::send_cancel_reply(self, sequence_number, return_code, accepted)
    }

    #[inline]
    fn try_handle_cancel(
        &mut self,
        cancel_handler: impl FnMut(&GoalId, GoalStatus) -> nros_core::CancelResponse,
    ) -> Result<Option<(GoalId, nros_core::CancelResponse)>, NodeError> {
        Self::try_handle_cancel(self, cancel_handler)
    }

    #[inline]
    fn try_handle_get_result_raw(
        &mut self,
        default_result_cdr: &[u8],
    ) -> Result<Option<GoalId>, NodeError> {
        Self::try_handle_get_result_raw(self, default_result_cdr)
    }

    #[inline]
    fn active_goal_count(&self) -> usize {
        Self::active_goal_count(self)
    }

    #[inline]
    fn active_goals(&self) -> &[RawActiveGoal] {
        Self::active_goals(self)
    }

    #[inline]
    fn goal_stamp(&self, goal_id: &GoalId) -> Time {
        Self::goal_stamp(self, goal_id)
    }

    #[inline]
    fn find_goal_status(&self, goal_id: &GoalId) -> GoalStatus {
        Self::find_goal_status(self, goal_id)
    }

    #[inline]
    fn publish_status_array(&self) -> Result<(), NodeError> {
        Self::publish_status_array(self)
    }
}

/// [`ActionClientCore`]'s methods, for `nros-c`, `nros-cpp` and the `nros` runtime. Sealed.
pub trait ActionClientCoreFfi: sealed::Sealed + Sized {
    /// The FFI entry point; see `ActionClientCore::is_server_ready` in `action_core.rs`.
    fn is_server_ready(&self) -> bool;

    /// The FFI entry point; see `ActionClientCore::server_readiness` in `action_core.rs`.
    fn server_readiness(&self) -> Result<bool, nros_rmw::TransportError>;

    /// The FFI entry point; see `ActionClientCore::new` in `action_core.rs`.
    fn new(
        send_goal_client: session::RmwServiceClient,
        cancel_goal_client: session::RmwServiceClient,
        get_result_client: session::RmwServiceClient,
        feedback_subscriber: session::RmwSubscriber,
    ) -> Self;

    /// The FFI entry point; see `ActionClientCore::send_goal_raw` in `action_core.rs`.
    fn send_goal_raw(&mut self, goal_cdr: &[u8]) -> Result<GoalId, NodeError>;

    /// The FFI entry point; see `ActionClientCore::try_recv_feedback_raw` in `action_core.rs`.
    fn try_recv_feedback_raw(&mut self) -> Result<Option<(GoalId, usize)>, NodeError>;

    /// The FFI entry point; see `ActionClientCore::send_cancel_request` in `action_core.rs`.
    fn send_cancel_request(&mut self, goal_id: &GoalId) -> Result<(), NodeError>;

    /// The FFI entry point; see `ActionClientCore::send_cancel_request_stamped` in `action_core.rs`.
    fn send_cancel_request_stamped(
        &mut self,
        goal_id: &GoalId,
        stamp_sec: i32,
        stamp_nanosec: u32,
    ) -> Result<i64, NodeError>;

    /// The FFI entry point; see `ActionClientCore::send_get_result_request` in `action_core.rs`.
    fn send_get_result_request(&mut self, goal_id: &GoalId) -> Result<(), NodeError>;

    /// The FFI entry point; see `ActionClientCore::register_goal_response_waker` in `action_core.rs`.
    fn register_goal_response_waker(&self, waker: &core::task::Waker) -> bool;

    /// The FFI entry point; see `ActionClientCore::register_cancel_response_waker` in `action_core.rs`.
    fn register_cancel_response_waker(&self, waker: &core::task::Waker) -> bool;

    /// The FFI entry point; see `ActionClientCore::register_result_waker` in `action_core.rs`.
    fn register_result_waker(&self, waker: &core::task::Waker) -> bool;

    /// The FFI entry point; see `ActionClientCore::register_feedback_waker` in `action_core.rs`.
    fn register_feedback_waker(&self, waker: &core::task::Waker) -> bool;

    /// The FFI entry point; see `ActionClientCore::try_recv_cancel_reply` in `action_core.rs`.
    fn try_recv_cancel_reply(&mut self) -> Result<Option<usize>, NodeError>;

    /// The FFI entry point; see `ActionClientCore::try_recv_cancel_reply_for` in `action_core.rs`.
    fn try_recv_cancel_reply_for(&mut self) -> Result<Option<(usize, GoalId)>, NodeError>;

    /// The FFI entry point; see `ActionClientCore::try_recv_cancel_reply_seq` in `action_core.rs`.
    fn try_recv_cancel_reply_seq(&mut self) -> Result<Option<(usize, i64, GoalId)>, NodeError>;

    /// The FFI entry point; see `ActionClientCore::try_recv_get_result_reply` in `action_core.rs`.
    fn try_recv_get_result_reply(&mut self) -> Result<Option<usize>, NodeError>;

    /// The FFI entry point; see `ActionClientCore::try_recv_get_result_reply_for` in `action_core.rs`.
    fn try_recv_get_result_reply_for(&mut self) -> Result<Option<(usize, GoalId)>, NodeError>;

    /// The FFI entry point; see `ActionClientCore::try_recv_send_goal_reply` in `action_core.rs`.
    fn try_recv_send_goal_reply(&mut self) -> Result<Option<usize>, NodeError>;

    /// The FFI entry point; see `ActionClientCore::try_recv_send_goal_reply_for` in `action_core.rs`.
    fn try_recv_send_goal_reply_for(&mut self) -> Result<Option<(usize, GoalId)>, NodeError>;

    /// The FFI entry point; see `ActionClientCore::forget_send_goal` in `action_core.rs`.
    fn forget_send_goal(&mut self, goal_id: &GoalId) -> bool;

    /// The FFI entry point; see `ActionClientCore::forget_get_result` in `action_core.rs`.
    fn forget_get_result(&mut self, goal_id: &GoalId) -> bool;

    /// The FFI entry point; see `ActionClientCore::forget_cancel` in `action_core.rs`.
    fn forget_cancel(&mut self, goal_id: &GoalId) -> bool;

    /// The FFI entry point; see `ActionClientCore::result_buffer_ref` in `action_core.rs`.
    fn result_buffer_ref(&self) -> &[u8];

    /// The FFI entry point; see `ActionClientCore::feedback_buffer_ref` in `action_core.rs`.
    fn feedback_buffer_ref(&self) -> &[u8];

    /// The FFI entry point; see `ActionClientCore::goal_counter` in `action_core.rs`.
    fn goal_counter(&self) -> u64;
}

impl<const GOAL_BUF: usize, const RESULT_BUF: usize, const FEEDBACK_BUF: usize> ActionClientCoreFfi
    for ActionClientCore<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF>
{
    #[inline]
    fn is_server_ready(&self) -> bool {
        Self::is_server_ready(self)
    }

    #[inline]
    fn server_readiness(&self) -> Result<bool, nros_rmw::TransportError> {
        Self::server_readiness(self)
    }

    #[inline]
    fn new(
        send_goal_client: session::RmwServiceClient,
        cancel_goal_client: session::RmwServiceClient,
        get_result_client: session::RmwServiceClient,
        feedback_subscriber: session::RmwSubscriber,
    ) -> Self {
        Self::new(
            send_goal_client,
            cancel_goal_client,
            get_result_client,
            feedback_subscriber,
        )
    }

    #[inline]
    fn send_goal_raw(&mut self, goal_cdr: &[u8]) -> Result<GoalId, NodeError> {
        Self::send_goal_raw(self, goal_cdr)
    }

    #[inline]
    fn try_recv_feedback_raw(&mut self) -> Result<Option<(GoalId, usize)>, NodeError> {
        Self::try_recv_feedback_raw(self)
    }

    #[inline]
    fn send_cancel_request(&mut self, goal_id: &GoalId) -> Result<(), NodeError> {
        Self::send_cancel_request(self, goal_id)
    }

    #[inline]
    fn send_cancel_request_stamped(
        &mut self,
        goal_id: &GoalId,
        stamp_sec: i32,
        stamp_nanosec: u32,
    ) -> Result<i64, NodeError> {
        Self::send_cancel_request_stamped(self, goal_id, stamp_sec, stamp_nanosec)
    }

    #[inline]
    fn send_get_result_request(&mut self, goal_id: &GoalId) -> Result<(), NodeError> {
        Self::send_get_result_request(self, goal_id)
    }

    #[inline]
    fn register_goal_response_waker(&self, waker: &core::task::Waker) -> bool {
        Self::register_goal_response_waker(self, waker)
    }

    #[inline]
    fn register_cancel_response_waker(&self, waker: &core::task::Waker) -> bool {
        Self::register_cancel_response_waker(self, waker)
    }

    #[inline]
    fn register_result_waker(&self, waker: &core::task::Waker) -> bool {
        Self::register_result_waker(self, waker)
    }

    #[inline]
    fn register_feedback_waker(&self, waker: &core::task::Waker) -> bool {
        Self::register_feedback_waker(self, waker)
    }

    #[inline]
    fn try_recv_cancel_reply(&mut self) -> Result<Option<usize>, NodeError> {
        Self::try_recv_cancel_reply(self)
    }

    #[inline]
    fn try_recv_cancel_reply_for(&mut self) -> Result<Option<(usize, GoalId)>, NodeError> {
        Self::try_recv_cancel_reply_for(self)
    }

    #[inline]
    fn try_recv_cancel_reply_seq(&mut self) -> Result<Option<(usize, i64, GoalId)>, NodeError> {
        Self::try_recv_cancel_reply_seq(self)
    }

    #[inline]
    fn try_recv_get_result_reply(&mut self) -> Result<Option<usize>, NodeError> {
        Self::try_recv_get_result_reply(self)
    }

    #[inline]
    fn try_recv_get_result_reply_for(&mut self) -> Result<Option<(usize, GoalId)>, NodeError> {
        Self::try_recv_get_result_reply_for(self)
    }

    #[inline]
    fn try_recv_send_goal_reply(&mut self) -> Result<Option<usize>, NodeError> {
        Self::try_recv_send_goal_reply(self)
    }

    #[inline]
    fn try_recv_send_goal_reply_for(&mut self) -> Result<Option<(usize, GoalId)>, NodeError> {
        Self::try_recv_send_goal_reply_for(self)
    }

    #[inline]
    fn forget_send_goal(&mut self, goal_id: &GoalId) -> bool {
        Self::forget_send_goal(self, goal_id)
    }

    #[inline]
    fn forget_get_result(&mut self, goal_id: &GoalId) -> bool {
        Self::forget_get_result(self, goal_id)
    }

    #[inline]
    fn forget_cancel(&mut self, goal_id: &GoalId) -> bool {
        Self::forget_cancel(self, goal_id)
    }

    #[inline]
    fn result_buffer_ref(&self) -> &[u8] {
        Self::result_buffer_ref(self)
    }

    #[inline]
    fn feedback_buffer_ref(&self) -> &[u8] {
        Self::feedback_buffer_ref(self)
    }

    #[inline]
    fn goal_counter(&self) -> u64 {
        Self::goal_counter(self)
    }
}

/// [`ActionServerRawHandle`]'s methods, for `nros-c`, `nros-cpp` and the `nros` runtime. Sealed.
pub trait ActionServerRawHandleFfi: sealed::Sealed + Sized {
    /// The FFI entry point; see `ActionServerRawHandle::invalid` in `action.rs`
    /// (a `const fn` there; a trait method cannot be one).
    fn invalid() -> Self;

    /// The FFI entry point; see `ActionServerRawHandle::is_invalid` in `action.rs`.
    fn is_invalid(&self) -> bool;

    /// The FFI entry point; see `ActionServerRawHandle::handle_id` in `action.rs`.
    fn handle_id(&self) -> HandleId;

    /// The FFI entry point; see `ActionServerRawHandle::publish_feedback_raw` in `action.rs`.
    fn publish_feedback_raw(
        &self,
        executor: &mut Executor,
        goal_id: &nros_core::GoalId,
        feedback_data: &[u8],
    ) -> Result<(), NodeError>;

    /// The FFI entry point; see `ActionServerRawHandle::complete_goal_raw` in `action.rs`.
    fn complete_goal_raw(
        &self,
        executor: &mut Executor,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
        result_data: &[u8],
    ) -> Result<(), NodeError>;

    /// The FFI entry point; see `ActionServerRawHandle::set_goal_status` in `action.rs`.
    fn set_goal_status(
        &self,
        executor: &mut Executor,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
    );

    /// The FFI entry point; see `ActionServerRawHandle::active_goal_count` in `action.rs`.
    fn active_goal_count(&self, executor: &Executor) -> usize;

    /// The FFI entry point; see `ActionServerRawHandle::goal_exists` in `action.rs`.
    fn goal_exists(&self, executor: &Executor, goal_id: &nros_core::GoalId) -> bool;

    /// The FFI entry point; see `ActionServerRawHandle::expire_goals` in `action.rs`.
    fn expire_goals(&self, executor: &mut Executor) -> usize;

    /// The FFI entry point; see `ActionServerRawHandle::for_each_active_goal` in `action.rs`.
    fn for_each_active_goal(&self, executor: &Executor, f: impl FnMut(&RawActiveGoal));

    /// The FFI entry point; see `ActionServerRawHandle::goal_status` in `action.rs`.
    fn goal_status(
        &self,
        executor: &Executor,
        goal_id: &nros_core::GoalId,
    ) -> Option<nros_core::GoalStatus>;
}

impl ActionServerRawHandleFfi for ActionServerRawHandle {
    #[inline]
    fn invalid() -> Self {
        Self::invalid()
    }

    #[inline]
    fn is_invalid(&self) -> bool {
        Self::is_invalid(self)
    }

    #[inline]
    fn handle_id(&self) -> HandleId {
        Self::handle_id(self)
    }

    #[inline]
    fn publish_feedback_raw(
        &self,
        executor: &mut Executor,
        goal_id: &nros_core::GoalId,
        feedback_data: &[u8],
    ) -> Result<(), NodeError> {
        Self::publish_feedback_raw(self, executor, goal_id, feedback_data)
    }

    #[inline]
    fn complete_goal_raw(
        &self,
        executor: &mut Executor,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
        result_data: &[u8],
    ) -> Result<(), NodeError> {
        Self::complete_goal_raw(self, executor, goal_id, status, result_data)
    }

    #[inline]
    fn set_goal_status(
        &self,
        executor: &mut Executor,
        goal_id: &nros_core::GoalId,
        status: nros_core::GoalStatus,
    ) {
        Self::set_goal_status(self, executor, goal_id, status)
    }

    #[inline]
    fn active_goal_count(&self, executor: &Executor) -> usize {
        Self::active_goal_count(self, executor)
    }

    #[inline]
    fn goal_exists(&self, executor: &Executor, goal_id: &nros_core::GoalId) -> bool {
        Self::goal_exists(self, executor, goal_id)
    }

    #[inline]
    fn expire_goals(&self, executor: &mut Executor) -> usize {
        Self::expire_goals(self, executor)
    }

    #[inline]
    fn for_each_active_goal(&self, executor: &Executor, f: impl FnMut(&RawActiveGoal)) {
        Self::for_each_active_goal(self, executor, f)
    }

    #[inline]
    fn goal_status(
        &self,
        executor: &Executor,
        goal_id: &nros_core::GoalId,
    ) -> Option<nros_core::GoalStatus> {
        Self::goal_status(self, executor, goal_id)
    }
}

/// [`ActionClientRawHandle`]'s methods, for `nros-c`, `nros-cpp` and the `nros` runtime. Sealed.
pub trait ActionClientRawHandleFfi: sealed::Sealed + Sized {
    /// The FFI entry point; see `ActionClientRawHandle::entry_index` in `action.rs`.
    fn entry_index(&self) -> usize;
}

impl ActionClientRawHandleFfi for ActionClientRawHandle {
    #[inline]
    fn entry_index(&self) -> usize {
        Self::entry_index(self)
    }
}
