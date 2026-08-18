//! Micro‑agent system — MAX‑TIER
//!
//! Defines:
//! - MicroRouteDecision
//! - MicroAgentAcceptance
//! - MicroAgentMetadata
//! - MicroAgentExecutor
//! - MicroAgentFallback
//!
//! Upgraded for:
//! - Deterministic vs adaptive micros
//! - Micro expansion intents
//! - Tier‑aware acceptance (via Agent)
//! - Spawn‑policy enforcement (via Agent)
//! - Recursion‑budget awareness (via Agent)
//! - Forbidden‑graph safety checks
//! - Cost prediction integration (via Agent)

use std::fmt::Debug;

use super::agent_state::AgentState;
use super::task::Task;
use super::delta::DeltaState;

use super::dnd::DoNotDoAgent;

// ============================================================================
// MICRO‑AGENT ROUTING DECISION
// ============================================================================

#[derive(Clone, Debug)]
pub struct MicroRouteDecision {
    pub accepted: bool,
    pub reason: Option<String>,
    pub priority: f32,
}

impl MicroRouteDecision {
    pub fn accept(reason: Option<String>) -> Self {
        Self {
            accepted: true,
            reason,
            priority: 1.0,
        }
    }

    pub fn reject(reason: Option<String>) -> Self {
        Self {
            accepted: false,
            reason,
            priority: 0.0,
        }
    }

    pub fn with_priority(mut self, p: f32) -> Self {
        self.priority = p;
        self
    }
}

// ============================================================================
// MICRO‑AGENT ACCEPTANCE RULES — UPGRADED
// ============================================================================

pub trait MicroAgentAcceptance<S: AgentState>: Send + Sync {
    /// Decide whether this micro‑agent should run.
    ///
    /// Implementations SHOULD enforce:
    /// - agent tier (via Agent::tier)
    /// - spawn policy (via Agent::spawn_policy)
    /// - recursion budget (via Agent::recursion_budget)
    /// - forbidden graph (via DoNotDoAgent)
    /// - cost prediction (via Agent::cost_predictor)
    fn should_accept(&self, state: &S, task: &Task) -> MicroRouteDecision;

    /// Optional: micro‑agent priority.
    fn priority(&self) -> f32 {
        1.0
    }

    /// Optional: micro‑agent name.
    fn name(&self) -> Option<String> {
        None
    }

    /// Whether this micro is deterministic or adaptive.
    fn deterministic(&self) -> bool {
        true
    }

    /// Adaptive micros may request expansion.
    fn expansion_intent(&self, _state: &S, _task: &Task) -> Option<usize> {
        None
    }

    /// Access to DND graph for safety checks.
    fn dnd(&self) -> &dyn DoNotDoAgent<S>;
}

// ============================================================================
// MICRO‑AGENT METADATA
// ============================================================================

#[derive(Clone, Debug)]
pub struct MicroAgentMetadata {
    pub name: String,
    pub accepted: bool,
    pub priority: f32,
    pub provenance: Option<String>,
}

impl MicroAgentMetadata {
    pub fn new(name: &str, accepted: bool, priority: f32) -> Self {
        Self {
            name: name.to_string(),
            accepted,
            priority,
            provenance: None,
        }
    }
}

// ============================================================================
// MICRO‑AGENT EXECUTOR
// ============================================================================

/// Executor for a single micro‑agent. Returns a boxed delta produced by the micro agent.
///
/// Note: the top‑level `Agent::execute` is intentionally separate from this trait's
/// `execute` so orchestrators can call either the micro executor directly or the
/// agent's end‑to‑end executor without ambiguity.
pub trait MicroAgentExecutor<S: AgentState>: Send + Sync {
    /// Execute the micro‑agent and produce a delta.
    fn execute(&self, state: &S, task: &Task) -> Box<dyn DeltaState + Send>;

    /// Optional metadata.
    fn metadata(&self) -> Option<MicroAgentMetadata> {
        None
    }
}

// ============================================================================
// MICRO‑AGENT FALLBACK
// ============================================================================

pub trait MicroAgentFallback<S: AgentState>: Send + Sync {
    /// Fallback execution path.
    fn fallback(&self, state: &S, task: &Task) -> Option<Box<dyn DeltaState + Send>>;

    /// Optional fallback reason.
    fn reason(&self) -> Option<String> {
        None
    }
}
