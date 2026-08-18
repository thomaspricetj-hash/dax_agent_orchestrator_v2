//! Subagent — MAX‑TIER execution unit
//!
//! Implements:
//! - Unified Agent<S> for tiered agents (Ceo/Master/Sub/Micro)
//! - FractalAgent<S> (delegating tier/budget/policy)
//! - MicroAgentAcceptance<S> (tier‑aware, DND‑aware)
//! - MicroAgentFallback<S> (simple no‑op fallback)
//! - DoNotDoAgent<S> (DND safety)
//! - ScratchpadAgent<S>, CapabilityIntrospection<S>, ReflectiveAgent<S>
//!
//! This is the “worker agent” used by DAX and AgentTreeExecutor.

use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::core::traits::{
    Agent,
    AgentState,
    Task,
    AgentExecutor,
    ReflectiveAgent,
    MicroAgentAcceptance,
    MicroAgentFallback,
    FractalAgent,
};
use crate::core::traits::agent::{AgentTier, RecursionBudget, SpawnPolicy};
use crate::core::traits::collapse::CollapseStrategy;
use crate::core::traits::MergeStrategy;
use crate::core::traits::delta::DeltaState;
use crate::core::traits::cost::CostPredictor;
use crate::core::traits::dnd::{DoNotDoAgent, DoNotDoGraph};
use crate::core::traits::capabilities::{CapabilityIntrospection, AgentCapabilities};
use crate::core::traits::scratchpad::{Scratchpad, ScratchpadAgent};
use crate::core::traits::reflection::ReflectionData;

// ---------------------------------------------------------------------------
// Default no-op DND graph implementation used when none provided.
struct NoopDnd<S: AgentState> {
    graph: DoNotDoGraph,
    _marker: PhantomData<S>,
}

impl<S: AgentState> NoopDnd<S> {
    fn new() -> Self {
        Self {
            graph: DoNotDoGraph::new(),
            _marker: PhantomData,
        }
    }
}

impl<S: AgentState> DoNotDoAgent<S> for NoopDnd<S> {
    fn dnd_graph(&self) -> &DoNotDoGraph {
        &self.graph
    }

    fn dnd_graph_mut(&mut self) -> &mut DoNotDoGraph {
        &mut self.graph
    }
}

// ---------------------------------------------------------------------------
// Reflection result type (local helper)
#[derive(Clone, Debug)]
pub struct ReflectionResult {
    pub should_run: bool,
    pub assumptions: Vec<String>,
}

// ---------------------------------------------------------------------------
// SubAgent definition
#[derive(Clone)]
pub struct SubAgent<S, E>
where
    S: AgentState,
    E: AgentExecutor<S> + Send + Sync + 'static,
{
    name: String,
    tier: AgentTier,
    collapse: Arc<dyn CollapseStrategy<S> + Send + Sync>,
    merge: Arc<dyn MergeStrategy + Send + Sync>,
    cost: Arc<dyn CostPredictor<S> + Send + Sync>,
    executor: Arc<E>,
    budget: RecursionBudget,
    policy: SpawnPolicy,
    dnd: Arc<dyn DoNotDoAgent<S> + Send + Sync>,
    scratchpad: Scratchpad,
    reflection_blocks: Vec<String>,
    dnd_forbidden_tags: Vec<String>,
    fractal_enabled: bool,
    micro_expand_enabled: bool,
    _marker: PhantomData<S>,
}

impl<S, E> fmt::Debug for SubAgent<S, E>
where
    S: AgentState,
    E: AgentExecutor<S> + Send + Sync + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SubAgent")
            .field("name", &self.name)
            .field("tier", &self.tier)
            .finish()
    }
}

