//! MAX‑TIER AGENT TREE
//! Hierarchical agent recursion, depth/cost guards, provenance.

use std::marker::PhantomData;

use crate::core::traits::{
    Agent,
    AgentState,
    Task,
    FractalAgent,
    agent::{AgentTier, RecursionBudget, SpawnPolicy},
};

/// Node in the hierarchical agent tree.
#[derive(Clone, Debug)]
pub struct AgentNode {
    pub id: String,
    pub tier: AgentTier,
    pub depth: usize,
    pub children: Vec<AgentNode>,
    pub provenance: Option<String>,
    pub cost: u64,
    pub recursion_budget: RecursionBudget,
    pub spawn_policy: SpawnPolicy,
}

impl AgentNode {
    pub fn new(
        id: &str,
        tier: AgentTier,
        depth: usize,
        budget: RecursionBudget,
        policy: SpawnPolicy,
        provenance: Option<String>,
    ) -> Self {
        Self {
            id: id.to_string(),
            tier,
            depth,
            children: Vec::new(),
            provenance,
            cost: 0,
            recursion_budget: budget,
            spawn_policy: policy,
        }
    }

    pub fn add_child(&mut self, child: AgentNode) {
        self.children.push(child);
    }
}

/// Entire hierarchical agent tree.
#[derive(Clone, Debug)]
pub struct AgentTree {
    pub root: AgentNode,
    pub max_depth: usize,
    pub max_cost: u64,
    pub total_nodes: usize,
}

impl AgentTree {
    pub fn new(root_id: &str, tier: AgentTier, budget: RecursionBudget, policy: SpawnPolicy) -> Self {
        Self {
            root: AgentNode::new(root_id, tier, 0, budget.clone(), policy.clone(), Some("root".to_string())),
            max_depth: budget.max_depth,
            max_cost: budget.max_cost,
            total_nodes: 1,
        }
    }

    pub fn increment_nodes(&mut self) {
        self.total_nodes += 1;
    }
}

/// Runtime node state.
#[derive(Clone, Debug)]
pub struct AgentTreeNodeState<S: AgentState> {
    pub node: AgentNode,
    pub state: S,
}

impl<S: AgentState> AgentTreeNodeState<S> {
    pub fn new(node: AgentNode, state: S) -> Self {
        Self { node, state }
    }
}

/// Execution context for a recursive agent tree run.
#[derive(Clone, Debug)]
pub struct AgentTreeContext<S: AgentState> {
    pub tree: AgentTree,
    pub nodes: Vec<AgentTreeNodeState<S>>,
}

impl<S: AgentState> AgentTreeContext<S> {
    pub fn new(root_node: AgentNode, root_state: S) -> Self {
        Self {
            tree: AgentTree {
                max_depth: root_node.recursion_budget.max_depth,
                max_cost: root_node.recursion_budget.max_cost,
                total_nodes: 1,
                root: root_node.clone(),
            },
            nodes: vec![AgentTreeNodeState::new(root_node, root_state)],
        }
    }

    pub fn add_child(&mut self, parent_idx: usize, child_node: AgentNode, child_state: S) -> usize {
        self.tree.increment_nodes();
        self.nodes[parent_idx].node.add_child(child_node.clone());
        self.nodes.push(AgentTreeNodeState::new(child_node, child_state));
        self.nodes.len() - 1
    }
}

/// Recursive executor over a unified Agent + FractalAgent.
pub struct AgentTreeExecutor<S, A>
where
    S: AgentState + Clone,
    A: Agent<S> + FractalAgent<S>,
{
    pub agent: A,
    _marker: PhantomData<S>,
}

impl<S, A> AgentTreeExecutor<S, A>
where
    S: AgentState + Clone,
    A: Agent<S> + FractalAgent<S>,
{
    pub fn new(agent: A) -> Self {
        Self {
            agent,
            _marker: PhantomData,
        }
    }

    pub fn run(&self, root_state: S, root_task: Task) -> AgentTreeContext<S> {
        // Disambiguate trait methods: call Agent trait explicitly for budget/policy.
        let budget = crate::core::traits::agent::Agent::recursion_budget(&self.agent);
        let policy = crate::core::traits::agent::Agent::spawn_policy(&self.agent);

        let root_node = AgentNode::new(
            "root",
            crate::core::traits::agent::Agent::tier(&self.agent),
            0,
            budget.clone(),
            policy.clone(),
            Some("root".to_string()),
        );

        let mut ctx = AgentTreeContext::new(root_node, root_state);
        self.recurse(&mut ctx, 0, root_task);
        ctx
    }

    fn recurse(&self, ctx: &mut AgentTreeContext<S>, node_idx: usize, task: Task) {
        // Snapshot the node fields we need, then drop the borrow.
        let (depth, budget, policy, tier, parent_id) = {
            let node = &ctx.nodes[node_idx].node;
            (
                node.depth,
                node.recursion_budget.clone(),
                node.spawn_policy.clone(),
                node.tier,
                node.id.clone(),
            )
        };

        // Depth guard
        if depth >= budget.max_depth {
            return;
        }

        // Forbidden graph guard (use FractalAgent::dnd to get the DoNotDoAgent and check)
        if crate::core::traits::fractal::FractalAgent::dnd(&self.agent)
            .dnd_graph()
            .is_forbidden(&task)
            .is_some()
        {
            return;
        }

        let state = ctx.nodes[node_idx].state.clone();

        // Tier‑aware fractal split (call FractalAgent::split_task explicitly)
        let split = crate::core::traits::fractal::FractalAgent::split_task(&self.agent, &state, &task, depth);
        if split.is_none() {
            return;
        }
        let split = split.unwrap();

        // Cost guard (via Agent::cost_predictor)
        let predictor = crate::core::traits::agent::Agent::cost_predictor(&self.agent);
        let mut total_cost: u64 = 0;

        for sub in &split.sub_tasks {
            let c = predictor.predict_task_cost(&state, sub) as u64;
            total_cost += c;
            if total_cost > budget.max_cost {
                return;
            }
        }

        // Hybrid micro expansion (use FractalAgent methods explicitly)
        if policy.allow_micro_expand {
            let requested = crate::core::traits::fractal::FractalAgent::micro_expansion_intent(&self.agent, &state, &task);
            let _approved = crate::core::traits::fractal::FractalAgent::approve_micro_expansion(&self.agent, requested, budget.max_micros);
        }

        // Spawn children
        for sub in split.sub_tasks {
            let _child_depth = depth + split.depth_increase;

            // Tier‑aware spawn rules
            match tier {
                AgentTier::Sub => {
                    if !policy.allow_micro_spawn && !policy.allow_sub_spawn {
                        continue;
                    }
                }
                AgentTier::Master => {
                    if !policy.allow_sub_spawn {
                        continue;
                    }
                }
                AgentTier::Micro => continue,
                AgentTier::Ceo => continue,
            }

            let child_node = AgentNode::new(
                &sub.name,
                tier,
                depth + split.depth_increase,
                budget.clone(),
                policy.clone(),
                Some(parent_id.clone()),
            );

            let child_state = state.clone();
            let child_idx = ctx.add_child(node_idx, child_node, child_state);

            self.recurse(ctx, child_idx, sub);
        }
    }
}






