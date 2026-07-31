use avian_agent::search::{levy_step, SearchMode, next_step};
use avian_agent::behavior_tree::{AttentionBudget, BTStatus};
use avian_core::rng::SimRng;

#[test]
fn test_levy_tail_heavy() {
    let mut rng = SimRng::from_seed(42);
    let mut steps = Vec::new();
    for _ in 0..1000 {
        steps.push(levy_step(&mut rng, 2.0));
    }
    let mean = steps.iter().sum::<f64>() / steps.len() as f64;
    assert!(mean > 1.5); // Heavy tail expectation
}

#[test]
fn test_attention_budget_not_exceeded() {
    let mut budget = AttentionBudget { total: 1.0, allocated: 0.0 };
    assert!(budget.allocate(0.7));
    assert!(!budget.allocate(0.5)); // Should fail, exceeds 1.0
}
