//! Recover a whole raw literal, not independently "safe" object overwrites.
//!
//! Every first physical write must replace entry nil or a raw-proved primitive.
//! Subsequent writes may replace
//! transaction-owned tables only because all producers disappear into the same literal.
//! Replaying Lua 5.1's pending-list register discipline must reproduce *every* write,
//! record and SETLIST in order, including scratch roots left behind by nested records.
//! Freshness alone would not suffice: a surviving child local can keep a later weak
//! reference alive after its owner is cleared.
//! Allocation hints are also part of the trace: a full SETLIST can precede trailing
//! record fields, and moving a hash allocation across that boundary changes nil capacity.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::DecompileDialect;
use crate::hir::common::{
    HirBlock, HirExpr, HirLValue, HirRecordField, HirStmt, HirTableConstructor, HirTableField,
    HirTableKey, TempId,
};
use crate::hir::promotion::{HomeSlotKey, RawLiteralInstruction};
use crate::hir::simplify::visit::{HirVisitor, visit_stmts};
use crate::transformer::Lua51TableAllocation;

use super::bindings::table_key_from_expr;
use super::scan::install_constructor_seed;
use super::{MAX_GENERIC_SET_LIST_SCAN_STMTS, TableBinding, TableConstructorPass};

const FIELDS_PER_FLUSH: usize = 50;
const MAX_DEPTH: usize = 64;

#[derive(Clone, PartialEq)]
enum Value {
    Producer(TempId),
    Constant(HirExpr),
}

#[derive(Clone, PartialEq, Eq)]
enum RecordKey {
    Name(String),
    Integer(i64),
}

impl RecordKey {
    fn from_expr(expr: &HirExpr) -> Option<Self> {
        let key = table_key_from_expr(expr, DecompileDialect::Lua51);
        if let HirTableKey::Name(name) = key {
            return Some(Self::Name(name));
        }
        let integer = super::builder::statically_known_numeric_key(&key).flatten()?;
        (-(1_i64 << 53)..=(1_i64 << 53))
            .contains(&integer)
            .then_some(Self::Integer(integer))
    }

    fn in_list(&self, length: usize) -> bool {
        matches!(self, Self::Integer(index)
            if usize::try_from(*index).is_ok_and(|index| index > 0 && index <= length))
    }

    fn to_hir(&self) -> HirTableKey {
        match self {
            Self::Name(name) => HirTableKey::Name(name.clone()),
            Self::Integer(index) => HirTableKey::Expr(HirExpr::Integer(*index)),
        }
    }
}

#[derive(Clone, PartialEq)]
enum Event {
    Write(TempId),
    NewTable(TempId, Lua51TableAllocation),
    Record(TempId, RecordKey, Value),
    List(TempId, u32, Vec<TempId>),
}

struct Field {
    order: usize,
    key: Option<RecordKey>,
    value: Value,
}

enum Contents {
    Scalar(HirExpr),
    Table {
        fields: Vec<Field>,
        list_count: usize,
    },
}

struct Producer {
    home: HomeSlotKey,
    order: usize,
    contents: Contents,
}

#[derive(Default)]
struct Transaction {
    producers: BTreeMap<TempId, Producer>,
    consumed: BTreeSet<TempId>,
    homes: BTreeSet<HomeSlotKey>,
    events: Vec<Event>,
}

pub(super) fn rebuild_single_pass_literals(
    pass: &TableConstructorPass<'_>,
    block: &mut HirBlock,
) -> bool {
    if pass.dialect != DecompileDialect::Lua51 || pass.promotion_facts.compacts_home_slots() {
        return false;
    }
    let mut global_uses = LastUses::default();
    visit_stmts(&block.stmts, &mut global_uses);
    rewrite_once_only_blocks(block, &mut |block| {
        rebuild_literal_transactions(pass, block, &global_uses.counts)
    })
}

pub(super) fn rebuild_root_literals(
    pass: &TableConstructorPass<'_>,
    block: &mut HirBlock,
) -> bool {
    if !pass.is_single_pass_root {
        return false;
    }
    let mut global_uses = LastUses::default();
    visit_stmts(&block.stmts, &mut global_uses);
    rebuild_literal_transactions(pass, block, &global_uses.counts)
}

