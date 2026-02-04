//! nano-ros Zephyr Listener Example (Rust)
//!
//! This example demonstrates a ROS 2 compatible subscriber running on
//! Zephyr RTOS using the nano-ros BSP.
//!
//! Architecture:
//! ```text
//! Rust Application (this file)
//!     └── nano-ros-bsp-zephyr (C BSP)
//!         └── zenoh_shim.c (C shim)
//!             └── zenoh-pico (C library)
//!                 └── Zephyr network stack
//! ```

#![no_std]

use core::ffi::{c_char, c_void};
use core::marker::PhantomData;
use core::sync::atomic::{AtomicU32, Ordering};

use log::{error, info};

// nano-ros CDR serialization
use nano_ros_core::{Deserialize, RosMessage};
use nano_ros_serdes::CdrReader;

// Generated message types
use std_msgs::msg::Int32;

// =============================================================================
// FFI bindings to nano-ros-bsp-zephyr
// =============================================================================

/// BSP error codes
pub const NANO_ROS_BSP_OK: i32 = 0;

/// Zephyr timeout type (opaque, we just pass k_timeout_t values)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct KTimeout {
    ticks: i64,
}

impl KTimeout {
    /// Create a timeout in seconds
    pub const fn secs(s: i64) -> Self {
        // k_ms_to_ticks_ceil64 approximation: assume 1000 ticks/sec for native_sim
        Self { ticks: s * 1000 }
    }
}

/// Subscriber callback type
pub type NanoRosSubscriberCallback = extern "C" fn(*const u8, usize, *mut c_void);

#[repr(C)]
pub struct NanoRosBspContext {
    initialized: bool,
    session_open: bool,
}

#[repr(C)]
pub struct NanoRosNode {
    ctx: *mut NanoRosBspContext,
    name: *const c_char,
    domain_id: i32,
}

#[repr(C)]
pub struct NanoRosSubscriber {
    node: *mut NanoRosNode,
    handle: i32,
    keyexpr: [u8; 256],
    callback: Option<NanoRosSubscriberCallback>,
    user_data: *mut c_void,
}

unsafe extern "C" {
    fn nano_ros_bsp_init(ctx: *mut NanoRosBspContext) -> i32;
    fn nano_ros_bsp_init_with_locator(ctx: *mut NanoRosBspContext, locator: *const c_char) -> i32;
    fn nano_ros_bsp_shutdown(ctx: *mut NanoRosBspContext);
    fn nano_ros_bsp_is_ready(ctx: *const NanoRosBspContext) -> bool;

    fn nano_ros_bsp_create_node(
        ctx: *mut NanoRosBspContext,
        node: *mut NanoRosNode,
        name: *const c_char,
    ) -> i32;

    fn nano_ros_bsp_create_subscriber(
        node: *mut NanoRosNode,
        sub: *mut NanoRosSubscriber,
        topic: *const c_char,
        type_name: *const c_char,
        callback: NanoRosSubscriberCallback,
        user_data: *mut c_void,
    ) -> i32;

    fn nano_ros_bsp_destroy_subscriber(sub: *mut NanoRosSubscriber);

    fn nano_ros_bsp_spin_once(ctx: *mut NanoRosBspContext, timeout: KTimeout) -> i32;
    fn nano_ros_bsp_spin(ctx: *mut NanoRosBspContext) -> i32;
}

// =============================================================================
// High-level Rust API wrapping BSP
// =============================================================================

/// Error type for Zephyr BSP operations
#[derive(Debug, Clone, Copy)]
pub enum BspError {
    /// BSP initialization failed
    InitFailed(i32),
    /// Node creation failed
    NodeFailed(i32),
    /// Subscriber creation failed
    SubscriberFailed(i32),
    /// Topic name too long
    TopicTooLong,
}

/// BSP context wrapper
pub struct BspContext {
    ctx: NanoRosBspContext,
}

impl BspContext {
    /// Initialize BSP with custom locator
    pub fn new(locator: &[u8]) -> Result<Self, BspError> {
        let mut ctx = NanoRosBspContext {
            initialized: false,
            session_open: false,
        };

        let ret = unsafe { nano_ros_bsp_init_with_locator(&mut ctx, locator.as_ptr() as *const c_char) };
        if ret != NANO_ROS_BSP_OK {
            return Err(BspError::InitFailed(ret));
        }

        Ok(Self { ctx })
    }

