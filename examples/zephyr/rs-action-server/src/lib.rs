//! nano-ros Zephyr Action Server Example (Rust)
//!
//! This example demonstrates a ROS 2 compatible action server running on
//! Zephyr RTOS using the zenoh-pico-shim crate.
//!
//! The server implements the Fibonacci action - computing Fibonacci sequences
//! with progress feedback.
//!
//! Architecture:
//! ```text
//! Rust Application (this file)
//!     └── zenoh-pico-shim (Rust wrapper)
//!         └── zenoh_shim.c (C shim, compiled by Zephyr)
//!             └── zenoh-pico (C library)
//!                 └── Zephyr network stack
//! ```
//!
//! Action channels (5 total):
//! - send_goal (queryable) - Accept new goal requests
//! - cancel_goal (queryable) - Handle cancellation requests
//! - get_result (queryable) - Return completed results
//! - feedback (publisher) - Send progress updates
//! - status (publisher) - Send status updates

#![no_std]

use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use log::{error, info};

// nano-ros CDR serialization
use nano_ros_core::{Deserialize, Serialize};
use nano_ros_serdes::{CdrReader, CdrWriter};

// zenoh-pico shim crate
use zenoh_pico_shim::{ShimContext, ShimError, ShimPublisher};

// Generated action types
use example_interfaces::action::{FibonacciFeedback, FibonacciGoal, FibonacciResult};
// Uuid type not currently used - goal_id handled as raw bytes
// use unique_identifier_msgs::msg::Uuid;

// =============================================================================
// Constants
// =============================================================================

/// Goal status values (matches action_msgs/GoalStatus)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalStatus {
    Unknown = 0,
    Accepted = 1,
    Executing = 2,
    Canceling = 3,
    Succeeded = 4,
    Canceled = 5,
    Aborted = 6,
}

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Clone, Copy)]
pub enum ZephyrError {
    Shim(ShimError),
    SerializationError,
    KeyExprTooLong,
}

impl From<ShimError> for ZephyrError {
    fn from(err: ShimError) -> Self {
        ZephyrError::Shim(err)
    }
}

// =============================================================================
// Goal Management
// =============================================================================

/// Maximum number of concurrent goals
const MAX_GOALS: usize = 4;

/// Active goal state
#[derive(Clone)]
struct ActiveGoal {
    /// Goal ID (UUID)
    goal_id: [u8; 16],
    /// Goal data
    order: i32,
    /// Current status
    status: GoalStatus,
    /// Completed result (if any)
    result: Option<FibonacciResult>,
    /// Is this slot in use?
    active: bool,
}

impl Default for ActiveGoal {
    fn default() -> Self {
        Self {
            goal_id: [0u8; 16],
            order: 0,
            status: GoalStatus::Unknown,
            result: None,
            active: false,
        }
    }
}

/// Global goal storage
static mut ACTIVE_GOALS: [ActiveGoal; MAX_GOALS] = [
    ActiveGoal {
        goal_id: [0u8; 16],
        order: 0,
        status: GoalStatus::Unknown,
        result: None,
        active: false,
    },
    ActiveGoal {
        goal_id: [0u8; 16],
        order: 0,
        status: GoalStatus::Unknown,
        result: None,
        active: false,
    },
    ActiveGoal {
        goal_id: [0u8; 16],
        order: 0,
        status: GoalStatus::Unknown,
        result: None,
        active: false,
    },
    ActiveGoal {
        goal_id: [0u8; 16],
        order: 0,
        status: GoalStatus::Unknown,
        result: None,
        active: false,
    },
];

/// Tracking for new goals to process
static mut NEW_GOAL_INDEX: Option<usize> = None;
static HAS_NEW_GOAL: AtomicBool = AtomicBool::new(false);

// =============================================================================
// Static context pointers (for C callbacks)
// =============================================================================

static mut SHIM_CTX: Option<*const ShimContext> = None;

/// Key expressions for action channels (null-terminated)
const SEND_GOAL_KEYEXPR: &[u8] = b"demo/fibonacci/_action/send_goal\0";
const CANCEL_GOAL_KEYEXPR: &[u8] = b"demo/fibonacci/_action/cancel_goal\0";
const GET_RESULT_KEYEXPR: &[u8] = b"demo/fibonacci/_action/get_result\0";
const FEEDBACK_KEYEXPR: &[u8] = b"demo/fibonacci/_action/feedback\0";
const STATUS_KEYEXPR: &[u8] = b"demo/fibonacci/_action/status\0";

