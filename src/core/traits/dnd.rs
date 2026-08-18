//! Do‑Not‑Do (DND) safety graph — MAX‑TIER
//!
//! Defines:
//! - ForbiddenAction (task + reason)
//! - DoNotDoGraph (collection of forbidden actions)
//! - DoNotDoAgent (agents with safety gating)

use std::fmt::Debug;

use super::agent_state::AgentState;
use super::task::Task;

// ============================================================================
// FORBIDDEN ACTION
// ============================================================================

#[derive(Clone, Debug)]
pub struct ForbiddenAction {
    pub task_name: String,
    pub reason: String,
}

impl ForbiddenAction {
    pub fn new(task: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            task_name: task.into(),
            reason: reason.into(),
        }
    }
}

// ============================================================================
// DO‑NOT‑DO GRAPH
// ============================================================================

#[derive(Clone, Debug)]
pub struct DoNotDoGraph {
    pub forbidden: Vec<ForbiddenAction>,
}

impl DoNotDoGraph {
    pub fn new() -> Self {
        Self { forbidden: Vec::new() }
    }

    pub fn forbid(&mut self, task: impl Into<String>, reason: impl Into<String>) {
        self.forbidden.push(ForbiddenAction::new(task, reason));
    }

    pub fn is_forbidden(&self, task: &Task) -> Option<String> {
        let name = &task.name;
        self.forbidden
            .iter()
            .find(|f| &f.task_name == name)
            .map(|f| f.reason.clone())
    }
}

// ============================================================================
// DO‑NOT‑DO AGENT TRAIT — FIXED
// ============================================================================
//
// IMPORTANT:
// - This trait MUST NOT define `allowed()` in terms of a method that the agent
//   does not implement.
// - Agents implementing this trait MUST provide `dnd_graph()`.
// - `allowed()` is now guaranteed safe and non‑ambiguous.
//

pub trait DoNotDoAgent<S: AgentState>: Send + Sync {
    /// Return the DND graph for this agent.
    fn dnd_graph(&self) -> &DoNotDoGraph;

    /// Mutable access to the DND graph.
    fn dnd_graph_mut(&mut self) -> &mut DoNotDoGraph;

    /// Whether this agent is allowed to run this task.
    fn allowed(&self, task: &Task) -> bool {
        self.dnd_graph().is_forbidden(task).is_none()
    }
}