fn rewrite_once_only_blocks(
    block: &mut HirBlock,
    rewrite: &mut impl FnMut(&mut HirBlock) -> bool,
) -> bool {
    let mut changed = rewrite(block);
    for stmt in &mut block.stmts {
        match stmt {
            HirStmt::If(branch) => {
                changed |= rewrite_once_only_blocks(&mut branch.then_block, rewrite);
                if let Some(block) = &mut branch.else_block {
                    changed |= rewrite_once_only_blocks(block, rewrite);
                }
            }
            HirStmt::Block(block) => {
                changed |= rewrite_once_only_blocks(block, rewrite);
            }
            // In particular, never enter any kind of loop or its nested conditionals.
            _ => {}
        }
    }
    changed
}

fn rebuild_literal_transactions(
    pass: &TableConstructorPass<'_>,
    block: &mut HirBlock,
    global_uses: &BTreeMap<TempId, usize>,
) -> bool {
    if pass.dialect != DecompileDialect::Lua51
        || pass.promotion_facts.compacts_home_slots()
        || block.stmts.len() > MAX_GENERIC_SET_LIST_SCAN_STMTS
        || !block
            .stmts
            .iter()
            .any(|stmt| super::scan::constructor_seed(stmt).is_some())
    {
        return false;
    }
    let mut uses = LastUses::default();
    for (index, stmt) in block.stmts.iter().enumerate() {
        uses.index = index;
        visit_stmts(std::slice::from_ref(stmt), &mut uses);
    }
    uses.protect_external_uses(global_uses);
    // Keep original coordinates for suffix-use checks while committing disjoint regions.
    let mut removed = 0;
    let mut index = 0;
    let mut changed = false;
    while index < block.stmts.len() {
        if let Some((constructor, end)) =
            literal_transaction(pass, &block.stmts, index, removed, &uses)
        {
            install_constructor_seed(&mut block.stmts[index], constructor);
            block.stmts.drain(index + 1..=end);
            removed += end - index;
            changed = true;
        }
        index += 1;
    }
    changed
}

#[derive(Default)]
struct LastUses {
    index: usize,
    last: BTreeMap<TempId, usize>,
    last_list: BTreeMap<TempId, usize>,
    counts: BTreeMap<TempId, usize>,
    external: BTreeSet<TempId>,
}

impl LastUses {
    fn protect_external_uses(&mut self, global: &BTreeMap<TempId, usize>) {
        self.external = self
            .counts
            .iter()
            .filter_map(|(temp, count)| (global.get(temp) != Some(count)).then_some(*temp))
            .collect();
    }
}

impl HirVisitor for LastUses {
    fn visit_stmt(&mut self, stmt: &HirStmt) {
        if let HirStmt::TableSetList(batch) = stmt
            && let HirExpr::TempRef(temp) = batch.base
        {
            self.last_list.insert(temp, self.index);
        }
    }

    fn visit_expr(&mut self, expr: &HirExpr) {
        if let HirExpr::TempRef(temp) = expr {
            self.last.insert(*temp, self.index);
            *self.counts.entry(*temp).or_default() += 1;
        }
    }
}

fn literal_transaction(
    pass: &TableConstructorPass<'_>,
    stmts: &[HirStmt],
    start: usize,
    removed: usize,
    uses: &LastUses,
) -> Option<(HirTableConstructor, usize)> {
    let (TableBinding::Temp(root), seed) = super::scan::constructor_seed(&stmts[start])? else {
        return None;
    };
    if !seed.fields.is_empty() || seed.trailing_multivalue.is_some() {
        return None;
    }
    let allocation = pass.promotion_facts.lua51_table_allocation(root)?;
    let mut transaction = Transaction::default();
    let mut best = None;
    let mut best_event_count = None;
    for (index, stmt) in stmts.iter().enumerate().skip(start) {
        let Some(root_write) = extend_transaction(&mut transaction, pass, root, stmt) else {
            break;
        };
        if root_write
            && uses
                .last_list
                .get(&root)
                .is_none_or(|last| *last <= index + removed)
            && transaction.allocation(root) == Some(allocation)
        {
            let Some(constructor) = complete_transaction(
                &mut transaction,
                pass,
                root,
                index + removed,
                uses,
                best_event_count,
            ) else {
                continue;
            };
            // Floating-byte hints are rounded: 17 and 18 records have the same
            // allocation. Keep the latest proved prefix so the last child does not
            // become an independent local root. A failed extension leaves it intact.
            best = Some((constructor, index));
            best_event_count = Some(transaction.events.len());
        }
    }
    best
}