impl<S, E> SubAgent<S, E>
where
    S: AgentState,
    E: AgentExecutor<S> + Send + Sync + 'static,
{
    pub fn new(
        name: &str,
        collapse: Arc<dyn CollapseStrategy<S> + Send + Sync>,
        merge: Arc<dyn MergeStrategy + Send + Sync>,
        cost: Arc<dyn CostPredictor<S> + Send + Sync>,
        executor: Arc<E>,
        budget: RecursionBudget,
        policy: SpawnPolicy,
    ) -> Self {
        Self {
            name: name.to_string(),
            tier: AgentTier::Sub,
            collapse,
            merge,
            cost,
            executor,
            budget,
            policy,
            dnd: Arc::new(NoopDnd::<S>::new()),
            scratchpad: Scratchpad::new(),
            reflection_blocks: Vec::new(),
            dnd_forbidden_tags: Vec::new(),
            fractal_enabled: false,
            micro_expand_enabled: false,
            _marker: PhantomData,
        }
    }

    pub fn new_master(
        name: &str,
        collapse: Arc<dyn CollapseStrategy<S> + Send + Sync>,
        merge: Arc<dyn MergeStrategy + Send + Sync>,
        cost: Arc<dyn CostPredictor<S> + Send + Sync>,
        executor: Arc<E>,
        budget: RecursionBudget,
        policy: SpawnPolicy,
    ) -> Self {
        let mut agent = Self::new(name, collapse, merge, cost, executor, budget, policy);
        agent.tier = AgentTier::Master;
        agent
    }

    pub fn new_ceo(
        name: &str,
        collapse: Arc<dyn CollapseStrategy<S> + Send + Sync>,
        merge: Arc<dyn MergeStrategy + Send + Sync>,
        cost: Arc<dyn CostPredictor<S> + Send + Sync>,
        executor: Arc<E>,
        budget: RecursionBudget,
        policy: SpawnPolicy,
    ) -> Self {
        let mut agent = Self::new(name, collapse, merge, cost, executor, budget, policy);
        agent.tier = AgentTier::Ceo;
        agent
    }

    pub fn new_micro(
        name: &str,
        collapse: Arc<dyn CollapseStrategy<S> + Send + Sync>,
        merge: Arc<dyn MergeStrategy + Send + Sync>,
        cost: Arc<dyn CostPredictor<S> + Send + Sync>,
        executor: Arc<E>,
        budget: RecursionBudget,
        policy: SpawnPolicy,
    ) -> Self {
        let mut agent = Self::new(name, collapse, merge, cost, executor, budget, policy);
        agent.tier = AgentTier::Micro;
        agent
    }

    // Reflection block API
    pub fn add_reflection_block(&mut self, tag: &str) {
        if !self.reflection_blocks.iter().any(|t| t == tag) {
            self.reflection_blocks.push(tag.to_string());
        }
    }

    pub fn reflection_tags(&self) -> &[String] {
        &self.reflection_blocks
    }

    // DND forbid API
    pub fn add_dnd_forbidden_tag(&mut self, tag: &str) {
        if !self.dnd_forbidden_tags.iter().any(|t| t == tag) {
            self.dnd_forbidden_tags.push(tag.to_string());
        }
    }

    pub fn dnd_forbidden_tags(&self) -> &[String] {
        &self.dnd_forbidden_tags
    }

    // Fractal splitting API
    pub fn enable_fractal_splitting(&mut self) {
        self.fractal_enabled = true;
    }

    pub fn simple_fractal_split(&self, task: &Task) -> Vec<Task> {
        if !self.fractal_enabled {
            return Vec::new();
        }

        vec![
            Task::new(format!("{}::child_1", task.name)),
            Task::new(format!("{}::child_2", task.name)),
        ]
    }

    // Micro expansion API
    pub fn enable_micro_expansion(&mut self) {
        self.micro_expand_enabled = true;
    }

    pub fn should_expand_micro(&self) -> bool {
        self.micro_expand_enabled
            && matches!(self.tier, AgentTier::Micro)
            && self.policy.allow_micro_expand
            && self.policy.allow_micro_spawn
    }
}

// ---------------------------------------------------------------------------
// CapabilityIntrospection
impl<S, E> CapabilityIntrospection<S> for SubAgent<S, E>
where
    S: AgentState,
    E: AgentExecutor<S> + Send + Sync + 'static,
{
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities::new()
    }
}

// ---------------------------------------------------------------------------
// ScratchpadAgent
impl<S, E> ScratchpadAgent<S> for SubAgent<S, E>
where
    S: AgentState,
    E: AgentExecutor<S> + Send + Sync + 'static,
{
    fn scratchpad(&self) -> &Scratchpad {
        &self.scratchpad
    }

    fn scratchpad_mut(&mut self) -> &mut Scratchpad {
        &mut self.scratchpad
    }
}

// ---------------------------------------------------------------------------
// Agent<S> implementation
impl<S, E> Agent<S> for SubAgent<S, E>
where
    S: AgentState + Clone,
    E: AgentExecutor<S> + Send + Sync + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn tier(&self) -> AgentTier {
        self.tier
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

    fn execute(&self, state: S, task: Task) -> Box<dyn DeltaState + Send> {
        <E as AgentExecutor<S>>::run(&*self.executor, state, task)
    }
}

