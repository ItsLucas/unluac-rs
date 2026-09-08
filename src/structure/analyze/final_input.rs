//! 汇总冻结最终 StructurePlan 所需的候选与边界；依赖各专题选择结果，不负责构建区域 arena。
//! 短路出口只有在另一臂的正常路径唯一汇入它时才能成为单臂 continuation；
//! 例如 `(a or b) and c` 的两臂另有共同尾部时，保留 if/else，不能把其中一臂当作尾部。

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn final_plan_input(
    branches: &[BranchCandidate],
    branch_regions: &[BranchRegionFact],
    branch_value_merges: &[BranchValueMergeCandidate],
    loops: &[LoopCandidate],
    condition_candidates: &[ShortCircuitCandidate],
    value_candidates: &[ShortCircuitCandidate],
    closed_control_dags: &[ClosedControlDagEvidence],
    residual_transfers: &[ResidualTransferEvidence],
    regions: &[RegionFact],
    scopes: &[ScopePlan],
    proto: &LoweredProto,
    cfg: &Cfg,
    dataflow: &DataflowFacts,
    graph_facts: &GraphFacts,
    exit_block: super::super::BlockRef,
    caps: ControlFlowCaps,
) -> Result<FinalPlanInput, StructureError> {
    let branch_regions = unique_branch_regions(branch_regions)?;
    let branch_value_merges = unique_branch_value_merges(branch_value_merges)?;
    let (conditions, condition_by_header) = selected_conditions(ConditionSelectionInput {
        proto,
        cfg,
        dataflow,
        loops,
        caps,
        branches,
        candidates: condition_candidates,
        closed_control_dags,
        residual_transfers,
    })?;
    let value_decisions = selected_value_decisions(
        proto,
        cfg,
        dataflow,
        loops,
        residual_transfers,
        value_candidates,
    );

    let branches = branches
        .iter()
        .cloned()
        .map(|mut branch| {
            let condition = condition_by_header.get(&branch.header).copied();
            let condition_ref = condition.and_then(|id| conditions.get(id.index()));
            let frozen_region = branch_regions
                .get(&branch.header)
                .map(|region| (*region).clone());
            let boundary_changed = frozen_region
                .as_ref()
                .is_none_or(|region| region.single_pass_fence.is_none())
                && normalize_branch_condition_boundary(
                    cfg,
                    graph_facts,
                    loops,
                    &mut branch,
                    condition_ref,
                );
            let value_merge = branch
                .merge
                .and_then(|merge| branch_value_merges.get(&(branch.header, merge)))
                .map(|candidate| (*candidate).clone());
            let region = frozen_region
                .filter(|region| !boundary_changed || Some(region.merge) == branch.merge)
                .or_else(|| {
                    branch.merge.map(|merge| {
                        BranchRegionFact::new(graph_facts, branch.header, merge, branch.kind, None)
                    })
                });
            BranchPlanInput {
                region,
                condition,
                value_merge,
                branch,
            }
        })
        .collect();
    let loops = loops
        .iter()
        .cloned()
        .map(|loop_| {
            let condition = required_loop_condition_header(cfg, &loop_)
                .and_then(|header| condition_by_header.get(&header).copied());
            let continuation = loop_continuation(
                proto,
                &loop_,
                condition.and_then(|id| conditions.get(id.index())),
                cfg,
                graph_facts,
                exit_block,
            );
            LoopPlanInput {
                carried_values: loop_.header_value_merges.clone(),
                condition,
                continuation,
                candidate: loop_,
                semantic_continue_edges: BTreeSet::new(),
            }
        })
        .collect();
    let unstructured = regions
        .iter()
        .cloned()
        .map(|fact| UnstructuredPlanData { fact, layout: None })
        .collect();

    Ok(FinalPlanInput {
        branches,
        loops,
        conditions,
        value_decisions,
        scopes: scopes.to_vec(),
        unstructured,
        residual_transfers: residual_transfers.to_vec(),
    })
}

