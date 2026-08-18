//! HostAgent — MAX‑TIER unified agent
//!
//! Top-level unified agent used by the example. Implements required trait
//! methods and includes `max_subs` in RecursionBudget initializer.

use std::fmt;
use std::sync::Arc;

use dax_agent_orchestrator::core::traits::{
    Agent,
    AgentState,
    Task,
    AgentExecutor,
    CollapseStrategy,
    MergeStrategy,
    CostPredictor,
    MicroAgentAcceptance,
    MicroAgentExecutor,
    MicroAgentFallback,
    FractalAgent,
    ReflectiveAgent,
    ScratchpadAgent,
    DoNotDoAgent,
    CapabilityIntrospection,
    AgentCapabilities,
    ReflectionData,
    FractalSplit,
    Scratchpad,
    DoNotDoGraph,
};
use dax_agent_orchestrator::core::traits::agent::{AgentTier, RecursionBudget, SpawnPolicy};
use dax_agent_orchestrator::core::traits::delta::DeltaState;

// ============================================================================
// HOST AGENT STRUCTURE
// ============================================================================

pub struct HostAgent<S: AgentState + Clone> {
    pub name: String,

    pub collapse: Arc<dyn CollapseStrategy<S> + Send + Sync>,
    pub merge: Arc<dyn MergeStrategy + Send + Sync>,
    pub cost: Arc<dyn CostPredictor<S> + Send + Sync>,
    pub executor: Arc<dyn AgentExecutor<S> + Send + Sync>,

    pub scratchpad: Scratchpad,
    pub dnd: DoNotDoGraph,

    // Provide simple defaults for budget/policy so HostAgent implements Agent cleanly.
    pub budget: RecursionBudget,
    pub policy: SpawnPolicy,
}

impl<S: AgentState + Clone> HostAgent<S> {
    pub fn new(
        name: impl Into<String>,
        collapse: Arc<dyn CollapseStrategy<S> + Send + Sync>,
        merge: Arc<dyn MergeStrategy + Send + Sync>,
        cost: Arc<dyn CostPredictor<S> + Send + Sync>,
        executor: Arc<dyn AgentExecutor<S> + Send + Sync>,
    ) -> Self {
        Self {
            name: name.into(),
            collapse,
            merge,
            cost,
            executor,
            scratchpad: Scratchpad::new(),
            dnd: DoNotDoGraph::new(),
            budget: RecursionBudget {
                max_depth: 32,
                max_micros: 16,
                max_subs: 8,          // <-- added missing field
                max_cost: u64::MAX,
            },
            policy: SpawnPolicy {
                allow_micro_spawn: true,
                allow_sub_spawn: true,
                allow_micro_expand: true,
            },
        }
    }
}

// Provide a compact Debug impl that doesn't require the trait objects to be Debug.
impl<S: AgentState + Clone> fmt::Debug for HostAgent<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostAgent")
            .field("name", &self.name)
            .field("scratchpad_present", &true)
            .field("dnd_present", &true)
            .finish()
    }
}

// ============================================================================
// MICRO‑AGENT ACCEPTANCE
// ============================================================================

impl<S: AgentState + Clone> MicroAgentAcceptance<S> for HostAgent<S> {
    fn should_accept(
        &self,
        _state: &S,
        task: &Task,
    ) -> dax_agent_orchestrator::core::traits::micro::MicroRouteDecision {
        if let Some(reason) = self.dnd.is_forbidden(task) {
            return dax_agent_orchestrator::core::traits::micro::MicroRouteDecision::reject(Some(reason));
        }

        dax_agent_orchestrator::core::traits::micro::MicroRouteDecision::accept(None)
    }

    fn priority(&self) -> f32 {
        10.0 // host agent has highest priority
    }

    fn name(&self) -> Option<String> {
        Some(self.name.clone())
    }

    // Required by the MicroAgentAcceptance trait: return the DoNotDo provider.
    fn dnd(&self) -> &dyn DoNotDoAgent<S> {
        self
    }
}

// ============================================================================
// MICRO‑AGENT EXECUTION
// ============================================================================

impl<S: AgentState + Clone> MicroAgentExecutor<S> for HostAgent<S> {
    fn execute(
        &self,
        state: &S,
        task: &Task,
    ) -> Box<dyn DeltaState + Send> {
        // AgentExecutor::run takes owned state and task in this codebase.
        // Clone the references and dispatch to the underlying executor.
        self.executor.run(state.clone(), task.clone())
    }
}

