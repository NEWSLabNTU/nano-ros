#![no_std]

pub mod talker {
    use nros::{
        Callback, CallbackCtx, CancelResponse, CdrReader, CdrWriter, DeserError, Deserialize,
        ExecutableNode, GoalResponse, Node, NodeContext, NodeOptions, NodeResult, RosAction,
        RosMessage, RosService, SerError, Serialize, TimerDuration,
    };

    pub struct Component;

    impl Node for Component {
        const NAME: &'static str = "talker";

        fn register(context: &mut NodeContext<'_>) -> NodeResult<()> {
            let mut node = context.create_node(NodeOptions::new("talker"))?;
            let _publisher = node.create_publisher_for_topic::<StringMsg>("chatter")?;
            let _timer =
                node.create_timer_for_callback_name("cb_timer", TimerDuration::from_millis(100))?;
            // W.5.3 — a subscription whose body reads the message (exercises the
            // generated subscription dispatch + CallbackCtx payload).
            let _sub =
                node.create_subscription_for_callback_name::<StringMsg>("cb_echo", "chatter")?;
            // W.5.3 — a service whose body reads the request + writes a reply
            // (exercises the generated service trampoline + CallbackCtx reply).
            let _srv =
                node.create_service_server_for_name_with_callback::<EchoSrv>("echo", "cb_srv")?;
            // W.5.5 — an action whose goal/cancel decision bodies run (exercises
            // the generated action goal/cancel decision trampolines).
            let _act = node.create_action_server_for_name_with_callbacks::<EchoAction>(
                "echo_action",
                "cb_act",
                "cb_act",
                "cb_act",
            )?;
            Ok(())
        }
    }

    // W.5 — executable body: the timer callback publishes a counter each tick.
    impl ExecutableNode for Component {
        /// Tick counter (this component's per-instance state).
        type State = u32;

        fn init() -> Self::State {
            0
        }

        fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
            if callback.as_str() == "cb_timer" {
                *state = state.wrapping_add(1);
                let _ = ctx.publish_to_topic::<StringMsg, 64>("chatter", &StringMsg);
            } else if callback.as_str() == "cb_echo" {
                // Read the incoming message (CDR payload) from the ctx.
                if ctx.message::<StringMsg>().is_ok() {
                    *state = state.wrapping_add(10);
                }
            } else if callback.as_str() == "cb_srv" {
                // Service body: read the request, write a reply.
                let _req = ctx.message::<StringMsg>();
                let _ = ctx.reply::<StringMsg, 64>(&StringMsg);
            } else if callback.as_str() == "cb_act" {
                // Action goal/cancel decision body: the ctx sink kind selects which
                // setter applies (one succeeds, the other is a no-op here).
                let _ = ctx.set_goal_response(GoalResponse::AcceptAndExecute);
                let _ = ctx.set_cancel_response(CancelResponse::Ok);
            }
        }
    }

    /// W.5.3 service type for the demo (request + reply are both `StringMsg`).
    pub struct EchoSrv;

    impl RosService for EchoSrv {
        type Request = StringMsg;
        type Reply = StringMsg;
        const SERVICE_NAME: &'static str = "std_srvs::srv::dds_::Echo_";
        const SERVICE_HASH: &'static str = "std_srvs/Echo";
    }

    /// W.5.5 action type for the demo (goal/result/feedback all `StringMsg`).
    pub struct EchoAction;

    impl RosAction for EchoAction {
        type Goal = StringMsg;
        type Result = StringMsg;
        type Feedback = StringMsg;
        // Wire-envelope stand-ins (phase-212.K envelope types) — the metadata
        // probe never serializes the action protocol, so the user-facing type
        // doubles for all five envelopes.
        type SendGoalRequest = StringMsg;
        type SendGoalResponse = StringMsg;
        type GetResultRequest = StringMsg;
        type GetResultResponse = StringMsg;
        type FeedbackMessage = StringMsg;
        const ACTION_NAME: &'static str = "nros_test::action::dds_::Echo_";
        const ACTION_HASH: &'static str = "nros_test/Echo";
    }

    #[derive(Default)]
    pub struct StringMsg;

    impl Serialize for StringMsg {
        fn serialize(&self, _writer: &mut CdrWriter) -> Result<(), SerError> {
            Ok(())
        }
    }

    impl Deserialize for StringMsg {
        fn deserialize(_reader: &mut CdrReader) -> Result<Self, DeserError> {
            Ok(Self)
        }
    }

    impl RosMessage for StringMsg {
        const TYPE_NAME: &'static str = "std_msgs::msg::dds_::String_";
        const TYPE_HASH: &'static str = "std_msgs/String";
    }
}