fn extend_transaction(
    transaction: &mut Transaction,
    pass: &TableConstructorPass<'_>,
    root: TempId,
    stmt: &HirStmt,
) -> Option<bool> {
    let mut root_write = false;
    match stmt {
        HirStmt::Assign(assign)
            if assign.values.tail.is_none()
                && assign.targets.len() == assign.values.fixed.len()
                && assign
                    .targets
                    .iter()
                    .all(|target| matches!(target, HirLValue::Temp(_))) =>
        {
            for (target, value) in assign.targets.iter().zip(&assign.values.fixed) {
                let HirLValue::Temp(temp) = target else {
                    return None;
                };
                let binding = TableBinding::Temp(*temp);
                let facts = pass.promotion_facts;
                let home = facts.trusted_temp_home_slot(*temp)?;
                if pass.materialized_bindings.get(binding) != Some(&1)
                    || facts.is_repeat_condition_prefix_temp(*temp)
                    || pass.reference_captured_home_slots.contains(&home)
                    || pass.reference_captured_bindings.get(binding) == Some(&true)
                    || *temp != root && pass.debug_identity_bindings.get(binding) == Some(&true)
                    || facts
                        .trusted_immediate_move_write_homes(*temp)
                        .is_none_or(|homes| !homes.is_empty())
                    || transaction.producers.contains_key(temp)
                    || transaction.homes.insert(home)
                        && !facts.overwrites_entry_nil(*temp)
                        && !facts.overwrites_raw_primitive(*temp)
                {
                    return None;
                }
                let contents = match value {
                    HirExpr::TableConstructor(table)
                        if table.fields.is_empty()
                            && table.trailing_multivalue.is_none()
                            && facts.is_direct_table_seed_temp(*temp) =>
                    {
                        Contents::Table {
                            fields: Vec::new(),
                            list_count: 0,
                        }
                    }
                    value if scalar(value) => Contents::Scalar(value.clone()),
                    _ => return None,
                };
                let event = if matches!(contents, Contents::Table { .. }) {
                    Event::NewTable(*temp, facts.lua51_table_allocation(*temp)?)
                } else {
                    Event::Write(*temp)
                };
                transaction.producers.insert(
                    *temp,
                    Producer {
                        home,
                        order: transaction.events.len(),
                        contents,
                    },
                );
                transaction.events.push(event);
            }
        }
        HirStmt::Assign(assign) => {
            let [HirLValue::TableAccess(access)] = assign.targets.as_slice() else {
                return None;
            };
            let [value] = assign.values.fixed.as_slice() else {
                return None;
            };
            let HirExpr::TempRef(owner) = access.base else {
                return None;
            };
            let key = RecordKey::from_expr(&access.key)?;
            if assign.values.tail.is_some() {
                return None;
            }
            let (value, order) = match value {
                HirExpr::TempRef(temp) => {
                    let producer = transaction.producers.get(temp)?;
                    if !matches!(producer.contents, Contents::Table { .. })
                        || !transaction.consumed.insert(*temp)
                        || *temp == root
                    {
                        return None;
                    }
                    (Value::Producer(*temp), producer.order)
                }
                value if scalar(value) => {
                    (Value::Constant(value.clone()), transaction.events.len())
                }
                _ => return None,
            };
            let Contents::Table { list_count, .. } = transaction.producers.get(&owner)?.contents
            else {
                return None;
            };
            if key.in_list(list_count) {
                return None;
            }
            let fields = transaction.fields(owner)?;
            if fields.iter().any(|field| field.key.as_ref() == Some(&key)) {
                return None;
            }
            fields.push(Field {
                order,
                key: Some(key.clone()),
                value: value.clone(),
            });
            transaction.events.push(Event::Record(owner, key, value));
            root_write = owner == root;
        }
        HirStmt::TableSetList(batch) => {
            let HirExpr::TempRef(owner) = batch.base else {
                return None;
            };
            let Contents::Table { list_count, .. } = transaction.producers.get(&owner)?.contents
            else {
                return None;
            };
            if list_count % FIELDS_PER_FLUSH != 0
                || usize::try_from(batch.start_index).ok()? != list_count + 1
                || batch.values.tail.is_some()
                || batch.values.fixed.is_empty()
                || batch.values.fixed.len() > FIELDS_PER_FLUSH
            {
                return None;
            }
            if transaction.fields(owner)?.iter().any(|field| {
                field
                    .key
                    .as_ref()
                    .is_some_and(|key| key.in_list(list_count + batch.values.fixed.len()))
            }) {
                return None;
            }
            let mut values = Vec::new();
            for value in &batch.values.fixed {
                let HirExpr::TempRef(temp) = value else {
                    return None;
                };
                let producer = transaction.producers.get(temp)?;
                let order = producer.order;
                if !transaction.consumed.insert(*temp) || *temp == root {
                    return None;
                }
                transaction.fields(owner)?.push(Field {
                    order,
                    key: None,
                    value: Value::Producer(*temp),
                });
                values.push(*temp);
            }
            let Contents::Table { list_count, .. } =
                &mut transaction.producers.get_mut(&owner)?.contents
            else {
                return None;
            };
            *list_count += values.len();
            transaction
                .events
                .push(Event::List(owner, batch.start_index, values));
            root_write = owner == root;
        }
        _ => return None,
    }
    Some(root_write)
}

