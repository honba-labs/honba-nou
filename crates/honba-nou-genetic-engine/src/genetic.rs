//! StockSharp Odysseus Port: Genetic Programming Operators & Pareto Optimization
//!
//! Provides:
//! - Multi-objective fitness evaluation: Sharpe, Profit Factor, Max Drawdown, and WFE.
//! - Pareto dominance relations and Pareto frontier preservation.
//! - Sub-tree crossover (swapping logic branches between parent rule trees).
//! - Point mutation (perturbing indicator parameters, swapping operators, substituting catalog indicators).
//! - Multi-objective tournament selection.

use rand::Rng;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ast::{
    AstCondition, CrossoverDirection, IndicatorTarget, PositionSizingRule, StrategyGenome,
};
use honba_nou_core::indicators::IndicatorCatalog;

/// Multi-objective fitness vector preserving the Pareto frontier.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FitnessVector {
    pub sharpe: f64,
    pub profit_factor: f64,
    /// Absolute drawdown percentage (e.g. 5.0 for 5% drawdown; lower is better)
    pub max_drawdown: f64,
    /// Walk-forward efficiency (>= 0.60 required)
    pub walk_forward_efficiency: f64,
}

impl FitnessVector {
    pub fn new(
        sharpe: f64,
        profit_factor: f64,
        max_drawdown: f64,
        walk_forward_efficiency: f64,
    ) -> Self {
        Self {
            sharpe,
            profit_factor,
            max_drawdown,
            walk_forward_efficiency,
        }
    }

    /// Pareto dominance test: returns true if `self` dominates `other` (self ≻ other).
    /// `self` is no worse in all objectives, and strictly better in at least one objective.
    pub fn dominates(&self, other: &Self) -> bool {
        let no_worse = self.sharpe >= other.sharpe
            && self.profit_factor >= other.profit_factor
            && self.max_drawdown <= other.max_drawdown // lower drawdown is better
            && self.walk_forward_efficiency >= other.walk_forward_efficiency;

        let strictly_better = self.sharpe > other.sharpe
            || self.profit_factor > other.profit_factor
            || self.max_drawdown < other.max_drawdown
            || self.walk_forward_efficiency > other.walk_forward_efficiency;

        no_worse && strictly_better
    }
}

/// Evaluated candidate strategy with fitness score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluatedCandidate {
    pub genome: StrategyGenome,
    pub fitness: FitnessVector,
    pub rank: usize, // Pareto rank: 0 is non-dominated front
}

/// Genetic programming operator configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneticConfig {
    pub crossover_prob: f64,
    pub mutation_prob: f64,
    pub subtree_swap_prob: f64,
    pub max_tree_depth: u32,
    pub tournament_size: u32,
}

impl Default for GeneticConfig {
    fn default() -> Self {
        Self {
            crossover_prob: 0.80,
            mutation_prob: 0.20,
            subtree_swap_prob: 0.50,
            max_tree_depth: 8,
            tournament_size: 3,
        }
    }
}

/// Computes the first Pareto frontier (non-dominated set) from candidates.
pub fn compute_pareto_frontier(candidates: &[EvaluatedCandidate]) -> Vec<EvaluatedCandidate> {
    let mut frontier = Vec::new();

    for (i, c1) in candidates.iter().enumerate() {
        let mut is_dominated = false;
        for (j, c2) in candidates.iter().enumerate() {
            if i != j && c2.fitness.dominates(&c1.fitness) {
                is_dominated = true;
                break;
            }
        }
        if !is_dominated {
            let mut front_candidate = c1.clone();
            front_candidate.rank = 0;
            frontier.push(front_candidate);
        }
    }

    frontier
}

/// Sub-tree crossover operator: swaps condition sub-trees between two parent genomes.
pub fn subtree_crossover<R: Rng>(
    parent_a: &StrategyGenome,
    parent_b: &StrategyGenome,
    rng: &mut R,
) -> (StrategyGenome, StrategyGenome) {
    let mut child_a = parent_a.clone();
    let mut child_b = parent_b.clone();

    child_a.id = Uuid::new_v4().to_string();
    child_b.id = Uuid::new_v4().to_string();
    child_a.generation = parent_a.generation.max(parent_b.generation) + 1;
    child_b.generation = child_a.generation;
    child_a.parent_ids = vec![parent_a.id.clone(), parent_b.id.clone()];
    child_b.parent_ids = vec![parent_b.id.clone(), parent_a.id.clone()];

    let swap_entry = rng.gen_bool(0.5);
    if swap_entry {
        std::mem::swap(&mut child_a.entry_condition, &mut child_b.entry_condition);
    } else {
        std::mem::swap(&mut child_a.exit_condition, &mut child_b.exit_condition);
    }

    (child_a, child_b)
}