// ---------------------------------------------------------------------------
// DoNotDoAgent<S> implementation
impl<S, E> DoNotDoAgent<S> for SubAgent<S, E>
where
    S: AgentState,
    E: AgentExecutor<S> + Send + Sync + 'static,
{
    fn dnd_graph(&self) -> &DoNotDoGraph {
        self.dnd.dnd_graph()
    }

    fn dnd_graph_mut(&mut self) -> &mut DoNotDoGraph {
        Arc::get_mut(&mut self.dnd)
            .expect("DND graph is shared; cannot mutably borrow")
            .dnd_graph_mut()
    }
}

// ---------------------------------------------------------------------------
// ReflectiveAgent<S> implementation
impl<S, E> ReflectiveAgent<S> for SubAgent<S, E>
where
    S: AgentState,
    E: AgentExecutor<S> + Send + Sync + 'static,
{
    fn reflect(&self, _state: &S, _task: &Task) -> ReflectionData {
        let assumptions = self
            .reflection_blocks
            .iter()
            .map(|t| format!("reflection tag `{}` active", t))
            .collect::<Vec<_>>();

        ReflectionData {
            should_run: true,
            assumptions,
            predicted_delta: None,
            reason: None,
            risks: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// MicroAgentAcceptance<S> implementation
impl<S, E> MicroAgentAcceptance<S> for SubAgent<S, E>
where
    S: AgentState,
    E: AgentExecutor<S> + Send + Sync + 'static,
{
    fn should_accept(
        &self,
        _state: &S,
        task: &Task,
    ) -> crate::core::traits::micro::MicroRouteDecision {
        let graph_forbidden = self.dnd.dnd_graph().is_forbidden(task).is_some();

        if graph_forbidden {
            crate::core::traits::micro::MicroRouteDecision::reject(Some(
                "forbidden by DND graph".into(),
            ))
        } else {
            crate::core::traits::micro::MicroRouteDecision::accept(Some(format!(
                "accepted by SubAgent `{}`",
                self.name
            )))
        }
    }

    fn dnd(&self) -> &dyn DoNotDoAgent<S> {
        self
    }
}

// ---------------------------------------------------------------------------
// MicroAgentFallback<S> implementation
impl<S, E> MicroAgentFallback<S> for SubAgent<S, E>
where
    S: AgentState,
    E: AgentExecutor<S> + Send + Sync + 'static,
{
    fn fallback(&self, _state: &S, _task: &Task) -> Option<Box<dyn DeltaState + Send>> {
        None
    }
}

// ---------------------------------------------------------------------------
// FractalAgent<S> implementation
impl<S, E> FractalAgent<S> for SubAgent<S, E>
where
    S: AgentState,
    E: AgentExecutor<S> + Send + Sync + 'static,
{
    fn dnd(&self) -> &dyn DoNotDoAgent<S> {
        self
    }

    fn micro_acceptance(&self, state: &S, task: &Task) -> bool {
        let decision = <SubAgent<S, E> as MicroAgentAcceptance<S>>::should_accept(self, state, task);
        decision.accepted
    }

    fn split_into_micros(
        &self,
        task: &Task,
    ) -> Option<crate::core::traits::fractal::FractalSplit> {
        if self.tier != AgentTier::Micro {
            return None;
        }

        if !self.should_expand_micro() {
            return None;
        }

        if self.dnd_graph().is_forbidden(task).is_some() {
            return None;
        }

        if !self.reflection_blocks.is_empty() {
            return None;
        }

        let children = vec![
            Task::new(format!("{}::micro_1", task.name)),
            Task::new(format!("{}::micro_2", task.name)),
        ];

        Some(crate::core::traits::fractal::FractalSplit {
            sub_tasks: children,
            reason: Some(format!("micro fractal split of `{}`", task.name)),
            depth_increase: 1,
        })
    }

    fn split_into_child_subs(
        &self,
        task: &Task,
    ) -> Option<crate::core::traits::fractal::FractalSplit> {
        match self.tier {
            AgentTier::Ceo | AgentTier::Master | AgentTier::Sub => {}
            AgentTier::Micro => return None,
        }

        if !self.policy.allow_sub_spawn {
            return None;
        }

        if !self.fractal_enabled {
            return None;
        }

        if self.dnd_graph().is_forbidden(task).is_some() {
            return None;
        }

        if !self.reflection_blocks.is_empty() {
            return None;
        }

        let children = vec![
            Task::new(format!("{}::sub_1", task.name)),
            Task::new(format!("{}::sub_2", task.name)),
        ];

        Some(crate::core::traits::fractal::FractalSplit {
            sub_tasks: children,
            reason: Some(format!("sub fractal split of `{}`", task.name)),
            depth_increase: 1,
        })
    }
}



