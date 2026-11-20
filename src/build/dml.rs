//! `insert`, `update`, `delete`, `merge`.

use sqlparser::ast::*;
use sqlparser::tokenizer::Token;

use super::{Builder, Ctx, R};
use crate::config::ListStyle;
use crate::layout::{Ind, Rule, Sp};

impl Builder<'_> {
    /// A parenthesised column list `(a, b, c)` laid out by `st`; by default
    /// filled to the margin with leading commas under the `(` (`( a, b\n,
    /// c)` once it wraps).
    pub fn column_list(&mut self, cols: &[R], st: ListStyle) {
        let (Some(f), Some(l)) = (cols.first(), cols.last()) else {
            return;
        };
        let (open, close) = (f.0 - 1, l.1 + 1);
        if !self.is(open, &Token::LParen)
            || !self.is(close, &Token::RParen)
            || !self.comma_separated(cols)
        {
            return;
        }
        for c in cols {
            self.src(*c);
        }
        let anchor = self.l.gaps[open].anchor;
        self.set(open, Rule::Keep(Ind::Src(anchor), Sp::One));
        self.paren_list(open, cols, close, st, anchor);
    }

    pub fn insert(&mut self, ins: &Insert, r: R) {
        let insert = r.0;
        let cols: Vec<R> = ins.columns.iter().filter_map(|c| self.rng(c)).collect();
        if cols.len() == ins.columns.len() {
            self.column_list(&cols, self.cfg.insert_columns.style());
        }
        if let Some(src) = &ins.source {
            if let Some(sr) = self.rng(src.as_ref()) {
                self.set(sr.0, Rule::Break(Ind::Col(insert, 0)));
            }
            self.query(src);
        }
        for a in &ins.assignments {
            self.generic(a, false);
        }
    }

    /// `set a = 1, b = 2`: chopped with leading commas when the author
    /// chopped it or it doesn't fit, `=` aligned.
    fn assignments(&mut self, set_tok: usize, assignments: &[Assignment]) {
        let a = self.align_group();
        let mut items = Vec::new();
        for asg in assignments {
            self.expr(&asg.value);
            let (Some(ar), Some(vr)) = (self.rng(asg), self.rng(&asg.value)) else {
                self.generic(asg, false);
                return;
            };
            let eq = vr.0 - 1;
            if eq > ar.0 && self.is(eq, &Token::Eq) {
                self.set(eq, Rule::Flat(Sp::One));
                self.align(eq, a, ar.0);
                self.set(vr.0, Rule::Keep(Ind::Src(ar.0), Sp::One));
            }
            items.push(ar);
        }
        let Some(&first) = items.first() else { return };
        if first.0 == set_tok + 1 {
            self.set(first.0, Rule::Flat(Sp::One));
        }
        if items.len() > 1 && self.comma_separated(&items) {
            // Several assignments: one per line.
            let g = self.group(first.0, items.last().unwrap().1, true);
            self.l.groups[g].force = true;
            self.comma_chop(&items, g, Ind::Col(first.0, -2));
        }
    }

    /// `from …` / `where …` of an update or delete, at `clause`.
    fn clauses_from_where(
        &mut self,
        from: &[TableWithJoins],
        selection: Option<&Expr>,
        clause: Ind,
    ) {
        let mut fr = Vec::new();
        for twj in from {
            if let Some(tr) = self.rng(twj) {
                self.table_with_joins(twj, tr, clause);
                fr.push(tr);
            }
        }
        if let Some(&f0) = fr.first() {
            if f0.0 > 0 && self.is_kw(f0.0 - 1, "from") {
                self.set(f0.0 - 1, Rule::Break(clause));
                self.set(f0.0, Rule::Flat(Sp::One));
            }
            if let Ind::Col(a, _) = clause {
                self.comma_flat(&fr, a);
            }
        }
        if let Some(w) = selection {
            if let Some(wr) = self.rng(w)
                && self.is_kw(wr.0 - 1, "where")
            {
                let wt = wr.0 - 1;
                self.set(wt, Rule::Break(clause));
                self.set(wr.0, Rule::Flat(Sp::One));
                self.condition(w, wt, self.ind_or(self.cfg.where_.indent), Ctx::Where);
                return;
            }
            self.in_ctx(Ctx::Where, |b| b.expr(w));
        }
    }

    fn output(&mut self, o: &OutputClause, clause: Ind) {
        if let Some(or) = self.rng(o) {
            self.set(or.0, Rule::Break(clause));
        }
        self.generic(o, false);
    }

    pub fn update(&mut self, u: &Update, r: R) {
        let clause = Ind::Col(r.0, 0);
        if let Some(tr) = self.rng(&u.table) {
            self.table_with_joins(&u.table, tr, clause);
        }
        if let Some(a0) = u.assignments.first().and_then(|a| self.rng(a)) {
            let set_tok = a0.0 - 1;
            if self.is_kw(set_tok, "set") {
                self.set(set_tok, Rule::Break(clause));
                self.assignments(set_tok, &u.assignments);
            }
        }
        if let Some(o) = &u.output {
            self.output(o, clause);
        }
        let from: &[TableWithJoins] = match &u.from {
            Some(UpdateTableFromKind::AfterSet(f)) => f,
            Some(f @ UpdateTableFromKind::BeforeSet(_)) => {
                self.generic(f, false);
                &[]
            }
            None => &[],
        };
        self.clauses_from_where(from, u.selection.as_ref(), clause);
    }

    pub fn delete(&mut self, d: &Delete, r: R) {
        let clause = Ind::Col(r.0, 0);
        if let Some(o) = &d.output {
            self.output(o, clause);
        }
        let from = match &d.from {
            FromTable::WithFromKeyword(f) | FromTable::WithoutKeyword(f) => f,
        };
        if d.tables.is_empty() {
            // delete [from] t [where]: the table stays on the `delete` line.
            for twj in from {
                if let Some(tr) = self.rng(twj) {
                    self.table_with_joins(twj, tr, clause);
                }
            }
            if let Some(w) = &d.selection {
                self.clauses_from_where(&[], Some(w), clause);
            }
        } else {
            // delete t from … where …
            self.clauses_from_where(from, d.selection.as_ref(), clause);
        }
        if let Some(u) = &d.using {
            self.generic(u, false);
        }
    }

    pub fn merge(&mut self, m: &Merge, r: R) {
        let merge = r.0;
        let clause = Ind::Col(merge, 0);
        self.table_factor(&m.table);
        self.table_factor(&m.source);
        if let Some(sr) = self.rng(&m.source)
            && self.is_kw(sr.0 - 1, "using")
        {
            self.set(sr.0 - 1, Rule::Break(clause));
            self.set(sr.0, Rule::Flat(Sp::One));
        }
        self.in_ctx(Ctx::MergeOn, |b| {
            b.chain = Some(Ctx::MergeOn);
            b.expr(&m.on);
        });
        if let Some(or) = self.rng(m.on.as_ref())
            && self.is_kw(or.0 - 1, "on")
        {
            self.set(or.0 - 1, Rule::Break(clause));
            self.set(or.0, Rule::Flat(Sp::One));
        }
        for c in &m.clauses {
            let Some(when) = self.att(&c.when_token) else {
                self.generic(c, false);
                continue;
            };
            self.set(when, Rule::Break(clause));
            // The action on its own line one indent in, or after `then`.
            let action = if self.cfg.merge_when.action_on_new_line {
                Rule::Break(Ind::Col(merge, self.ind()))
            } else {
                Rule::Flat(Sp::One)
            };
            if let Some(p) = &c.predicate {
                self.in_ctx(Ctx::MergeWhen, |b| {
                    b.chain = Some(Ctx::MergeWhen);
                    b.expr(p);
                });
            }
            match &c.action {
                MergeAction::Update(u) => {
                    let Some(upd) = self.att(&u.update_token) else {
                        continue;
                    };
                    self.set(upd, action);
                    match &u.kind {
                        MergeUpdateKind::Set(assignments) => {
                            if let Some(a0) = assignments.first().and_then(|a| self.rng(a)) {
                                let set_tok = a0.0 - 1;
                                if self.is_kw(set_tok, "set") {
                                    self.set(set_tok, Rule::Break(Ind::Col(merge, self.ind())));
                                    self.assignments(set_tok, assignments);
                                }
                            }
                        }
                        k => self.generic(k, false),
                    }
                }
                MergeAction::Insert(i) => {
                    let Some(ins) = self.att(&i.insert_token) else {
                        continue;
                    };
                    self.set(ins, action);
                    let cols: Vec<R> = i.columns.iter().filter_map(|c| self.rng(c)).collect();
                    if cols.len() == i.columns.len() && !cols.is_empty() {
                        self.column_list(&cols, self.cfg.insert_columns.style());
                        self.set(cols[0].0 - 1, Rule::Flat(Sp::One));
                    }
                    if let MergeInsertKind::Values(v) = &i.kind
                        && let Some(kind) = self.att(&i.kind_token)
                        && self.is_kw(kind, "values")
                    {
                        self.set(kind, Rule::Break(Ind::Col(merge, self.ind())));
                        let mut items = Vec::new();
                        for row in &v.rows {
                            for e in &row.content {
                                self.expr(e);
                                if let Some(er) = self.rng(e) {
                                    items.push(er);
                                }
                            }
                        }
                        if v.rows.len() == 1
                            && items.len() == v.rows[0].content.len()
                            && !items.is_empty()
                        {
                            self.column_list(&items, self.cfg.values.row.style());
                            self.set(items[0].0 - 1, Rule::Flat(Sp::One));
                        }
                        continue;
                    }
                    self.generic(&i.kind, false);
                }
                MergeAction::Delete { delete_token } => {
                    if let Some(d) = self.att(delete_token) {
                        self.set(d, action);
                    }
                }
                a => self.generic(a, false),
            }
        }
        if let Some(o) = &m.output {
            self.output(o, clause);
        }
    }
}