/// Request counter
static REQUEST_COUNT: AtomicU32 = AtomicU32::new(0);

// =============================================================================
// SendGoal Service Callback
// =============================================================================

/// SendGoal request format:
/// - goal_id: UUID (16 bytes)
/// - goal: FibonacciGoal (order: i32)
///
/// SendGoal response format:
/// - accepted: bool (1 byte)
/// - stamp: Time (sec: i32, nanosec: u32)
extern "C" fn on_send_goal(
    _keyexpr: *const i8,
    _keyexpr_len: usize,
    payload: *const u8,
    payload_len: usize,
    _ctx: *mut c_void,
) {
    let count = REQUEST_COUNT.fetch_add(1, Ordering::Relaxed);
    info!("[{}] send_goal request ({} bytes)", count, payload_len);

    // Safety: payload is valid for payload_len bytes
    let payload_slice = if payload.is_null() || payload_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(payload, payload_len) }
    };

    // Deserialize request (goal_id + goal)
    let mut reader = CdrReader::new(payload_slice);

    // Read goal_id (UUID - 16 bytes as a sequence)
    let mut goal_id = [0u8; 16];
    let uuid_len = match reader.read_u32() {
        Ok(len) => len as usize,
        Err(_) => {
            info!("[{}] Failed to read UUID length", count);
            return;
        }
    };
    if uuid_len != 16 {
        info!("[{}] Invalid UUID length: {}", count, uuid_len);
        return;
    }
    for i in 0..16 {
        goal_id[i] = match reader.read_u8() {
            Ok(b) => b,
            Err(_) => {
                info!("[{}] Failed to read UUID byte", count);
                return;
            }
        };
    }

    // Read goal (FibonacciGoal)
    let goal = match FibonacciGoal::deserialize(&mut reader) {
        Ok(g) => g,
        Err(_) => {
            info!("[{}] Failed to deserialize goal", count);
            return;
        }
    };

    info!(
        "[{}] Goal request: order={}, id={:02x}{:02x}{:02x}{:02x}...",
        count, goal.order, goal_id[0], goal_id[1], goal_id[2], goal_id[3]
    );

    // Try to accept the goal
    // SAFETY: Callback runs in interrupt context, protected by single-writer pattern
    let accepted = unsafe {
        let mut accepted = false;
        let goals = ptr::addr_of_mut!(ACTIVE_GOALS);
        for i in 0..MAX_GOALS {
            if !(*goals)[i].active {
                (*goals)[i] = ActiveGoal {
                    goal_id,
                    order: goal.order,
                    status: GoalStatus::Accepted,
                    result: None,
                    active: true,
                };
                *ptr::addr_of_mut!(NEW_GOAL_INDEX) = Some(i);
                HAS_NEW_GOAL.store(true, Ordering::SeqCst);
                accepted = true;
                info!("[{}] Goal accepted (slot {})", count, i);
                break;
            }
        }
        accepted
    };

    // Create response
    let mut response_buf = [0u8; 64];
    let mut writer = CdrWriter::new(&mut response_buf);

    // Write accepted (bool as u8)
    if writer.write_u8(if accepted { 1 } else { 0 }).is_err() {
        info!("[{}] Failed to write accepted", count);
        return;
    }

    // Write stamp (Time: sec=0, nanosec=0)
    if writer.write_i32(0).is_err() || writer.write_u32(0).is_err() {
        info!("[{}] Failed to write stamp", count);
        return;
    }

    let response_len = writer.position();

    // Send reply
    // SAFETY: SHIM_CTX is initialized before callbacks are registered
    unsafe {
        if let Some(ctx_ptr) = *ptr::addr_of!(SHIM_CTX) {
            let ctx = &*ctx_ptr;
            if let Err(e) = ctx.query_reply(SEND_GOAL_KEYEXPR, &response_buf[..response_len], None)
            {
                info!("[{}] Failed to send reply: {}", count, e);
            } else {
                info!("[{}] Sent send_goal response: accepted={}", count, accepted);
            }
        }
    }
}

// =============================================================================
// CancelGoal Service Callback
// =============================================================================

