//! High-level Node API for STM32F4
//!
//! This module provides a simplified interface for ROS-style pub/sub
//! communication without exposing smoltcp or zenoh-pico details.

use core::ffi::{c_char, c_void};
use core::fmt::Write as _;

use heapless::String;
use nano_ros_core::RosMessage;

use crate::config::Config;
use crate::platform::Platform;
use crate::subscription_trampoline;
use crate::{Error, Publisher, Result, Subscription};

use zenoh_pico_shim_sys::{
    zenoh_shim_close, zenoh_shim_declare_publisher, zenoh_shim_declare_subscriber, zenoh_shim_init,
    zenoh_shim_open, zenoh_shim_spin_once,
};

/// High-level node handle for pub/sub operations
pub struct Node {
    platform: Platform,
    _initialized: bool,
    domain_id: u32,
}

impl Node {
    /// Create a typed publisher for a ROS 2 topic
    ///
    /// Constructs the ROS 2 keyexpr from topic name and `M::TYPE_NAME`:
    /// `<domain_id>/<topic>/<type_name>/TypeHashNotSupported`
    ///
    /// # Example
    ///
    /// ```ignore
    /// let pub_ = node.create_publisher::<Int32>("/chatter")?;
    /// pub_.publish(&Int32 { data: 42 })?;
    /// ```
    pub fn create_publisher<M: RosMessage>(&mut self, topic: &str) -> Result<Publisher<M>> {
        let mut key = format_ros2_keyexpr(self.domain_id, topic, M::TYPE_NAME);
        key.push('\0').map_err(|_| Error::TopicTooLong)?;
        let handle = unsafe { zenoh_shim_declare_publisher(key.as_bytes().as_ptr() as *const c_char) };
        if handle < 0 {
            defmt::error!("Failed to create publisher: {}", handle);
            return Err(Error::Publisher);
        }
        defmt::info!("Publisher created (handle={})", handle);
        Ok(Publisher {
            handle,
            _marker: core::marker::PhantomData,
        })
    }

    /// Create a typed subscription for a ROS 2 topic
    ///
    /// Messages are deserialized from CDR and delivered to the callback.
    ///
    /// # Limitations
    ///
    /// The callback is a function pointer (`fn(&M)`), not a closure.
    /// Use `static` variables for external state — the standard bare-metal pattern.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn on_message(msg: &Int32) {
    ///     // handle message
    /// }
    /// let _sub = node.create_subscription::<Int32>("/chatter", on_message)?;
    /// ```
    pub fn create_subscription<M: RosMessage>(
        &mut self,
        topic: &str,
        callback: fn(&M),
    ) -> Result<Subscription<M>> {
        let mut key = format_ros2_keyexpr_wildcard(self.domain_id, topic, M::TYPE_NAME);
        key.push('\0').map_err(|_| Error::TopicTooLong)?;
        let ctx = callback as *mut c_void;
        let handle = unsafe {
            zenoh_shim_declare_subscriber(
                key.as_bytes().as_ptr() as *const c_char,
                subscription_trampoline::<M>,
                ctx,
            )
        };
        if handle < 0 {
            defmt::error!("Failed to create subscriber: {}", handle);
            return Err(Error::Subscriber);
        }
        defmt::info!("Subscriber created (handle={})", handle);
        Ok(Subscription {
            handle,
            _marker: core::marker::PhantomData,
        })
    }

    /// Process network events and callbacks
    ///
    /// This must be called periodically to:
    /// - Handle network traffic
    /// - Dispatch subscriber callbacks
    /// - Process zenoh protocol messages
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Maximum time to spend processing (0 = non-blocking)
    pub fn spin_once(&mut self, timeout_ms: u32) {
        // Poll network interface and bridge to zenoh-pico
        self.platform.poll();

        // Process zenoh events
        unsafe {
            zenoh_shim_spin_once(timeout_ms);
        }
    }

    /// Get current uptime in milliseconds
    pub fn now_ms(&self) -> u64 {
        self.platform.now_ms()
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        defmt::info!("Shutting down node...");
        unsafe {
            zenoh_shim_close();
        }
    }
}

/// Entry point for BSP-based applications
///
/// Initializes all hardware and zenoh infrastructure, then calls the
/// user-provided closure with a ready-to-use `Node`.
///
/// # Example
///
/// ```no_run
/// use nano_ros_bsp_stm32f4::prelude::*;
///
/// // Define a message type
/// struct Int32 { data: i32 }
/// // ... impl Serialize, Deserialize, RosMessage ...
///
/// #[entry]
/// fn main() -> ! {
///     run_node(Config::nucleo_f429zi(), |node| {
///         let pub_ = node.create_publisher::<Int32>("/chatter")?;
///         pub_.publish(&Int32 { data: 42 })?;
///         Ok(())
///     })
/// }
/// ```
///
/// # Panics
///
/// Panics if hardware initialization fails.
pub fn run_node<F>(config: Config, f: F) -> !
where
    F: FnOnce(&mut Node) -> Result<()>,
{
    defmt::info!("nano-ros STM32F4 BSP starting...");
    defmt::info!(
        "  IP: {}.{}.{}.{}",
        config.ip[0],
        config.ip[1],
        config.ip[2],
        config.ip[3]
    );

    // Initialize platform (clocks, Ethernet, smoltcp, zenoh-pico platform)
    let platform = match unsafe { crate::platform::init(&config) } {
        Ok(p) => p,
        Err(e) => {
            defmt::error!("Platform init failed: {:?}", e);
            loop {
                cortex_m::asm::wfi();
            }
        }
    };

    // Initialize zenoh session
    defmt::info!("Connecting to zenoh router...");
    let ret = unsafe { zenoh_shim_init(config.zenoh_locator.as_ptr() as *const c_char) };
    if ret < 0 {
        defmt::error!("zenoh_shim_init failed: {}", ret);
        loop {
            cortex_m::asm::wfi();
        }
    }

    let ret = unsafe { zenoh_shim_open() };
    if ret < 0 {
        defmt::error!("zenoh_shim_open failed: {}", ret);
        loop {
            cortex_m::asm::wfi();
        }
    }
    defmt::info!("Zenoh session opened");

    // Create node
    let mut node = Node {
        platform,
        _initialized: true,
        domain_id: config.domain_id,
    };

    // Run user code
    match f(&mut node) {
        Ok(()) => {
            defmt::info!("Application completed successfully");
        }
        Err(e) => {
            defmt::error!("Application error: {:?}", e);
        }
    }

    // Node will be dropped here, closing zenoh session

    defmt::info!("Entering idle loop");
    loop {
        cortex_m::asm::wfi();
    }
}

/// Format a ROS 2 data keyexpr: `<domain_id>/<topic>/<type_name>/TypeHashNotSupported`
fn format_ros2_keyexpr(domain_id: u32, topic: &str, type_name: &str) -> String<256> {
    let mut key = String::<256>::new();
    let topic_stripped = topic.trim_matches('/');
    let _ = write!(
        key,
        "{}/{}/{}/TypeHashNotSupported",
        domain_id, topic_stripped, type_name
    );
    key
}

/// Format a ROS 2 subscriber keyexpr with wildcard: `<domain_id>/<topic>/<type_name>/*`
fn format_ros2_keyexpr_wildcard(domain_id: u32, topic: &str, type_name: &str) -> String<256> {
    let mut key = String::<256>::new();
    let topic_stripped = topic.trim_matches('/');
    let _ = write!(key, "{}/{}/{}/*", domain_id, topic_stripped, type_name);
    key
}