    /// Check if BSP is ready
    pub fn is_ready(&self) -> bool {
        unsafe { nano_ros_bsp_is_ready(&self.ctx) }
    }

    /// Spin once with timeout
    pub fn spin_once(&mut self, timeout: KTimeout) {
        unsafe {
            nano_ros_bsp_spin_once(&mut self.ctx, timeout);
        }
    }

    /// Spin forever (blocking)
    pub fn spin(&mut self) {
        unsafe {
            nano_ros_bsp_spin(&mut self.ctx);
        }
    }
}

impl Drop for BspContext {
    fn drop(&mut self) {
        unsafe {
            nano_ros_bsp_shutdown(&mut self.ctx);
        }
    }
}

/// BSP Node wrapper
pub struct BspNode<'a> {
    node: NanoRosNode,
    _ctx: PhantomData<&'a mut BspContext>,
}

impl<'a> BspNode<'a> {
    /// Create a new node
    pub fn new(ctx: &'a mut BspContext, name: &[u8]) -> Result<Self, BspError> {
        let mut node = NanoRosNode {
            ctx: &mut ctx.ctx,
            name: core::ptr::null(),
            domain_id: 0,
        };

        let ret = unsafe {
            nano_ros_bsp_create_node(&mut ctx.ctx, &mut node, name.as_ptr() as *const c_char)
        };
        if ret != NANO_ROS_BSP_OK {
            return Err(BspError::NodeFailed(ret));
        }

        Ok(Self {
            node,
            _ctx: PhantomData,
        })
    }

    /// Spin once with timeout
    ///
    /// This processes network events and callbacks.
    pub fn spin_once(&mut self, timeout: KTimeout) {
        unsafe {
            nano_ros_bsp_spin_once(self.node.ctx, timeout);
        }
    }

    /// Create a subscriber with raw callback
    ///
    /// # Safety
    ///
    /// The callback and user_data must remain valid for the subscriber's lifetime.
    pub unsafe fn create_subscriber<M: RosMessage>(
        &mut self,
        topic: &[u8],
        callback: NanoRosSubscriberCallback,
        user_data: *mut c_void,
    ) -> Result<BspSubscriber<'a, M>, BspError> {
        // Create the BspSubscriber first with uninitialized sub field.
        // This ensures the sub struct has a stable address that won't change
        // when we return it. The C code stores this address as ctx for callbacks.
        let mut subscriber = BspSubscriber {
            sub: NanoRosSubscriber {
                node: &mut self.node,
                handle: -1,
                keyexpr: [0; 256],
                callback: Some(callback),
                user_data,
            },
            _phantom: PhantomData,
        };

        let type_name = M::TYPE_NAME.as_bytes();
        let mut type_name_buf = [0u8; 128];
        if type_name.len() >= type_name_buf.len() {
            return Err(BspError::TopicTooLong);
        }
        type_name_buf[..type_name.len()].copy_from_slice(type_name);

        let ret = unsafe {
            nano_ros_bsp_create_subscriber(
                &mut self.node,
                &mut subscriber.sub,  // Now uses the address inside BspSubscriber
                topic.as_ptr() as *const c_char,
                type_name_buf.as_ptr() as *const c_char,
                callback,
                user_data,
            )
        };
        if ret != NANO_ROS_BSP_OK {
            return Err(BspError::SubscriberFailed(ret));
        }

        Ok(subscriber)
    }
}

/// BSP Subscriber wrapper
pub struct BspSubscriber<'a, M> {
    sub: NanoRosSubscriber,
    _phantom: PhantomData<&'a M>,
}

impl<M> Drop for BspSubscriber<'_, M> {
    fn drop(&mut self) {
        unsafe {
            nano_ros_bsp_destroy_subscriber(&mut self.sub);
        }
    }
}

// =============================================================================
// Message callback and handling
// =============================================================================

/// Counter for received messages
static MSG_COUNT: AtomicU32 = AtomicU32::new(0);

