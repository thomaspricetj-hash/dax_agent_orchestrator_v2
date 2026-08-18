use std::process::Command;
use std::time::Duration;
use std::thread::sleep;

use dax_agent_orchestrator::core::traits::{
    Agent,
    AgentState,
    Task,
    delta::{DeltaState, ZeroDelta},
    executors::AgentExecutor,
    agent::{AgentTier, RecursionBudget, SpawnPolicy},
    collapse::CollapseStrategy,
    merge::MergeStrategy,
    cost::CostPredictor,
};

use dax_agent_orchestrator::engine::subagent::SubAgent;

// ---------------------------------------------------------------------------
// Dummy State + Engine Components
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct TestState;

impl AgentState for TestState {
    fn apply_delta(&mut self, _delta: &dyn DeltaState) {
        // no-op for tests
    }
}

struct DummyCollapse;

impl<S: AgentState> CollapseStrategy<S> for DummyCollapse {
    fn apply_single(&self, _state: &mut S, _delta: &dyn DeltaState) {
        // no-op collapse for tests
    }
}

struct DummyMerge;

impl MergeStrategy for DummyMerge {
    fn merge(
        &self,
        _deltas: &[Box<dyn DeltaState + Send + 'static>],
    ) -> Box<dyn DeltaState + Send + 'static> {
        Box::new(ZeroDelta {})
    }
}

struct DummyCost;

impl<S: AgentState> CostPredictor<S> for DummyCost {
    fn predict_task_cost(&self, _state: &S, _task: &Task) -> usize {
        0
    }
}

struct DummyExecutor;

impl AgentExecutor<TestState> for DummyExecutor {
    fn run(&self, _state: TestState, _task: Task) -> Box<dyn DeltaState + Send> {
        Box::new(ZeroDelta {})
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn mk_task(name: &str) -> Task {
    Task::new(name.to_string())
}

fn mk_budget() -> RecursionBudget {
    RecursionBudget {
        max_depth: 10,
        max_micros: 10,
        max_subs: 10,
        max_cost: 100,
    }
}

fn mk_policy_all() -> SpawnPolicy {
    SpawnPolicy {
        allow_sub_spawn: true,
        allow_micro_spawn: true,
        allow_micro_expand: true,
    }
}

fn mk_agent_sub() -> SubAgent<TestState, DummyExecutor> {
    SubAgent::new(
        "sub",
        std::sync::Arc::new(DummyCollapse),
        std::sync::Arc::new(DummyMerge),
        std::sync::Arc::new(DummyCost),
        std::sync::Arc::new(DummyExecutor),
        mk_budget(),
        mk_policy_all(),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn public_api_compile_smoke() {
    #[allow(unused_imports)]
    {
        use dax_agent_orchestrator::core::traits::{Agent, AgentState, Task};
        use dax_agent_orchestrator::engine::dax::DaxOrchestrator;
        use dax_agent_orchestrator::engine::subagent::SubAgent;
        use dax_agent_orchestrator::core::agent_tree::AgentTree;
    }

    assert!(true);
}

#[test]
fn tier_assignments_are_correct() {
    let sub = mk_agent_sub();

    // mk_task now used meaningfully
    let t = mk_task("tier_test");
    let _ = sub.execute(TestState, t);

    assert_eq!(Agent::tier(&sub), AgentTier::Sub);
}

#[test]
fn run_example_and_check_output() {
    let example_name = "host_agent";

    let output = Command::new("cargo")
        .args(&["run", "--example", example_name, "--quiet"])
        .output()
        .expect("failed to spawn cargo run for example");

    sleep(Duration::from_millis(50));

    assert!(
        output.status.success(),
        "example `{}` failed to run; stderr:\n{}",
        example_name,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected = "host_agent example: HostAgent type compiled and ready.";

    assert!(
        stdout.contains(expected),
        "example stdout missing expected text.\nExpected: {}\nStdout:\n{}",
        expected,
        stdout
    );
}