/// CancelGoal request format:
/// - goal_info: GoalInfo (goal_id: UUID, stamp: Time)
///
/// CancelGoal response format:
/// - return_code: i8
/// - goals_canceling: GoalInfo[] (sequence)
extern "C" fn on_cancel_goal(
    _keyexpr: *const i8,
    _keyexpr_len: usize,
    payload: *const u8,
    payload_len: usize,
    _ctx: *mut c_void,
) {
    let count = REQUEST_COUNT.fetch_add(1, Ordering::Relaxed);
    info!("[{}] cancel_goal request ({} bytes)", count, payload_len);

    // Safety: payload is valid for payload_len bytes
    let payload_slice = if payload.is_null() || payload_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(payload, payload_len) }
    };

    // Deserialize request (goal_info)
    let mut reader = CdrReader::new(payload_slice);

    // Read goal_id (UUID - 16 bytes as a sequence)
    let mut goal_id = [0u8; 16];
    let uuid_len = reader.read_u32().unwrap_or(0) as usize;
    if uuid_len == 16 {
        for i in 0..16 {
            goal_id[i] = reader.read_u8().unwrap_or(0);
        }
    }

    info!(
        "[{}] Cancel request for goal {:02x}{:02x}{:02x}{:02x}...",
        count, goal_id[0], goal_id[1], goal_id[2], goal_id[3]
    );

    // Find and cancel the goal
    let mut return_code: i8 = 2; // ERROR_UNKNOWN_GOAL_ID
    let mut canceling_goal_id: Option<[u8; 16]> = None;

    // SAFETY: Callback access protected by single-writer pattern
    unsafe {
        let goals = ptr::addr_of_mut!(ACTIVE_GOALS);
        for i in 0..MAX_GOALS {
            if (*goals)[i].active && (*goals)[i].goal_id == goal_id {
                if (*goals)[i].status == GoalStatus::Executing
                    || (*goals)[i].status == GoalStatus::Accepted
                {
                    (*goals)[i].status = GoalStatus::Canceling;
                    return_code = 0; // ERROR_NONE
                    canceling_goal_id = Some(goal_id);
                    info!("[{}] Goal marked for cancellation", count);
                } else {
                    return_code = 1; // ERROR_GOAL_TERMINATED
                    info!("[{}] Goal already terminated", count);
                }
                break;
            }
        }
    }

    // Create response
    let mut response_buf = [0u8; 128];
    let mut writer = CdrWriter::new(&mut response_buf);

    // Write return_code
    if writer.write_i8(return_code).is_err() {
        info!("[{}] Failed to write return_code", count);
        return;
    }

    // Write goals_canceling sequence
    let num_canceling = if canceling_goal_id.is_some() { 1u32 } else { 0u32 };
    if writer.write_u32(num_canceling).is_err() {
        info!("[{}] Failed to write sequence length", count);
        return;
    }

    if let Some(gid) = canceling_goal_id {
        // Write GoalInfo: goal_id (UUID) + stamp (Time)
        // UUID is a sequence of uint8
        if writer.write_u32(16).is_err() {
            return;
        }
        for b in &gid {
            if writer.write_u8(*b).is_err() {
                return;
            }
        }
        // stamp (sec=0, nanosec=0)
        if writer.write_i32(0).is_err() || writer.write_u32(0).is_err() {
            return;
        }
    }

    let response_len = writer.position();

    // Send reply
    // SAFETY: SHIM_CTX is initialized before callbacks are registered
    unsafe {
        if let Some(ctx_ptr) = *ptr::addr_of!(SHIM_CTX) {
            let ctx = &*ctx_ptr;
            if let Err(e) =
                ctx.query_reply(CANCEL_GOAL_KEYEXPR, &response_buf[..response_len], None)
            {
                info!("[{}] Failed to send reply: {}", count, e);
            } else {
                info!(
                    "[{}] Sent cancel_goal response: return_code={}",
                    count, return_code
                );
            }
        }
    }
}

// =============================================================================
// GetResult Service Callback
// =============================================================================