/// W.5.10 — a dedicated Fibonacci action-server component for the tick-driven
/// runtime exchange test. Declares exactly one node + one action server (so the
/// generated package's `MAX_ENTITIES` matches the single-entity plan), accepts
/// the goal in `on_callback`, and drives it to completion in `tick`:
/// `for_each_active_goal` → publish growing-sequence feedback → `complete_goal`.
/// The Fibonacci message types mirror `example_interfaces` CDR byte-for-byte so
/// the prebuilt `action-client` example interoperates with this generated server.
pub mod fib_server {
    use nros::{
        Callback, CallbackCtx, CdrReader, CdrWriter, DeserError, Deserialize, ExecutableNode,
        GoalResponse, GoalStatus, Node, NodeContext, NodeOptions, NodeResult, RosAction,
        RosMessage, SerError, Serialize, TickCtx,
    };

    pub struct Component;

    impl Node for Component {
        const NAME: &'static str = "fib_server";

        fn register(context: &mut NodeContext<'_>) -> NodeResult<()> {
            let mut node = context.create_node(NodeOptions::new("fib_server"))?;
            let _act = node.create_action_server_for_name_with_callbacks::<FibonacciAction>(
                "fibonacci",
                "cb_fib_goal",
                "cb_fib_cancel",
                "cb_fib_accepted",
            )?;
            Ok(())
        }
    }

    impl ExecutableNode for Component {
        /// Ticks since the active goal appeared (drives the sequence length).
        type State = u32;

        fn init() -> Self::State {
            0
        }

        fn on_callback(
            _state: &mut Self::State,
            callback: Callback<'_>,
            ctx: &mut CallbackCtx<'_>,
        ) {
            if callback.as_str() == "cb_fib_goal" {
                // Accept + execute; `tick` drives feedback + result.
                let _ = ctx.set_goal_response(GoalResponse::AcceptAndExecute);
            }
        }

        fn tick(state: &mut Self::State, ctx: &mut TickCtx<'_>) {
            let mut goal: Option<nros::GoalId> = None;
            ctx.for_each_active_goal_for_name("fibonacci", &mut |g, _status| {
                if goal.is_none() {
                    goal = Some(*g);
                }
            });
            let Some(goal_id) = goal else {
                return;
            };
            *state = state.wrapping_add(1);
            let n = (*state as usize).min(11);
            let mut sequence: nros::heapless::Vec<i32, 64> = nros::heapless::Vec::new();
            let (mut a, mut b) = (0i32, 1i32);
            for _ in 0..n {
                let _ = sequence.push(a);
                let next = a + b;
                a = b;
                b = next;
            }
            let feedback = FibonacciFeedback {
                sequence: sequence.clone(),
            };
            let _ = ctx.publish_feedback_for_name::<FibonacciFeedback, 512>(
                "fibonacci",
                &goal_id,
                &feedback,
            );
            if n >= 11 {
                let result = FibonacciResult { sequence };
                let _ = ctx.complete_goal_for_name::<FibonacciResult, 512>(
                    "fibonacci",
                    &goal_id,
                    GoalStatus::Succeeded,
                    &result,
                );
                *state = 0;
            }
        }
    }

    // Fibonacci message types — byte-for-byte the `example_interfaces` generated
    // CDR (`write_u32(len)` + `write_i32` per element). Vendored here (not a
    // path-dep into the superproject `examples/`) to keep the codegen clone
    // standalone, but wire-identical so the example client interoperates.
    #[derive(Default)]
    pub struct FibonacciGoal {
        pub order: i32,
    }

