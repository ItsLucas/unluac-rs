//! Factor guards which enter the same continuation without repeating its effects.

use crate::hir::common::{HirDecisionNode, HirDecisionTarget};

use super::helpers::{logical_and, logical_or};

pub(super) fn merge_single_entry_guard(
    nodes: &[HirDecisionNode],
    incoming: &[usize],
    node: &mut HirDecisionNode,
) -> bool {
    for follow_truthy in [false, true] {
        let (next, shared) = if follow_truthy {
            (&node.truthy, &node.falsy)
        } else {
            (&node.falsy, &node.truthy)
        };
        let HirDecisionTarget::Node(child_ref) = next else {
            continue;
        };
        // A shared CurrentValue refers to different tests in parent and child.
        if matches!(shared, HirDecisionTarget::CurrentValue) {
            continue;
        }
        if incoming.get(child_ref.index()) != Some(&1) {
            continue;
        }
        let Some(child) = nodes.get(child_ref.index()) else {
            continue;
        };
        let (test, continuation) = if child.truthy == *shared {
            (
                if follow_truthy {
                    child.test.clone().negate()
                } else {
                    child.test.clone()
                },
                &child.falsy,
            )
        } else if child.falsy == *shared {
            (
                if follow_truthy {
                    child.test.clone()
                } else {
                    child.test.clone().negate()
                },
                &child.truthy,
            )
        } else {
            continue;
        };
        // CurrentValue belongs to the child test, not the new combined guard.
        if matches!(continuation, HirDecisionTarget::CurrentValue) {
            continue;
        }
        if follow_truthy {
            node.test = logical_and(node.test.clone(), test);
            node.truthy = continuation.clone();
        } else {
            node.test = logical_or(node.test.clone(), test);
            node.falsy = continuation.clone();
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::common::{HirDecisionNodeRef, HirExpr, ParamId};

    fn node(test: usize, truthy: HirDecisionTarget, falsy: HirDecisionTarget) -> HirDecisionNode {
        HirDecisionNode {
            id: HirDecisionNodeRef(test),
            test: HirExpr::ParamRef(ParamId(test)),
            truthy,
            falsy,
        }
    }

    fn target(index: usize) -> HirDecisionTarget {
        HirDecisionTarget::Node(HirDecisionNodeRef(index))
    }

    #[test]
    fn shared_continuation_preserves_all_guard_orientations() {
        for follow_truthy in [false, true] {
            for child_shared_truthy in [false, true] {
                let shared = target(2);
                let other = target(3);
                let parent = if follow_truthy {
                    node(0, target(1), shared.clone())
                } else {
                    node(0, shared.clone(), target(1))
                };
                let child = if child_shared_truthy {
                    node(1, shared.clone(), other.clone())
                } else {
                    node(1, other.clone(), shared.clone())
                };
                let nodes = vec![parent.clone(), child];
                let mut merged = parent;
                assert!(merge_single_entry_guard(&nodes, &[1, 1], &mut merged));
                let mut rhs = HirExpr::ParamRef(ParamId(1));
                if follow_truthy == child_shared_truthy {
                    rhs = rhs.negate();
                }
                assert_eq!(
                    merged.test,
                    if follow_truthy {
                        logical_and(HirExpr::ParamRef(ParamId(0)), rhs)
                    } else {
                        logical_or(HirExpr::ParamRef(ParamId(0)), rhs)
                    }
                );
                assert_eq!(
                    (merged.truthy, merged.falsy),
                    if follow_truthy {
                        (other, shared)
                    } else {
                        (shared, other)
                    }
                );
            }
        }
    }

    #[test]
    fn shared_child_is_not_copied_into_multiple_guards() {
        let mut parent = node(0, target(2), target(1));
        let original = parent.clone();
        let nodes = vec![parent.clone(), node(1, target(2), target(3))];
        assert!(!merge_single_entry_guard(&nodes, &[1, 2], &mut parent));
        assert_eq!(parent, original);
    }

    #[test]
    fn child_current_value_is_not_replaced_by_guard_value() {
        let mut parent = node(0, target(2), target(1));
        let original = parent.clone();
        let nodes = vec![
            parent.clone(),
            node(1, target(2), HirDecisionTarget::CurrentValue),
        ];
        assert!(!merge_single_entry_guard(&nodes, &[1, 1], &mut parent));
        assert_eq!(parent, original);
    }

    #[test]
    fn shared_expression_terminal_preserves_all_guard_orientations() {
        for follow_truthy in [false, true] {
            for child_shared_truthy in [false, true] {
                let shared = HirDecisionTarget::Expr(HirExpr::Integer(42));
                let other = target(2);
                let mut parent = if follow_truthy {
                    node(0, target(1), shared.clone())
                } else {
                    node(0, shared.clone(), target(1))
                };
                let child = if child_shared_truthy {
                    node(1, shared.clone(), other.clone())
                } else {
                    node(1, other.clone(), shared.clone())
                };
                let nodes = vec![parent.clone(), child];
                assert!(merge_single_entry_guard(&nodes, &[1, 1], &mut parent));
                assert_eq!(
                    (parent.truthy, parent.falsy),
                    if follow_truthy {
                        (other, shared)
                    } else {
                        (shared, other)
                    }
                );
            }
        }
    }

    #[test]
    fn shared_current_value_keeps_each_test_identity() {
        let mut parent = node(0, HirDecisionTarget::CurrentValue, target(1));
        let original = parent.clone();
        let nodes = vec![
            parent.clone(),
            node(1, HirDecisionTarget::CurrentValue, target(2)),
        ];
        assert!(!merge_single_entry_guard(&nodes, &[1, 1], &mut parent));
        assert_eq!(parent, original);
    }
}