/// GetResult request format:
/// - goal_id: UUID
///
/// GetResult response format:
/// - status: i8
/// - result: FibonacciResult
extern "C" fn on_get_result(
    _keyexpr: *const i8,
    _keyexpr_len: usize,
    payload: *const u8,
    payload_len: usize,
    _ctx: *mut c_void,
) {
    let count = REQUEST_COUNT.fetch_add(1, Ordering::Relaxed);
    info!("[{}] get_result request ({} bytes)", count, payload_len);

    // Safety: payload is valid for payload_len bytes
    let payload_slice = if payload.is_null() || payload_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(payload, payload_len) }
    };

    // Deserialize request (goal_id)
    let mut reader = CdrReader::new(payload_slice);

    // Read goal_id (UUID - 16 bytes as a sequence)
    let mut goal_id = [0u8; 16];
    let uuid_len = reader.read_u32().unwrap_or(0) as usize;
    if uuid_len == 16 {
        for i in 0..16 {
            goal_id[i] = reader.read_u8().unwrap_or(0);
        }
    }

    info!(
        "[{}] GetResult for goal {:02x}{:02x}{:02x}{:02x}...",
        count, goal_id[0], goal_id[1], goal_id[2], goal_id[3]
    );

    // Find the goal and get result
    let mut status: i8 = 0; // Unknown
    let mut result = FibonacciResult::default();

    // SAFETY: Callback read access to ACTIVE_GOALS
    unsafe {
        let goals = ptr::addr_of!(ACTIVE_GOALS);
        for i in 0..MAX_GOALS {
            if (*goals)[i].active && (*goals)[i].goal_id == goal_id {
                status = (*goals)[i].status as i8;
                if let Some(ref r) = (*goals)[i].result {
                    result = r.clone();
                }
                info!("[{}] Found goal with status={}", count, status);
                break;
            }
        }
    }

    // Create response
    let mut response_buf = [0u8; 512];
    let mut writer = CdrWriter::new(&mut response_buf);

    // Write status
    if writer.write_i8(status).is_err() {
        info!("[{}] Failed to write status", count);
        return;
    }

    // Write result
    if result.serialize(&mut writer).is_err() {
        info!("[{}] Failed to serialize result", count);
        return;
    }

    let response_len = writer.position();

    // Send reply
    // SAFETY: SHIM_CTX is initialized before callbacks are registered
    unsafe {
        if let Some(ctx_ptr) = *ptr::addr_of!(SHIM_CTX) {
            let ctx = &*ctx_ptr;
            if let Err(e) =
                ctx.query_reply(GET_RESULT_KEYEXPR, &response_buf[..response_len], None)
            {
                info!("[{}] Failed to send reply: {}", count, e);
            } else {
                info!(
                    "[{}] Sent get_result response: status={}, seq_len={}",
                    count,
                    status,
                    result.sequence.len()
                );
            }
        }
    }
}

// =============================================================================
// Feedback Publisher Helper
// =============================================================================

/// Publish feedback for a goal
fn publish_feedback(
    publisher: &ShimPublisher,
    goal_id: &[u8; 16],
    feedback: &FibonacciFeedback,
) -> Result<(), ZephyrError> {
    let mut buf = [0u8; 512];
    let mut writer = CdrWriter::new(&mut buf);

    // Write goal_id (UUID as sequence)
    writer.write_u32(16).map_err(|_| ZephyrError::SerializationError)?;
    for b in goal_id {
        writer.write_u8(*b).map_err(|_| ZephyrError::SerializationError)?;
    }

    // Write feedback
    feedback
        .serialize(&mut writer)
        .map_err(|_| ZephyrError::SerializationError)?;

    let len = writer.position();
    publisher.publish(&buf[..len])?;

    Ok(())
}

