//! XRCE-DDS action server — Fibonacci action via XRCE Agent.
//!
//! Composes the action protocol from 2 service servers (send_goal, get_result)
//! + 1 publisher (feedback). Matches the wire format used by `nros-node`'s
//! `ConnectedActionServer`.
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR  — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)
//!   XRCE_TIMEOUT     — Server timeout in seconds (default: 30)

use nros_core::{
    heapless, CdrReader, CdrWriter, Deserialize, GoalId, GoalStatus, RosAction, Serialize,
};
use nros_rmw::{
    Publisher, QosSettings, Rmw, RmwConfig, ServiceInfo, ServiceServerTrait, Session, SessionMode,
    TopicInfo,
};
use nros_rmw_xrce::XrceRmw;
use nros_rmw_xrce::posix_udp::init_posix_udp_transport;
use std::time::Instant;

use example_interfaces::action::{Fibonacci, FibonacciFeedback, FibonacciResult};

fn main() {
    let agent_addr = std::env::var("XRCE_AGENT_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:2019".to_string());
    let domain_id: u32 = std::env::var("XRCE_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let timeout_secs: u64 = std::env::var("XRCE_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    eprintln!(
        "XRCE Action Server: agent={}, domain={}, timeout={}s",
        agent_addr, domain_id, timeout_secs
    );

    unsafe {
        init_posix_udp_transport(&agent_addr);
    }

    let config = RmwConfig {
        locator: &agent_addr,
        mode: SessionMode::Client,
        domain_id,
        node_name: "xrce_action_server",
        namespace: "",
    };

    let mut session = XrceRmw::open(&config).expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Build DDS type names for action sub-entities
    let action_type = Fibonacci::ACTION_NAME;
    let send_goal_type = format!("{}SendGoal_", action_type);
    let get_result_type = format!("{}GetResult_", action_type);
    let feedback_type = format!("{}FeedbackMessage_", action_type);

    // Create send_goal service server
    let send_goal_info =
        ServiceInfo::new("/fibonacci/_action/send_goal", &send_goal_type, "");
    let mut send_goal_server = session
        .create_service_server(&send_goal_info)
        .expect("Failed to create send_goal server");
    eprintln!("send_goal service server created");

    // Create get_result service server
    let get_result_info =
        ServiceInfo::new("/fibonacci/_action/get_result", &get_result_type, "");
    let mut get_result_server = session
        .create_service_server(&get_result_info)
        .expect("Failed to create get_result server");
    eprintln!("get_result service server created");

    // Create feedback publisher
    let feedback_topic =
        TopicInfo::new("/fibonacci/_action/feedback", &feedback_type, "");
    let feedback_publisher = session
        .create_publisher(&feedback_topic, QosSettings::BEST_EFFORT)
        .expect("Failed to create feedback publisher");
    eprintln!("feedback publisher created");

    println!("Action server ready");

    // State for completed goals
    let mut stored_result: Option<(GoalId, GoalStatus, FibonacciResult)> = None;
    let mut goal_counter: u64 = 0;

    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let mut req_buf = [0u8; 512];
    let mut reply_buf = [0u8; 512];
    let mut feedback_buf = [0u8; 512];

    while start.elapsed() < timeout {
        session.spin_once(100);

        // --- Handle send_goal requests ---
        if let Some(request) = send_goal_server
            .try_recv_request(&mut req_buf)
            .expect("recv error")
        {
            let data_len = request.data.len();
            let seq = request.sequence_number;

            // Parse: CDR header + GoalId(16 bytes) + FibonacciGoal(i32 order)
            if let Ok(mut reader) = CdrReader::new_with_header(&req_buf[..data_len]) {
                let client_goal_id = GoalId::deserialize(&mut reader).unwrap_or_default();
                let order = reader.read_i32().unwrap_or(0);
                let _ = client_goal_id; // client's proposed ID, we generate our own

                println!("Received goal: order={}", order);

                // Accept the goal
                goal_counter += 1;
                let goal_id = GoalId::from_counter(goal_counter);

                // Send goal response: bool(accepted) + i32(stamp_sec) + u32(stamp_nsec) + GoalId
                let mut writer = CdrWriter::new_with_header(&mut reply_buf).unwrap();
                writer.write_bool(true).unwrap();
                writer.write_i32(0).unwrap(); // stamp placeholder
                writer.write_u32(0).unwrap();
                goal_id.serialize(&mut writer).unwrap();
                let len = writer.position();
                send_goal_server.send_reply(seq, &reply_buf[..len]).unwrap();

                println!("Goal accepted: {:?}", goal_id);
                session.spin_once(100); // flush reply

                // Execute Fibonacci computation with feedback
                let mut sequence: heapless::Vec<i32, 64> = heapless::Vec::new();

                for i in 0..=order {
                    let val = if i <= 1 { i } else {
                        let n = sequence.len();
                        sequence[n - 1] + sequence[n - 2]
                    };
                    let _ = sequence.push(val);

                    // Publish feedback: GoalId(16 bytes) + FibonacciFeedback
                    let feedback = FibonacciFeedback {
                        sequence: sequence.clone(),
                    };
                    let mut writer = CdrWriter::new_with_header(&mut feedback_buf).unwrap();
                    goal_id.serialize(&mut writer).unwrap();
                    feedback.serialize(&mut writer).unwrap();
                    let fb_len = writer.position();
                    let _ = feedback_publisher.publish_raw(&feedback_buf[..fb_len]);

                    println!("Feedback: step={}, sequence_len={}", i, sequence.len());
                    session.spin_once(100); // flush feedback
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }

                // Store result
                let result = FibonacciResult { sequence };
                println!("Goal completed: result_len={}", result.sequence.len());
                stored_result = Some((goal_id, GoalStatus::Succeeded, result));
            }
        }

        // --- Handle get_result requests ---
        if let Some(request) = get_result_server
            .try_recv_request(&mut req_buf)
            .expect("recv error")
        {
            let data_len = request.data.len();
            let seq = request.sequence_number;

            // Parse: CDR header + GoalId(16 bytes)
            if let Ok(mut reader) = CdrReader::new_with_header(&req_buf[..data_len]) {
                let requested_id = GoalId::deserialize(&mut reader).unwrap_or_default();

                // Send result response: i8(status) + FibonacciResult
                let mut writer = CdrWriter::new_with_header(&mut reply_buf).unwrap();

                if let Some((ref stored_id, status, ref result)) = stored_result {
                    if *stored_id == requested_id || requested_id.is_zero() {
                        writer.write_i8(status as i8).unwrap();
                        result.serialize(&mut writer).unwrap();
                        println!("Sent result: status={}", status);
                    } else {
                        writer.write_i8(GoalStatus::Unknown as i8).unwrap();
                        // Empty result (sequence length 0)
                        writer.write_u32(0).unwrap();
                        println!("Unknown goal requested: {:?}", requested_id);
                    }
                } else {
                    writer.write_i8(GoalStatus::Unknown as i8).unwrap();
                    writer.write_u32(0).unwrap();
                    println!("No goal available");
                }

                let len = writer.position();
                get_result_server.send_reply(seq, &reply_buf[..len]).unwrap();
                session.spin_once(100);
            }
        }
    }

    eprintln!("Server timeout, exiting");
    let _ = session.close();
}
