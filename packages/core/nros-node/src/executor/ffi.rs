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
//! Not `#[doc(hidden)]`: that leaves a method callable and semver-visible.
#![allow(clippy::too_many_arguments, clippy::missing_safety_doc)]

use core::mem::MaybeUninit;

use nros_rmw::QoSProfile;

use super::*;
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