// =============================================================================
// Main Entry Point
// =============================================================================

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    // Initialize logging
    unsafe {
        zephyr::set_logger().ok();
    }

    info!("nano-ros Zephyr Action Server Starting");
    info!("Board: {}", zephyr::kconfig::CONFIG_BOARD);
    info!("Action: Fibonacci");

    // Connection parameters
    let locator = b"tcp/192.0.2.2:7447\0";
    info!("Connecting to zenoh router at tcp/192.0.2.2:7447");

    // Create ShimContext
    let ctx = match ShimContext::new(locator) {
        Ok(ctx) => ctx,
        Err(e) => {
            error!("Failed to create context: {}", e);
            return;
        }
    };
    info!("Session opened");

    // Store context pointer for callbacks
    // SAFETY: SHIM_CTX is written once before any callbacks are registered
    unsafe {
        *ptr::addr_of_mut!(SHIM_CTX) = Some(&ctx as *const ShimContext);
    }

    // Create queryables for action services
    info!("Creating action services...");

    let _send_goal_queryable = match unsafe {
        ctx.declare_queryable_raw(SEND_GOAL_KEYEXPR, on_send_goal, core::ptr::null_mut())
    } {
        Ok(q) => {
            info!("  send_goal service ready");
            q
        }
        Err(e) => {
            error!("Failed to create send_goal queryable: {}", e);
            return;
        }
    };

    let _cancel_goal_queryable = match unsafe {
        ctx.declare_queryable_raw(CANCEL_GOAL_KEYEXPR, on_cancel_goal, core::ptr::null_mut())
    } {
        Ok(q) => {
            info!("  cancel_goal service ready");
            q
        }
        Err(e) => {
            error!("Failed to create cancel_goal queryable: {}", e);
            return;
        }
    };

    let _get_result_queryable = match unsafe {
        ctx.declare_queryable_raw(GET_RESULT_KEYEXPR, on_get_result, core::ptr::null_mut())
    } {
        Ok(q) => {
            info!("  get_result service ready");
            q
        }
        Err(e) => {
            error!("Failed to create get_result queryable: {}", e);
            return;
        }
    };

    // Create publishers for feedback and status
    let feedback_publisher = match ctx.declare_publisher(FEEDBACK_KEYEXPR) {
        Ok(p) => {
            info!("  feedback publisher ready");
            p
        }
        Err(e) => {
            error!("Failed to create feedback publisher: {}", e);
            return;
        }
    };

    let _status_publisher = match ctx.declare_publisher(STATUS_KEYEXPR) {
        Ok(p) => {
            info!("  status publisher ready");
            p
        }
        Err(e) => {
            error!("Failed to create status publisher: {}", e);
            return;
        }
    };

    info!("Action server ready: /demo/fibonacci");
    info!("Waiting for action goals...");
    info!("(Run native-rs-action-client or a zenoh-based client)");

    // Main loop - process goals
    loop {
        // Check for new goals to execute
        if HAS_NEW_GOAL.load(Ordering::SeqCst) {
            HAS_NEW_GOAL.store(false, Ordering::SeqCst);

            // SAFETY: Single-threaded access to NEW_GOAL_INDEX protected by HAS_NEW_GOAL atomic
            if let Some(index) = unsafe { (*ptr::addr_of_mut!(NEW_GOAL_INDEX)).take() } {
                // Get goal data
                // SAFETY: Single-threaded main loop, callbacks only write to different fields
                let (goal_id, order) = unsafe {
                    let goals = ptr::addr_of!(ACTIVE_GOALS);
                    ((*goals)[index].goal_id, (*goals)[index].order)
                };

                info!("Executing goal: order={}", order);

                // Set status to executing
                // SAFETY: Single-threaded main loop
                unsafe {
                    let goals = ptr::addr_of_mut!(ACTIVE_GOALS);
                    (*goals)[index].status = GoalStatus::Executing;
                }

                // Compute Fibonacci sequence with feedback
                let mut sequence: heapless::Vec<i32, 64> = heapless::Vec::new();

                for i in 0..=order {
                    // Check for cancellation
                    // SAFETY: Reading status field, callbacks may update it
                    let cancelled = unsafe {
                        let goals = ptr::addr_of!(ACTIVE_GOALS);
                        (*goals)[index].status == GoalStatus::Canceling
                    };
                    if cancelled {
                        info!("Goal cancelled at step {}", i);
                        // SAFETY: Single-threaded main loop
                        unsafe {
                            let goals = ptr::addr_of_mut!(ACTIVE_GOALS);
                            (*goals)[index].status = GoalStatus::Canceled;
                            (*goals)[index].result = Some(FibonacciResult {
                                sequence: sequence.clone(),
                            });
                        }
                        break;
                    }

                    let next_val = if i == 0 {
                        0
                    } else if i == 1 {
                        1
                    } else {
                        let len = sequence.len();
                        sequence[len - 1] + sequence[len - 2]
                    };

                    let _ = sequence.push(next_val);

                    // Send feedback
                    let feedback = FibonacciFeedback {
                        sequence: sequence.clone(),
                    };

                    if let Err(e) = publish_feedback(&feedback_publisher, &goal_id, &feedback) {
                        error!("Failed to publish feedback: {:?}", e);
                    } else {
                        info!("Feedback: {:?}", feedback.sequence);
                    }

                    // Simulate computation time
                    zephyr::time::sleep(zephyr::time::Duration::millis(500));
                }

                // Check final status
                // SAFETY: Reading status field
                let status = unsafe {
                    let goals = ptr::addr_of!(ACTIVE_GOALS);
                    (*goals)[index].status
                };
                if status == GoalStatus::Executing {
                    // Goal completed successfully
                    let result = FibonacciResult { sequence };
                    info!("Goal completed: {:?}", result.sequence);

                    // SAFETY: Single-threaded main loop
                    unsafe {
                        let goals = ptr::addr_of_mut!(ACTIVE_GOALS);
                        (*goals)[index].status = GoalStatus::Succeeded;
                        (*goals)[index].result = Some(result);
                    }
                }
            }
        }

        // Sleep briefly
        zephyr::time::sleep(zephyr::time::Duration::millis(100));
    }
}