pub(super) fn normalize_branch_condition_boundary(
    cfg: &Cfg,
    graph_facts: &GraphFacts,
    loops: &[LoopCandidate],
    branch: &mut BranchCandidate,
    condition: Option<&ConditionPlanInput>,
) -> bool {
    let Some(condition) = condition.filter(|condition| condition.candidate.nodes.len() > 1) else {
        return false;
    };
    let ShortCircuitExit::BranchExit { truthy, falsy } = condition.candidate.exit else {
        return false;
    };
    if let Some((then_entry, continuation)) =
        loop_guard_boundary(branch.header, truthy, falsy, loops)
    {
        branch.then_entry = then_entry;
        branch.else_entry = None;
        branch.merge = Some(continuation);
        branch.kind = BranchKind::Guard;
        branch.invert_hint = false;
        return true;
    }
    if let Some((then_entry, continuation)) =
        branch_endpoint_boundary(cfg, graph_facts, loops, truthy, falsy)
    {
        branch.then_entry = then_entry;
        branch.else_entry = None;
        branch.merge = Some(continuation);
        branch.kind = BranchKind::Guard;
        branch.invert_hint = false;
        return true;
    }
    let Some(merge) = branches::find_soft_merge(cfg, graph_facts, branch.header, truthy, falsy)
    else {
        return false;
    };
    if merge == truthy
        || merge == falsy
        || condition.candidate.blocks.contains(&merge)
        || branch.merge.is_some_and(|current| {
            current != truthy && current != falsy && !graph_facts.dominates(merge, current)
        })
    {
        return false;
    }
    branch.then_entry = truthy;
    branch.else_entry = Some(falsy);
    branch.merge = Some(merge);
    branch.kind = BranchKind::IfElse;
    branch.invert_hint = false;
    true
}

pub(super) fn branch_endpoint_boundary(
    cfg: &Cfg,
    graph_facts: &GraphFacts,
    loops: &[LoopCandidate],
    truthy: super::super::BlockRef,
    falsy: super::super::BlockRef,
) -> Option<(super::super::BlockRef, super::super::BlockRef)> {
    // frontier 只证明某条路径汇入出口，不证明所有本轮正常路径都汇入它。第二个汇入点
    // 必须留在分支之外；函数出口、共同 loop owner 的退出以及回边不属于本轮 continuation。
    let joins_only_at = |from, target| {
        let mut frontier = graph_facts.dominance_frontier_blocks(from).filter(|block| {
            *block != cfg.exit_block
                && (*block == target
                    || ((!graph_facts.loop_headers.contains(block)
                        || !graph_facts.dominates(*block, from))
                        && !loops.iter().any(|loop_| {
                            loop_.blocks.contains(&from)
                                && loop_.blocks.contains(&target)
                                && loop_.exits.iter().any(|exit| {
                                    exit == block
                                        || branches::transparent_jump_target(cfg, *exit)
                                            == Some(*block)
                                })
                        })))
        });
        frontier.next() == Some(target) && frontier.next().is_none()
    };
    let truthy_joins_falsy = joins_only_at(truthy, falsy);
    let falsy_joins_truthy = joins_only_at(falsy, truthy);
    match (truthy_joins_falsy, falsy_joins_truthy) {
        (true, false) => Some((truthy, falsy)),
        (false, true) => Some((falsy, truthy)),
        (true, true) | (false, false) => None,
    }
}

pub(super) fn loop_guard_boundary(
    header: super::super::BlockRef,
    truthy: super::super::BlockRef,
    falsy: super::super::BlockRef,
    loops: &[LoopCandidate],
) -> Option<(super::super::BlockRef, super::super::BlockRef)> {
    loops
        .iter()
        .filter(|loop_| {
            loop_.condition_header != Some(header)
                && (loop_.kind_hint == super::super::LoopKindHint::RepeatLike
                    || loop_.header != header)
                && (loop_.blocks.contains(&header) || loop_.body_scope_blocks.contains(&header))
        })
        .filter_map(|loop_| {
            let is_iteration_boundary = |block| {
                block == loop_.header
                    || loop_.continue_target == Some(block)
                    || loop_.control_blocks.contains(&block)
            };
            match (is_iteration_boundary(truthy), is_iteration_boundary(falsy)) {
                (true, false) => Some((
                    (loop_.body_scope_blocks.len(), loop_.blocks.len()),
                    (falsy, truthy),
                )),
                (false, true) => Some((
                    (loop_.body_scope_blocks.len(), loop_.blocks.len()),
                    (truthy, falsy),
                )),
                (true, true) | (false, false) => None,
            }
        })
        .min_by_key(|(score, _)| *score)
        .map(|(_, boundary)| boundary)
}