    impl Serialize for FibonacciGoal {
        fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
            writer.write_i32(self.order)?;
            Ok(())
        }
    }

    impl Deserialize for FibonacciGoal {
        fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
            Ok(Self {
                order: reader.read_i32()?,
            })
        }
    }

    impl RosMessage for FibonacciGoal {
        const TYPE_NAME: &'static str = "example_interfaces::action::dds_::Fibonacci_Goal_";
        const TYPE_HASH: &'static str = "TypeHashNotSupported";
    }

    #[derive(Default)]
    pub struct FibonacciResult {
        pub sequence: nros::heapless::Vec<i32, 64>,
    }

    impl Serialize for FibonacciResult {
        fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
            writer.write_u32(self.sequence.len() as u32)?;
            for item in &self.sequence {
                writer.write_i32(*item)?;
            }
            Ok(())
        }
    }

    impl Deserialize for FibonacciResult {
        fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
            let len = reader.read_u32()? as usize;
            let mut sequence = nros::heapless::Vec::new();
            for _ in 0..len {
                sequence
                    .push(reader.read_i32()?)
                    .map_err(|_| DeserError::CapacityExceeded)?;
            }
            Ok(Self { sequence })
        }
    }

    impl RosMessage for FibonacciResult {
        const TYPE_NAME: &'static str = "example_interfaces::action::dds_::Fibonacci_Result_";
        const TYPE_HASH: &'static str = "TypeHashNotSupported";
    }

    #[derive(Default)]
    pub struct FibonacciFeedback {
        pub sequence: nros::heapless::Vec<i32, 64>,
    }

    impl Serialize for FibonacciFeedback {
        fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
            writer.write_u32(self.sequence.len() as u32)?;
            for item in &self.sequence {
                writer.write_i32(*item)?;
            }
            Ok(())
        }
    }

    impl Deserialize for FibonacciFeedback {
        fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
            let len = reader.read_u32()? as usize;
            let mut sequence = nros::heapless::Vec::new();
            for _ in 0..len {
                sequence
                    .push(reader.read_i32()?)
                    .map_err(|_| DeserError::CapacityExceeded)?;
            }
            Ok(Self { sequence })
        }
    }

    impl RosMessage for FibonacciFeedback {
        const TYPE_NAME: &'static str = "example_interfaces::action::dds_::Fibonacci_Feedback_";
        const TYPE_HASH: &'static str = "TypeHashNotSupported";
    }

    /// Matches `example_interfaces::action::Fibonacci`.
    pub struct FibonacciAction;

    impl RosAction for FibonacciAction {
        type Goal = FibonacciGoal;
        type Result = FibonacciResult;
        type Feedback = FibonacciFeedback;
        // Wire-envelope stand-ins (phase-212.K envelope types) — the metadata
        // probe never serializes the action protocol; the user-facing types
        // double for the envelopes.
        type SendGoalRequest = FibonacciGoal;
        type SendGoalResponse = FibonacciResult;
        type GetResultRequest = FibonacciGoal;
        type GetResultResponse = FibonacciResult;
        type FeedbackMessage = FibonacciFeedback;
        const ACTION_NAME: &'static str = "example_interfaces::action::dds_::Fibonacci_";
        const ACTION_HASH: &'static str = "TypeHashNotSupported";
    }
}

/// Bridge-test source: a minimal `std_msgs/Int32` talker on `/chatter` (one node,
/// one publisher, one timer). Used by the bridge topic-forwarding runtime test as
/// the generated bridge package's own component — it publishes on the primary
/// session (endpoint 0 / router A); the generated `register_bridges` relay then
/// forwards `/chatter` to endpoint 1 (router B), where the prebuilt `listener`
/// example receives it. CDR matches `std_msgs::msg::Int32` byte-for-byte.
pub mod chatter_talker {
    use nros::{
        Callback, CallbackCtx, CdrReader, CdrWriter, DeserError, Deserialize, ExecutableNode, Node,
        NodeContext, NodeOptions, NodeResult, RosMessage, SerError, Serialize, TimerDuration,
    };

    pub struct Component;

    impl Node for Component {
        const NAME: &'static str = "chatter_talker";

        fn register(context: &mut NodeContext<'_>) -> NodeResult<()> {
            let mut node = context.create_node(NodeOptions::new("chatter_talker"))?;
            let _publisher = node.create_publisher_for_topic::<Int32Msg>("chatter")?;
            let _timer =
                node.create_timer_for_callback_name("cb_pub", TimerDuration::from_millis(100))?;
            Ok(())
        }
    }

    impl ExecutableNode for Component {
        /// Monotonic counter published as the Int32 payload.
        type State = i32;

        fn init() -> Self::State {
            0
        }

        fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
            if callback.as_str() == "cb_pub" {
                *state = state.wrapping_add(1);
                let _ = ctx.publish_to_topic::<Int32Msg, 16>("chatter", &Int32Msg(*state));
            }
        }
    }

    /// `std_msgs/Int32` — CDR is a single `int32` field.
    #[derive(Default)]
    pub struct Int32Msg(pub i32);

    impl Serialize for Int32Msg {
        fn serialize(&self, writer: &mut CdrWriter) -> Result<(), SerError> {
            writer.write_i32(self.0)?;
            Ok(())
        }
    }

    impl Deserialize for Int32Msg {
        fn deserialize(reader: &mut CdrReader) -> Result<Self, DeserError> {
            Ok(Self(reader.read_i32()?))
        }
    }

    impl RosMessage for Int32Msg {
        const TYPE_NAME: &'static str = "std_msgs::msg::dds_::Int32_";
        const TYPE_HASH: &'static str = "TypeHashNotSupported";
    }
}
