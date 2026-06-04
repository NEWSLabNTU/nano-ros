use rclrs::CreateBasicExecutor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize ROS context from environment
    let context = rclrs::Context::default_from_env()?;

    // Create executor (manages event loop)
    let executor = context.create_basic_executor();

    // Create node through executor
    let node = executor.create_node("minimal_publisher")?;

    // Create publisher for std_msgs/String on the "chatter" topic
    let publisher = node.create_publisher::<std_msgs::msg::String>("chatter")?;

    // Publish a few messages
    for i in 0..5 {
        let mut msg = std_msgs::msg::String::default();
        msg.data = format!("Hello from Rust! Message #{}", i);

        println!("Publishing: '{}'", msg.data);
        publisher.publish(msg)?;

        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    println!("✓ Published 5 messages successfully!");
    Ok(())
}