fn complete_transaction(
    transaction: &mut Transaction,
    pass: &TableConstructorPass<'_>,
    root: TempId,
    end: usize,
    uses: &LastUses,
    previous_event_count: Option<usize>,
) -> Option<HirTableConstructor> {
    if transaction.consumed.len() + 1 != transaction.producers.len()
        || transaction.producers.keys().any(|temp| {
            *temp != root
                && (uses.external.contains(temp)
                    || uses.last.get(temp).is_some_and(|last| *last > end))
        })
    {
        return None;
    }
    for producer in transaction.producers.values_mut() {
        if let Contents::Table { fields, .. } = &mut producer.contents {
            fields.sort_by_key(|field| field.order);
        }
    }
    let home = transaction.producers.get(&root)?.home;
    let mut replay = Vec::new();
    let value = transaction.replay(root, home, 0, 0, &mut replay)?;
    let physical_trace = replay
        .iter()
        .map(|event| match event {
            Event::Write(temp) => RawLiteralInstruction::Write(*temp),
            Event::NewTable(temp, allocation) => {
                RawLiteralInstruction::NewTable(*temp, *allocation)
            }
            Event::Record(..) => RawLiteralInstruction::Record,
            Event::List(..) => RawLiteralInstruction::SetList,
        })
        .collect::<Vec<_>>();
    if replay != transaction.events
        || !pass
            .promotion_facts
            .raw_literal_trace_matches(&physical_trace)
    {
        return None;
    }
    if let Some(start) = previous_event_count {
        let mut added_roots = BTreeSet::new();
        for event in &transaction.events[start..] {
            match event {
                Event::NewTable(temp, _) => {
                    added_roots.insert(transaction.producers.get(temp)?.home);
                }
                Event::Write(temp) => {
                    added_roots.remove(&transaction.producers.get(temp)?.home);
                }
                Event::Record(..) | Event::List(..) => {}
            }
        }
        if !pass.promotion_facts.raw_literal_successor_overwrites(
            root,
            physical_trace.len(),
            &added_roots,
        ) {
            return None;
        }
    }
    let HirExpr::TableConstructor(constructor) = value else {
        return None;
    };
    Some(*constructor)
}

