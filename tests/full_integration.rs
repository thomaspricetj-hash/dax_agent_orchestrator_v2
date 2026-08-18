// tests/full_integration.rs
//
// Full integration test using REAL SubAgent + REAL traits.

use std::any::Any;
use std::process::Command;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

use dax_agent_orchestrator::core::{
    traits::{
        agent::{RecursionBudget, SpawnPolicy},
        AgentState,
        Task,
        collapse::CollapseStrategy,
        cost::CostPredictor,
        delta::DeltaState,
        executors::AgentExecutor,
        merge::MergeStrategy,
    },
    agent_tree::AgentTreeExecutor,
};

use dax_agent_orchestrator::engine::dax::DaxOrchestrator;
use dax_agent_orchestrator::engine::subagent::SubAgent;

// -----------------------------
// Local minimal delta
// -----------------------------

#[derive(Clone)]
struct NoopDelta;

impl DeltaState for NoopDelta {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    // Required by the crate-level DeltaState re-export
    fn clone_box(&self) -> Box<dyn dax_agent_orchestrator::DeltaState + Send + 'static> {
        Box::new(self.clone())
    }
}

// -----------------------------
// Local minimal engine components
// -----------------------------

#[derive(Clone)]
struct LocalCollapse;

impl<S: AgentState> CollapseStrategy<S> for LocalCollapse {
    fn apply_single(&self, _state: &mut S, _delta: &dyn DeltaState) {
        // no-op
    }
}

#[derive(Clone)]
struct LocalMerge;

impl MergeStrategy for LocalMerge {
    fn merge(&self, _deltas: &[Box<dyn DeltaState + Send>]) -> Box<dyn DeltaState + Send> {
        Box::new(NoopDelta)
    }
}

#[derive(Clone)]
struct LocalCost;

impl<S: AgentState> CostPredictor<S> for LocalCost {
    fn predict_task_cost(&self, _state: &S, _task: &Task) -> usize {
        0
    }
}

#[derive(Clone)]
struct LocalExecutor;

impl<S: AgentState> AgentExecutor<S> for LocalExecutor {
    fn run(&self, _state: S, _task: Task) -> Box<dyn DeltaState + Send> {
        Box::new(NoopDelta)
    }
}

// -----------------------------
// Helpers
// -----------------------------

fn make_task(name: &str) -> Task {
    Task::new(name.to_string())
}

fn default_budget() -> RecursionBudget {
    RecursionBudget {
        max_depth: 4,
        max_micros: 8,
        max_subs: 4,
        max_cost: 100,
    }
}

fn default_policy() -> SpawnPolicy {
    SpawnPolicy {
        allow_micro_spawn: true,
        allow_sub_spawn: true,
        allow_micro_expand: true,
    }
}

// -----------------------------
// Integration test
// -----------------------------

#[test]
fn full_integration() {
    #[derive(Clone, Debug)]
    struct TestState;

    impl AgentState for TestState {
        fn apply_delta(&mut self, _delta: &dyn DeltaState) {
            // no-op
        }
    }

    let collapse: Arc<dyn CollapseStrategy<TestState> + Send + Sync> = Arc::new(LocalCollapse);
    let merge: Arc<dyn MergeStrategy + Send + Sync> = Arc::new(LocalMerge);
    let cost: Arc<dyn CostPredictor<TestState> + Send + Sync> = Arc::new(LocalCost);
    let exec: Arc<LocalExecutor> = Arc::new(LocalExecutor);

    let budget = default_budget();
    let policy = default_policy();

    // Use concrete LocalExecutor as E for SubAgent<S, E>
    let sub_agent: SubAgent<TestState, LocalExecutor> = SubAgent::new(
        "integration_sub",
        collapse.clone(),
        merge.clone(),
        cost.clone(),
        exec.clone(),
        budget.clone(),
        policy.clone(),
    );

    let agent_arc: Arc<SubAgent<TestState, LocalExecutor>> = Arc::new(sub_agent.clone());

    // DAX orchestrator
    let dax = DaxOrchestrator::new(agent_arc.clone());

    let state = TestState;
    let task = make_task("root");

    let result = dax.execute(state.clone(), task.clone());

    assert!(result.recursion_depth <= budget.max_depth);
    assert!(result.cost <= budget.max_cost);

    // AgentTreeExecutor
    let tree_exec = AgentTreeExecutor::new(sub_agent);
    let ctx = tree_exec.run(TestState, make_task("root"));

    assert!(ctx.tree.total_nodes >= 1);
    assert_eq!(ctx.tree.max_depth, budget.max_depth);
    assert_eq!(ctx.tree.max_cost, budget.max_cost);

    // Example binary
    let output = Command::new("cargo")
        .args(&["run", "--example", "host_agent", "--quiet"])
        .output()
        .expect("failed to run example");

    sleep(Duration::from_millis(50));

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("host_agent example: HostAgent type compiled and ready."),
        "example output missing expected text"
    );
}