/// Callback invoked when a message is received
///
/// Note: This must be `extern "C"` to match the C function pointer ABI.
extern "C" fn on_int32_message(data: *const u8, len: usize, _ctx: *mut c_void) {
    let count = MSG_COUNT.fetch_add(1, Ordering::Relaxed);

    // Safety: data is valid for len bytes, provided by C BSP
    let payload = unsafe { core::slice::from_raw_parts(data, len) };

    // Deserialize the Int32 message
    let mut reader = CdrReader::new(payload);
    match Int32::deserialize(&mut reader) {
        Ok(msg) => {
            info!("[{}] Received: data={} ({} bytes)", count, msg.data, len);
        }
        Err(_) => {
            info!(
                "[{}] Received {} bytes (deserialization failed)",
                count, len
            );
        }
    }
}

// =============================================================================
// Static storage for subscriber
// =============================================================================

use core::mem::MaybeUninit;
use core::ptr::addr_of_mut;

/// A wrapper to make MaybeUninit safe to use in a static.
/// Safety: We ensure single-threaded access via SUBSCRIBER_INITIALIZED flag.
struct StaticSubscriber(MaybeUninit<NanoRosSubscriber>);

// Safety: Zephyr runs on single core and we use proper synchronization
unsafe impl Sync for StaticSubscriber {}

/// Static storage for the subscriber struct.
///
/// The C BSP stores a pointer to this struct in its internal callback table.
/// We must use static storage to ensure the address remains stable throughout
/// the program's lifetime. Stack-allocated structs would be invalidated when
/// the function returns or when Rust moves values.
static mut SUBSCRIBER_STORAGE: StaticSubscriber =
    StaticSubscriber(MaybeUninit::uninit());

/// Flag to track if subscriber storage is initialized
static SUBSCRIBER_INITIALIZED: AtomicU32 = AtomicU32::new(0);

// =============================================================================
// Main entry point
// =============================================================================

/// Entry point for Zephyr (called by zephyr-lang-rust)
#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    // Initialize logging
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nano-ros Zephyr Listener (BSP)");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);

    // Connection parameters (for QEMU/native_sim, connect to host at 192.0.2.2)
    let locator = b"tcp/192.0.2.2:7447\0";

    info!("Connecting to zenoh router...");

    // Initialize BSP
    let mut ctx = match BspContext::new(locator) {
        Ok(ctx) => ctx,
        Err(e) => {
            error!("BSP init failed: {:?}", e);
            return;
        }
    };
    info!("Session opened");

    // Create node
    let mut node = match BspNode::new(&mut ctx, b"listener\0") {
        Ok(n) => n,
        Err(e) => {
            error!("Node creation failed: {:?}", e);
            return;
        }
    };

    // Initialize subscriber in static storage to ensure stable address.
    // The C code stores a pointer to this struct for callbacks.
    let sub_ptr: *mut NanoRosSubscriber = unsafe {
        // Use addr_of_mut! to avoid creating a mutable reference to a static mut
        let storage_ptr = addr_of_mut!(SUBSCRIBER_STORAGE);
        let sub = (*storage_ptr).0.as_mut_ptr();

        // Initialize the struct fields
        (*sub).node = &mut node.node;
        (*sub).handle = -1;
        (*sub).keyexpr = [0; 256];
        (*sub).callback = Some(on_int32_message);
        (*sub).user_data = core::ptr::null_mut();

        sub
    };

    // Get type name for Int32
    let type_name = Int32::TYPE_NAME.as_bytes();
    let mut type_name_buf = [0u8; 128];
    type_name_buf[..type_name.len()].copy_from_slice(type_name);

    // Create subscriber using C BSP
    let ret = unsafe {
        nano_ros_bsp_create_subscriber(
            &mut node.node,
            sub_ptr,
            b"/chatter\0".as_ptr() as *const c_char,
            type_name_buf.as_ptr() as *const c_char,
            on_int32_message,
            core::ptr::null_mut(),
        )
    };

    if ret != NANO_ROS_BSP_OK {
        error!("Subscriber creation failed: {}", ret);
        return;
    }

    SUBSCRIBER_INITIALIZED.store(1, Ordering::Release);
    info!("Waiting for messages on /chatter...");

    // Main loop - use spin_once to process events
    loop {
        node.spin_once(KTimeout::secs(10));
        let count = MSG_COUNT.load(Ordering::Relaxed);
        info!("Total messages received: {}", count);
    }
}