impl Transaction {
    fn allocation(&self, temp: TempId) -> Option<Lua51TableAllocation> {
        let Contents::Table { fields, .. } = &self.producers.get(&temp)?.contents else {
            return None;
        };
        let records = fields.iter().filter(|field| field.key.is_some()).count();
        Some(Lua51TableAllocation::from_field_counts(
            fields.len() - records,
            records,
        ))
    }

    fn fields(&mut self, owner: TempId) -> Option<&mut Vec<Field>> {
        if self.consumed.contains(&owner) {
            return None;
        }
        match &mut self.producers.get_mut(&owner)?.contents {
            Contents::Table { fields, .. } => Some(fields),
            Contents::Scalar(_) => None,
        }
    }

    fn replay(
        &self,
        temp: TempId,
        root_home: HomeSlotKey,
        offset: usize,
        depth: usize,
        events: &mut Vec<Event>,
    ) -> Option<HirExpr> {
        let producer = self.producers.get(&temp)?;
        if depth > MAX_DEPTH || producer.home.offset_from(root_home) != Some(offset) {
            return None;
        }
        let Contents::Table { fields, .. } = &producer.contents else {
            let Contents::Scalar(value) = &producer.contents else {
                return None;
            };
            events.push(Event::Write(temp));
            return Some(value.clone());
        };
        events.push(Event::NewTable(temp, self.allocation(temp)?));
        let mut constructor = HirTableConstructor::default();
        let mut pending = Vec::new();
        let mut start_index = 1;
        for field in fields {
            let value = match &field.value {
                Value::Producer(child) => self.replay(
                    *child,
                    root_home,
                    offset + pending.len() + 1,
                    depth + 1,
                    events,
                )?,
                Value::Constant(value) => value.clone(),
            };
            if let Some(key) = &field.key {
                constructor
                    .fields
                    .push(HirTableField::Record(HirRecordField {
                        key: key.to_hir(),
                        value,
                    }));
                events.push(Event::Record(temp, key.clone(), field.value.clone()));
            } else {
                let Value::Producer(child) = field.value else {
                    return None;
                };
                constructor.fields.push(HirTableField::Array(value));
                pending.push(child);
                if pending.len() == FIELDS_PER_FLUSH {
                    events.push(Event::List(temp, start_index, std::mem::take(&mut pending)));
                    start_index += FIELDS_PER_FLUSH as u32;
                }
            }
        }
        if !pending.is_empty() {
            events.push(Event::List(temp, start_index, pending));
        }
        Some(HirExpr::TableConstructor(Box::new(constructor)))
    }
}

