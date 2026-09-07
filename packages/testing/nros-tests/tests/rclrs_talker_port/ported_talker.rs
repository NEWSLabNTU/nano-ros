use nros::*;
use nros_std_msgs_diag::msg::String as StringMsg;

fn main() -> Result<(), Box<dyn core::error::Error>> {
    let context = Context::default_from_env()?;
    let mut executor = context.create_executor()?;
    let mut node = executor.create_node("talker")?;

    let publisher = node.create_publisher::<StringMsg>("chatter")?;

    executor.spin_blocking(SpinOptions::default())?;
    Ok(())
}
