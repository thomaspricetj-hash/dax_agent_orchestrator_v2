//! Fractal recursion system — MAX‑TIER
//!
//! Defines:
//! - FractalSplit (sub‑task grouping)
//! - FractalAgent (recursive agent trait)
//!
//! Upgraded for:
//! - AgentTier awareness (via Agent)
//! - RecursionBudget enforcement (via Agent)
//! - SpawnPolicy enforcement (via Agent)
//! - Hybrid micro expansion (micros request, subs approve)
//! - Forbidden‑graph safety checks
//! - Cost prediction integration (via Agent)
//! - Depth + cost guards

use std::fmt::Debug;
use std::sync::Arc;

use super::agent_state::AgentState;
use super::task::Task;

use super::agent::{Agent, AgentTier, RecursionBudget, SpawnPolicy};
use super::dnd::DoNotDoAgent;
use super::cost::CostPredictor;

// ============================================================================
// FRACTAL SPLIT
// ============================================================================

#[derive(Clone, Debug)]
pub struct FractalSplit {
    pub sub_tasks: Vec<Task>,
    pub reason: Option<String>,
    pub depth_increase: usize,
}

impl FractalSplit {
    pub fn new(sub_tasks: Vec<Task>) -> Self {
        Self {
            sub_tasks,
            reason: None,
            depth_increase: 1,
        }
    }
}

// ============================================================================
// FRACTAL AGENT TRAIT — MAX‑TIER
// ============================================================================

pub trait FractalAgent<S: AgentState>: Send + Sync {
    /// Whether this agent supports fractal recursion.
    fn can_fractal(&self) -> bool {
        true
    }

    /// Agent tier (delegated to Agent).
    fn tier(&self) -> AgentTier
    where
        Self: Agent<S>,
    {
        crate::core::traits::agent::Agent::tier(self)
    }

    /// Recursion budget (delegated to Agent).
    fn recursion_budget(&self) -> RecursionBudget
    where
        Self: Agent<S>,
    {
        crate::core::traits::agent::Agent::recursion_budget(self)
    }

    /// Spawn policy (delegated to Agent).
    fn spawn_policy(&self) -> SpawnPolicy
    where
        Self: Agent<S>,
    {
        crate::core::traits::agent::Agent::spawn_policy(self)
    }

    /// Forbidden graph (unsafe paths).
    fn dnd(&self) -> &dyn DoNotDoAgent<S>;

    /// Split a task into sub‑tasks.
    ///
    /// MUST enforce:
    /// - recursion depth
    /// - forbidden graph
    /// - cost limits
    /// - spawn policy
    fn split_task(
        &self,
        state: &S,
        task: &Task,
        depth: usize,
    ) -> Option<FractalSplit>
    where
        Self: Agent<S>,
    {
        if !self.can_fractal() {
            return None;
        }

        // --- Depth guard ----------------------------------------------------
        let budget = crate::core::traits::agent::Agent::recursion_budget(self);
        if depth >= budget.max_depth {
            return None;
        }

        // --- Forbidden graph check ------------------------------------------
        // Disambiguate which `dnd` is intended by calling the FractalAgent trait method explicitly,
        // then access the underlying DoNotDoGraph and check `is_forbidden`.
        if crate::core::traits::fractal::FractalAgent::dnd(self)
            .dnd_graph()
            .is_forbidden(task)
            .is_some()
        {
            return None;
        }

        // --- Cost guard (via Agent::cost_predictor) -------------------------
        // Annotate the predictor with Arc<dyn CostPredictor<S> + Send + Sync> so the import is used.
        let predictor: Arc<dyn CostPredictor<S> + Send + Sync> =
            crate::core::traits::agent::Agent::cost_predictor(self);
        let predicted_cost = predictor.predict_task_cost(state, task);
        if predicted_cost as u64 > budget.max_cost {
            return None;
        }

        // --- Tier‑aware behavior --------------------------------------------
        match crate::core::traits::agent::Agent::tier(self) {
            AgentTier::Ceo => {
                // CEO never splits tasks directly.
                None
            }

            AgentTier::Master => {
                // Masters may split tasks into high‑level partitions.
                self.split_master_task(state, task, depth)
            }

            AgentTier::Sub => {
                // Subs may split tasks and spawn micros or child subs.
                self.split_sub_task(state, task, depth)
            }

            AgentTier::Micro => {
                // Micros do not split tasks.
                None
            }
        }
    }

    // ========================================================================
    // MASTER‑LEVEL SPLITTING
    // ========================================================================
    fn split_master_task(
        &self,
        _state: &S,
        _task: &Task,
        _depth: usize,
    ) -> Option<FractalSplit> {
        // Default: no master splitting unless implemented.
        None
    }

    // ========================================================================
    // SUB‑LEVEL SPLITTING (HYBRID FRACTAL)
    // ========================================================================
    fn split_sub_task(
        &self,
        state: &S,
        task: &Task,
        _depth: usize,
    ) -> Option<FractalSplit>
    where
        Self: Agent<S>,
    {
        let policy = crate::core::traits::agent::Agent::spawn_policy(self);
        let budget = crate::core::traits::agent::Agent::recursion_budget(self);

        // If subs are not allowed to spawn anything, no recursion.
        if !policy.allow_micro_spawn && !policy.allow_sub_spawn {
            return None;
        }

        // Ask micro acceptance logic if this task should be handled by micros.
        let micro_accept = crate::core::traits::fractal::FractalAgent::micro_acceptance(self, state, task);

        if micro_accept {
            // Micro expansion request (adaptive micros)
            if policy.allow_micro_expand {
                let requested = crate::core::traits::fractal::FractalAgent::micro_expansion_intent(self, state, task);
                let approved = crate::core::traits::fractal::FractalAgent::approve_micro_expansion(self, requested, budget.max_micros);
                let _ = approved; // orchestrator uses this; fractal trait just computes it
            }

            // Subs splitting into micro‑tasks.
            return self.split_into_micros(task);
        }

        // If allowed, subs may spawn child subs (bounded fractal).
        if policy.allow_sub_spawn {
            return self.split_into_child_subs(task);
        }

        None
    }

    // ========================================================================
    // MICRO‑LEVEL EXPANSION INTENT
    // ========================================================================
    fn micro_expansion_intent(&self, _state: &S, _task: &Task) -> usize {
        // Default: no expansion intent unless implemented.
        0
    }

    fn approve_micro_expansion(&self, requested: usize, max_allowed: usize) -> usize {
        requested.min(max_allowed)
    }

    // ========================================================================
    // SUB‑LEVEL SPLITTING HELPERS
    // ========================================================================
    fn split_into_micros(&self, _task: &Task) -> Option<FractalSplit> {
        None
    }

    fn split_into_child_subs(&self, _task: &Task) -> Option<FractalSplit> {
        None
    }

    // ========================================================================
    // MICRO ACCEPTANCE HOOK
    // ========================================================================
    fn micro_acceptance(&self, state: &S, task: &Task) -> bool;
}