/// Point mutation operator:
/// - Perturbs indicator parameters within `IndicatorCatalog` bounds.
/// - Inverts comparison operators (`>` <-> `<`).
/// - Swaps logical operators (`And` <-> `Or`).
/// - Substitutes indicators from verified catalog.
pub fn point_mutation<R: Rng>(
    genome: &mut StrategyGenome,
    catalog: &IndicatorCatalog,
    rng: &mut R,
) {
    let mutate_entry = rng.gen_bool(0.6);
    if mutate_entry {
        mutate_condition(&mut genome.entry_condition, catalog, rng);
    } else {
        mutate_condition(&mut genome.exit_condition, catalog, rng);
    }

    if rng.gen_bool(0.2) {
        match &mut genome.position_sizing {
            PositionSizingRule::FixedNotional { amount } => {
                let delta = dec!(5000.0) * Decimal::from(if rng.gen_bool(0.5) { 1 } else { -1 });
                *amount = (*amount + delta).max(dec!(10000.0));
            }
            PositionSizingRule::VolatilityAdjusted { target_vol, .. } => {
                let delta = dec!(0.005) * Decimal::from(if rng.gen_bool(0.5) { 1 } else { -1 });
                *target_vol = (*target_vol + delta).max(dec!(0.01)).min(dec!(0.10));
            }
            _ => {}
        }
    }
}

fn mutate_condition<R: Rng>(condition: &mut AstCondition, catalog: &IndicatorCatalog, rng: &mut R) {
    match condition {
        AstCondition::Threshold(t) => {
            let choice = rng.gen_range(0..3);
            match choice {
                0 => {
                    t.op = t.op.invert();
                }
                1 => {
                    if let Some(spec) = catalog.get(&t.indicator) {
                        if let Some(param_spec) = spec.parameters.first() {
                            let step = param_spec.step;
                            let delta =
                                step * Decimal::from(if rng.gen_bool(0.5) { 1 } else { -1 });
                            t.param = (t.param + delta).clamp(param_spec.min, param_spec.max);
                        }
                    }
                }
                _ => {
                    if let IndicatorTarget::Constant(ref mut c) = t.target {
                        let delta = Decimal::from(if rng.gen_bool(0.5) { 2 } else { -2 });
                        *c = (*c + delta).max(dec!(5.0)).min(dec!(95.0));
                    }
                }
            }
        }
        AstCondition::Crossover(c) => {
            c.direction = match c.direction {
                CrossoverDirection::CrossesAbove => CrossoverDirection::CrossesBelow,
                CrossoverDirection::CrossesBelow => CrossoverDirection::CrossesAbove,
            };
        }
        AstCondition::And(children) => {
            if rng.gen_bool(0.3) {
                let c = std::mem::take(children);
                *condition = AstCondition::Or(c);
            } else if !children.is_empty() {
                let idx = rng.gen_range(0..children.len());
                mutate_condition(&mut children[idx], catalog, rng);
            }
        }
        AstCondition::Or(children) => {
            if rng.gen_bool(0.3) {
                let c = std::mem::take(children);
                *condition = AstCondition::And(c);
            } else if !children.is_empty() {
                let idx = rng.gen_range(0..children.len());
                mutate_condition(&mut children[idx], catalog, rng);
            }
        }
        AstCondition::Not(inner) => {
            mutate_condition(inner, catalog, rng);
        }
        _ => {}
    }
}

/// Multi-objective tournament selection preserving Pareto frontier.
pub fn tournament_selection<'a, R: Rng>(
    population: &'a [EvaluatedCandidate],
    tournament_size: usize,
    rng: &mut R,
) -> &'a EvaluatedCandidate {
    let mut best: Option<&'a EvaluatedCandidate> = None;

    for _ in 0..tournament_size {
        let idx = rng.gen_range(0..population.len());
        let candidate = &population[idx];

        best = match best {
            None => Some(candidate),
            Some(curr) => {
                if candidate.fitness.dominates(&curr.fitness) {
                    Some(candidate)
                } else if curr.fitness.dominates(&candidate.fitness) {
                    Some(curr)
                } else if candidate.rank < curr.rank {
                    Some(candidate)
                } else if candidate.fitness.sharpe > curr.fitness.sharpe {
                    Some(candidate)
                } else {
                    Some(curr)
                }
            }
        };
    }

    best.unwrap()
}