fn scalar(value: &HirExpr) -> bool {
    matches!(
        value,
        HirExpr::Nil
            | HirExpr::Boolean(_)
            | HirExpr::Integer(_)
            | HirExpr::Number(_)
            | HirExpr::String(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::common::{HirGenericFor, HirIf, HirNumericFor, HirRepeat, HirWhile, LocalId};

    #[test]
    fn once_only_walk_excludes_all_loop_bodies_and_their_conditionals() {
        let branch = HirStmt::If(Box::new(HirIf {
            cond: HirExpr::Boolean(true),
            then_block: HirBlock {
                stmts: vec![HirStmt::Block(Box::default())],
            },
            else_block: Some(HirBlock::default()),
        }));
        let loop_body = HirBlock {
            stmts: vec![branch.clone()],
        };
        let mut root = HirBlock {
            stmts: vec![
                branch,
                HirStmt::Block(Box::default()),
                HirStmt::While(Box::new(HirWhile {
                    cond: HirExpr::Boolean(true),
                    body: loop_body.clone(),
                })),
                HirStmt::Repeat(Box::new(HirRepeat {
                    cond: HirExpr::Boolean(false),
                    body: loop_body.clone(),
                })),
                HirStmt::NumericFor(Box::new(HirNumericFor {
                    binding: LocalId(0),
                    start: HirExpr::Integer(1),
                    limit: HirExpr::Integer(2),
                    step: HirExpr::Integer(1),
                    body: loop_body.clone(),
                })),
                HirStmt::GenericFor(Box::new(HirGenericFor {
                    bindings: vec![LocalId(1)],
                    iterator: Vec::new().into(),
                    body: loop_body,
                })),
            ],
        };
        let mut visited = 0;
        assert!(!rewrite_once_only_blocks(&mut root, &mut |_| {
            visited += 1;
            false
        }));
        assert_eq!(visited, 5);
    }

    #[test]
    fn integer_record_keys_normalize_aliases_and_reject_unknown_keys() {
        assert!(RecordKey::from_expr(&HirExpr::Number(1.0)) == Some(RecordKey::Integer(1)));
        assert!(RecordKey::from_expr(&HirExpr::Integer(1)) == Some(RecordKey::Integer(1)));
        assert!(RecordKey::from_expr(&HirExpr::Number(f64::NAN)).is_none());
        assert!(RecordKey::from_expr(&HirExpr::Number(1.5)).is_none());
        assert!(RecordKey::from_expr(&HirExpr::Integer(1_i64 << 54)).is_none());
        assert!(RecordKey::from_expr(&HirExpr::TempRef(TempId(1))).is_none());
        assert!(RecordKey::Integer(1).in_list(3));
        assert!(!RecordKey::Integer(0).in_list(3));
        assert!(!RecordKey::Integer(-1).in_list(3));
        assert!(!RecordKey::Integer(4).in_list(3));
    }

    #[test]
    fn descendant_producers_cannot_ignore_uses_outside_their_block() {
        let mut local = LastUses {
            counts: BTreeMap::from([(TempId(1), 1), (TempId(2), 2)]),
            ..LastUses::default()
        };
        local.protect_external_uses(&BTreeMap::from([(TempId(1), 2), (TempId(2), 2)]));
        assert!(local.external.contains(&TempId(1)));
        assert!(!local.external.contains(&TempId(2)));
    }

    fn table(home: usize, fields: Vec<(Option<&str>, usize)>) -> Producer {
        Producer {
            home: HomeSlotKey::new(home, 0),
            order: 0,
            contents: Contents::Table {
                fields: fields
                    .into_iter()
                    .map(|(name, temp)| Field {
                        order: 0,
                        key: name.map(|name| RecordKey::Name(name.to_owned())),
                        value: Value::Producer(TempId(temp)),
                    })
                    .collect(),
                list_count: 0,
            },
        }
    }

    fn mixed_row() -> Transaction {
        Transaction {
            producers: BTreeMap::from([
                (TempId(0), table(0, vec![(None, 1), (Some("tag"), 2)])),
                (
                    TempId(1),
                    Producer {
                        home: HomeSlotKey::new(1, 0),
                        order: 1,
                        contents: Contents::Scalar(HirExpr::Integer(1)),
                    },
                ),
                (TempId(2), table(2, Vec::new())),
            ]),
            ..Transaction::default()
        }
    }

    #[test]
    fn pending_array_values_reserve_record_scratch_homes() {
        let transaction = mixed_row();
        let mut events = Vec::new();
        assert!(
            transaction
                .replay(TempId(0), HomeSlotKey::new(0, 0), 0, 0, &mut events)
                .is_some()
        );
        assert!(
            events
                == vec![
                    Event::NewTable(TempId(0), Lua51TableAllocation::from_field_counts(1, 1)),
                    Event::Write(TempId(1)),
                    Event::NewTable(TempId(2), Lua51TableAllocation::from_field_counts(0, 0)),
                    Event::Record(
                        TempId(0),
                        RecordKey::Name("tag".to_owned()),
                        Value::Producer(TempId(2))
                    ),
                    Event::List(TempId(0), 1, vec![TempId(1)]),
                ]
        );
    }

    #[test]
    fn record_reordering_cannot_hide_a_different_scratch_write() {
        let mut transaction = mixed_row();
        transaction.fields(TempId(0)).unwrap().swap(0, 1);
        assert!(
            transaction
                .replay(TempId(0), HomeSlotKey::new(0, 0), 0, 0, &mut Vec::new(),)
                .is_none()
        );
    }

    #[test]
    fn unknown_or_different_epoch_homes_do_not_replay() {
        let transaction = mixed_row();
        assert!(
            transaction
                .replay(TempId(0), HomeSlotKey::new(0, 1), 0, 0, &mut Vec::new(),)
                .is_none()
        );
    }
}
