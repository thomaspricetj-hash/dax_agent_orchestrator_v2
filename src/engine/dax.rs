//! DAX Orchestrator — MAX‑TIER
//!
//! Unified execution pipeline:
//! - Micro‑agent routing
//! - Reflection gating
//! - Fractal recursion
//! - Collapse + merge
//! - Cost prediction
//! - Deterministic recursion guards via RecursionBudget
//!
//! This is the top‑level orchestrator used by SyntheticMind.

use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::core::traits::{
    Agent,
    AgentState,
    Task,
    FractalAgent,
    ReflectiveAgent,
    MicroAgentAcceptance,
    MicroAgentFallback,
};
use crate::core::traits::agent::RecursionBudget;

use crate::core::traits::collapse::CollapseStrategy;
use crate::core::traits::MergeStrategy;
use crate::core::traits::delta::DeltaState;
use crate::core::traits::cost::CostPredictor;

// ============================================================================
// DAX EXECUTION RESULT
// ============================================================================

pub struct DaxResult {
    pub deltas: Vec<Box<dyn DeltaState + Send>>,
    pub recursion_depth: usize,
    pub cost: u64,
}

impl fmt::Debug for DaxResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DaxResult")
            .field("deltas_count", &self.deltas.len())
            .field("recursion_depth", &self.recursion_depth)
            .field("cost", &self.cost)
            .finish()
    }
}

// ============================================================================
// DAX ORCHESTRATOR
// ============================================================================
//
// Note: the orchestrator requires `S: Clone` because it clones the state when
// executing subtasks.

#[derive(Debug)]
pub struct DaxOrchestrator<A, S>
where
    A: Agent<S>
        + FractalAgent<S>
        + ReflectiveAgent<S>
        + MicroAgentAcceptance<S>
        + MicroAgentFallback<S>,
    S: AgentState + Clone,
{
    pub agent: Arc<A>,
    pub budget: RecursionBudget,
    _state_marker: PhantomData<S>,
}

impl<A, S> DaxOrchestrator<A, S>
where
    A: Agent<S>
        + FractalAgent<S>
        + ReflectiveAgent<S>
        + MicroAgentAcceptance<S>
        + MicroAgentFallback<S>,
    S: AgentState + Clone,
{
    pub fn new(agent: Arc<A>) -> Self {
        // Disambiguate recursion_budget to use the Agent trait implementation explicitly.
        let budget = crate::core::traits::agent::Agent::recursion_budget(&*agent);
        Self {
            agent,
            budget,
            _state_marker: PhantomData,
        }
    }

    // ========================================================================
    // MAIN EXECUTION ENTRYPOINT
    // ========================================================================

    pub fn execute(&self, state: S, task: Task) -> DaxResult {
        self.execute_recursive(state, task, 0, 0)
    }

    // ========================================================================
    // RECURSIVE EXECUTION
    // ========================================================================

    fn execute_recursive(
        &self,
        mut state: S,
        task: Task,
        depth: usize,
        cost: u64,
    ) -> DaxResult {
        // Depth guard from RecursionBudget
        if depth >= self.budget.max_depth {
            return DaxResult {
                deltas: vec![],
                recursion_depth: depth,
                cost,
            };
        }

        // Reflection gating
        let reflection = self.agent.reflect(&state, &task);
        if !reflection.should_run {
            return DaxResult {
                deltas: vec![],
                recursion_depth: depth,
                cost,
            };
        }

        let mut total_cost = cost;
        if !reflection.assumptions.is_empty() {
            total_cost += reflection.assumptions.len() as u64;
            if total_cost > self.budget.max_cost {
                return DaxResult {
                    deltas: vec![],
                    recursion_depth: depth,
                    cost: total_cost,
                };
            }
        }

        // Micro‑agent acceptance + fallback
        let decision = self.agent.should_accept(&state, &task);
        if !decision.accepted {
            if let Some(fallback_delta) = self.agent.fallback(&state, &task) {
                return DaxResult {
                    deltas: vec![fallback_delta],
                    recursion_depth: depth,
                    cost: total_cost,
                };
            }

            return DaxResult {
                deltas: vec![],
                recursion_depth: depth,
                cost: total_cost,
            };
        }

        // Fractal recursion
        let split = self.agent.split_task(&state, &task, depth);
        let sub_tasks = match split {
            Some(s) => s.sub_tasks,
            None => vec![task.clone()],
        };

        // CostPredictor usage (disambiguated via Agent)
        let predictor: Arc<dyn CostPredictor<S> + Send + Sync> =
            crate::core::traits::agent::Agent::cost_predictor(&*self.agent);

        let mut all_deltas: Vec<Box<dyn DeltaState + Send>> = Vec::with_capacity(sub_tasks.len());

        for sub in sub_tasks {
            let predicted = predictor.predict_task_cost(&state, &sub) as u64;
            total_cost += predicted;

            if total_cost > self.budget.max_cost {
                break;
            }

            // Execute the subtask via the Agent implementation explicitly.
            let delta = crate::core::traits::agent::Agent::execute(&*self.agent, state.clone(), sub.clone());
            all_deltas.push(delta);
        }

        // Collapse
        {
            let cs: Arc<dyn CollapseStrategy<S> + Send + Sync> = self.agent.collapse_strategy();
            cs.apply_many(&mut state, &all_deltas);
        }

        // Merge
        {
            let ms: Arc<dyn MergeStrategy + Send + Sync> = self.agent.merge_strategy();
            let merged_delta: Box<dyn DeltaState + Send> = ms.merge(&all_deltas);
            all_deltas.push(merged_delta);
        }

        DaxResult {
            deltas: all_deltas,
            recursion_depth: depth,
            cost: total_cost,
        }
    }
}