// ============================================================================
// MICRO‑AGENT FALLBACK
// ============================================================================

impl<S: AgentState + Clone> MicroAgentFallback<S> for HostAgent<S> {
    fn fallback(
        &self,
        _state: &S,
        _task: &Task,
    ) -> Option<Box<dyn DeltaState + Send>> {
        None
    }

    fn reason(&self) -> Option<String> {
        Some("HostAgent fallback not implemented".to_string())
    }
}

// ============================================================================
// FRACTAL AGENT
// ============================================================================

impl<S: AgentState + Clone> FractalAgent<S> for HostAgent<S> {
    fn split_task(
        &self,
        _state: &S,
        task: &Task,
        _depth: usize,
    ) -> Option<FractalSplit> {
        // Default: host agent does not split tasks; return the task as-is.
        Some(FractalSplit::new(vec![task.clone()]))
    }

    // Delegate budget/policy to the stored values.
    fn recursion_budget(&self) -> RecursionBudget {
        self.budget.clone()
    }

    fn spawn_policy(&self) -> SpawnPolicy {
        self.policy.clone()
    }

    // Required by FractalAgent: provide DoNotDo provider.
    fn dnd(&self) -> &dyn DoNotDoAgent<S> {
        self
    }

    // Required by FractalAgent: micro acceptance hook.
    fn micro_acceptance(&self, state: &S, task: &Task) -> bool {
        let decision = <HostAgent<S> as MicroAgentAcceptance<S>>::should_accept(self, state, task);
        decision.accepted
    }
}

// ============================================================================
// REFLECTIVE AGENT
// ============================================================================

impl<S: AgentState + Clone> ReflectiveAgent<S> for HostAgent<S> {
    fn reflect(
        &self,
        _state: &S,
        task: &Task,
    ) -> ReflectionData {
        let mut r = ReflectionData::new();
        r.assumptions.push(format!("HostAgent assumes '{}' is valid", task.name));
        r
    }
}

// ============================================================================
// SCRATCHPAD AGENT
// ============================================================================

impl<S: AgentState + Clone> ScratchpadAgent<S> for HostAgent<S> {
    fn scratchpad(&self) -> &Scratchpad {
        &self.scratchpad
    }

    fn scratchpad_mut(&mut self) -> &mut Scratchpad {
        &mut self.scratchpad
    }
}

// ============================================================================
// DND AGENT
// ============================================================================

impl<S: AgentState + Clone> DoNotDoAgent<S> for HostAgent<S> {
    fn dnd_graph(&self) -> &DoNotDoGraph {
        &self.dnd
    }

    fn dnd_graph_mut(&mut self) -> &mut DoNotDoGraph {
        &mut self.dnd
    }
}

// ============================================================================
// CAPABILITY INTROSPECTION
// ============================================================================

impl<S: AgentState + Clone> CapabilityIntrospection<S> for HostAgent<S> {
    fn capabilities(&self) -> AgentCapabilities {
        let mut c = AgentCapabilities::new();
        c.can_reflect = true;
        c.can_fractal = true;
        c.has_scratchpad = true;
        c.has_dnd = true;
        c.can_merge = true;
        c.can_collapse = true;
        c.can_predict_cost = true;
        c
    }
}

// ============================================================================
// UNIFIED AGENT IMPLEMENTATION
// ============================================================================

impl<S: AgentState + Clone> Agent<S> for HostAgent<S> {
    fn name(&self) -> &str {
        &self.name
    }

    fn tier(&self) -> AgentTier {
        AgentTier::Ceo
    }

    fn recursion_budget(&self) -> RecursionBudget {
        self.budget.clone()
    }

    fn spawn_policy(&self) -> SpawnPolicy {
        self.policy.clone()
    }

    fn cost_predictor(&self) -> Arc<dyn CostPredictor<S> + Send + Sync> {
        self.cost.clone()
    }

    fn collapse_strategy(&self) -> Arc<dyn CollapseStrategy<S> + Send + Sync> {
        self.collapse.clone()
    }

    fn merge_strategy(&self) -> Arc<dyn MergeStrategy + Send + Sync> {
        self.merge.clone()
    }

    fn executor(&self) -> Arc<dyn AgentExecutor<S> + Send + Sync> {
        self.executor.clone()
    }
}

// ============================================================================
// Example entrypoint for the example binary
// ============================================================================

fn main() {
    // Minimal smoke test for the example binary.
    println!("host_agent example: HostAgent type compiled and ready.");
}






