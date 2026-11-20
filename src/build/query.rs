//! Queries: `with`, set operations, `select` and its clauses, `from` and
//! joins, `values`, `order by`.

use sqlparser::ast::*;
use sqlparser::tokenizer::Token;

use super::{Builder, Ctx, R};
use crate::config::{Comma, Wrap};
use crate::layout::{Ind, Rule, Sp};

impl Builder<'_> {
    /// A query: its conditions' contexts are its own.
    pub fn query(&mut self, q: &Query) {
        self.in_ctx(Ctx::Other, |b| b.query_inner(q));
    }

    fn query_inner(&mut self, q: &Query) {
        let Some(r) = self.rng(q) else {
            self.generic(q, true);
            return;
        };
        self.src(r);
        let mut body_ind = Ind::Col(r.0, 0);
        if let Some(w) = &q.with {
            self.with(w, r);
        }
        let Some(br) = self.rng(q.body.as_ref()) else {
            self.generic(q, true);
            return;
        };
        if q.with.is_some() {
            // At the `with` line's indent (`;with` starts a column early).
            self.set(br.0, Rule::Break(Ind::LineOf(r.0, 0)));
            body_ind = Ind::Col(br.0, 0);
        }
        self.set_expr(&q.body, br);
        // Clauses after the body start lines at the body's column.
        let clause_col = Ind::Col(br.0, 0);
        let _ = body_ind;
        if let Some(ob) = &q.order_by {
            if let Some(mut obr) = self.rng(ob) {
                // The span may start at the first expression.
                if !self.is_kw(obr.0, "order") && obr.0 >= 2 && self.is_kw(obr.0 - 2, "order") {
                    obr.0 -= 2;
                }
                self.set(obr.0, Rule::Break(clause_col));
                self.order_by(ob, obr);
            } else {
                self.generic(ob, false);
            }
        }
        let mut rest_from = self.range(ob_span(q)).map_or(br.1 + 1, |x| x.1 + 1);
        if q.order_by.is_none() {
            rest_from = br.1 + 1;
        }
        // `option (...)`, `for xml ...`, `offset … fetch`: own lines.
        for kw in ["option", "for", "offset"] {
            if let Some(i) = self.find_kw(rest_from, r.1, kw) {
                self.set(i, Rule::Break(clause_col));
            }
        }
        if let Some(lc) = &q.limit_clause {
            self.generic(lc, false);
        }
    }

    /// `with A as (…), B as (…)`: each CTE's `(` on its own line one indent
    /// in, the query inside one more, `)` under `(`; later CTEs start
    /// `   , B as` so their names line up with the first.
    fn with(&mut self, w: &With, qr: R) {
        let Some(with_tok) = self.att(&w.with_token) else {
            return;
        };
        let mut prev_close: Option<usize> = None;
        let mut first_name: Option<usize> = None;
        for cte in &w.cte_tables {
            let Some(close) = self.att(&cte.closing_paren_token) else {
                return;
            };
            let Some(open) = self.open_paren(close) else {
                return;
            };
            let Some(name) = self.at(cte.alias.name.span.start) else {
                return;
            };
            let first_name = *first_name.get_or_insert(name);
            if let Some(pc) = prev_close
                && pc + 1 < name
                && self.is(pc + 1, &Token::Comma)
                && pc + 2 == name
            {
                if self.cfg.cte_list.comma == Comma::Leading {
                    // `   , B as`
                    self.set(pc + 1, Rule::Break(Ind::LineOf(with_tok, 3)));
                    self.set(name, Rule::Flat(Sp::One));
                } else {
                    // `),` and `B as` under the first name.
                    self.set(pc + 1, Rule::Flat(Sp::No));
                    self.set(name, Rule::Break(Ind::Col(first_name, 0)));
                }
            }
            // `as (` : `(` on its own line, 4 in from `with` when there are
            // several CTEs, 4 in from the CTE's name when there's one.
            let paren = if w.cte_tables.len() == 1 {
                Ind::Col(name, self.ind())
            } else {
                Ind::LineOf(with_tok, self.ind())
            };
            let own_line = self.cfg.cte.lparen_on_new_line;
            if own_line {
                self.set(open, Rule::Break(paren));
            } else if self.is_kw(open - 1, "as") {
                self.set(open, Rule::Flat(Sp::One));
            }
            if let Some(cr) = self.rng(cte.query.as_ref())
                && cr.0 == open + 1
                && cr.1 + 1 == close
            {
                if own_line {
                    self.set(cr.0, Rule::Break(Ind::Col(open, self.ind())));
                    self.set(close, Rule::Break(Ind::Col(open, 0)));
                } else {
                    // `as (` ends the line; the query one indent in from it.
                    self.set(cr.0, Rule::Break(Ind::LineOf(open, self.ind())));
                    self.set(close, Rule::Break(Ind::LineOf(open, 0)));
                }
            }
            self.query(&cte.query);
            prev_close = Some(close);
        }
        let _ = qr;
    }

    pub fn set_expr(&mut self, body: &SetExpr, r: R) {
        match body {
            SetExpr::Select(s) => self.select(s, r),
            SetExpr::Query(q) => {
                // ( query )
                self.paren_query(q, r);
            }
            SetExpr::SetOperation { left, right, .. } => {
                let (Some(lr), Some(rr)) = (self.rng(left.as_ref()), self.rng(right.as_ref()))
                else {
                    self.generic(body, false);
                    return;
                };
                self.set_expr(left, lr);
                self.set_expr(right, rr);
                // `union [all]` on its own line, the right side on the next.
                if lr.1 + 1 < rr.0 {
                    self.set(lr.1 + 1, Rule::Break(Ind::Col(r.0, 0)));
                    for i in lr.1 + 2..rr.0 {
                        self.set(i, Rule::Flat(Sp::One));
                    }
                    self.set(rr.0, Rule::Break(Ind::Col(r.0, 0)));
                }
            }
            SetExpr::Values(v) => self.values(v, r),
            _ => self.generic(body, false),
        }
    }

    /// A parenthesised query used as a set operand.
    fn paren_query(&mut self, q: &Query, r: R) {
        let Some(qr) = self.rng(q) else {
            self.generic(q, true);
            return;
        };
        if qr.0 > r.0 && self.is(r.0, &Token::LParen) && self.is(r.1, &Token::RParen) {
            self.subquery_block(r.0, qr, r.1);
        }
        self.query(q);
    }

    /// `( query )` laid out as a block: the query on the next line 4 in from
    /// the `(`, `)` under the `(`.
    pub fn subquery_block(&mut self, open: usize, qr: R, close: usize) {
        self.set(qr.0, Rule::Break(Ind::Col(open, self.ind())));
        self.set(close, Rule::Break(Ind::Col(open, 0)));
    }

    fn values(&mut self, v: &Values, r: R) {
        // values (…)
        //      , (…)
        let mut rows: Vec<R> = Vec::new();
        for row in &v.rows {
            let mut items = Vec::new();
            for e in &row.content {
                self.expr(e);
                if let Some(er) = self.rng(e) {
                    items.push(er);
                }
            }
            if items.is_empty() || items.len() != row.content.len() {
                return;
            }
            let (open, close) = (items[0].0 - 1, items.last().unwrap().1 + 1);
            if !self.is(open, &Token::LParen) || !self.is(close, &Token::RParen) {
                return;
            }
            // Like a column list: `( a, b\n, c)` once it wraps.
            self.column_list(&items, self.cfg.values.row.style());
            rows.push((open, close));
        }
        if rows.is_empty() || !self.comma_separated(&rows) {
            return;
        }
        // `values` then the first row on the same line (or, after a
        // comment, on its own line where rows go).
        let mut comma = Ind::Col(rows[0].0, -2);
        if rows[0].0 > r.0 {
            if self.l.trivia[rows[0].0].forces_break() {
                self.set(rows[0].0, Rule::Break(Ind::Col(r.0, self.ind())));
                comma = Ind::Col(r.0, 5);
            } else {
                self.set(rows[0].0, Rule::Flat(Sp::One));
            }
        }
        if rows.len() > 1 {
            // Several rows: one per line (by default).
            let st = self.cfg.values.style();
            self.bare_list(&rows, st, comma, r.0);
        }
    }

    fn select(&mut self, s: &Select, r: R) {
        let sel = r.0;
        let clause = Ind::Col(sel, 0);
        // Projection.
        let mut items: Vec<R> = Vec::new();
        let a = self.align_group();
        let saved_item = self.item;
        for it in &s.projection {
            let Some(ir) = self.rng(it) else {
                self.generic(it, false);
                items.clear();
                continue;
            };
            self.src(ir);
            self.item = Some(ir.0);
            self.select_item(it, ir, a);
            items.push(ir);
        }
        self.item = saved_item;
        if let Some(&first) = items.first() {
            if first.0 > sel {
                // `select [distinct] [top n] first`
                for i in sel + 1..=first.0 {
                    if self.rule(i) == Rule::Src {
                        self.set(i, Rule::Flat(Sp::Src));
                    }
                }
                self.set(first.0, Rule::Flat(Sp::One));
                if let Some(t) = &s.top {
                    self.generic(t, false);
                }
            }
            // A comment before the first item puts it on a line of its own,
            // 4 in from `select`; leading commas stay where they'd be with
            // the item on the `select` line.
            let st = self.cfg.select_list.style();
            let comma = if self.l.trivia[first.0].forces_break() && first.0 > sel {
                self.set(first.0, Rule::Break(Ind::Col(sel, self.ind())));
                if st.comma == Comma::Leading {
                    Ind::Col(sel, 5)
                } else {
                    Ind::Col(sel, self.ind() - 2)
                }
            } else {
                Ind::Col(first.0, -2)
            };
            if items.len() == s.projection.len() {
                self.bare_list(&items, st, comma, sel);
            }
        }
        let mut last = items.last().map_or(sel, |x| x.1);
        // into
        if let Some(into) = &s.into
            && let Some(ir) = self.rng(into)
        {
            // The span leaves out `into`.
            let k = (sel + 1..ir.0)
                .rev()
                .find(|&i| self.is_kw(i, "into"))
                .unwrap_or(ir.0);
            self.set(k, Rule::Break(clause));
            for i in k + 1..=ir.0 {
                self.set(i, Rule::Flat(Sp::One));
            }
            last = ir.1;
        }
        // from
        if !s.from.is_empty() {
            let mut fr: Vec<R> = Vec::new();
            for twj in &s.from {
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
                self.comma_flat(&fr, sel);
                last = fr.last().unwrap().1;
            }
        }
        // where
        if let Some(w) = &s.selection
            && let Some(wr) = self.rng(w)
        {
            if self.is_kw(wr.0 - 1, "where") {
                let wt = wr.0 - 1;
                self.set(wt, Rule::Break(clause));
                self.set(wr.0, Rule::Flat(Sp::One));
                self.condition(w, wt, 2, Ctx::Where);
            } else {
                self.in_ctx(Ctx::Where, |b| b.expr(w));
            }
            last = wr.1;
        }
        // group by
        if let GroupByExpr::Expressions(exprs, _) = &s.group_by {
            let mut gr = Vec::new();
            for e in exprs {
                self.expr(e);
                if let Some(er) = self.rng(e) {
                    gr.push(er);
                }
            }
            if let Some(&g0) = gr.first() {
                if g0.0 >= 2 && self.is_kw(g0.0 - 1, "by") && self.is_kw(g0.0 - 2, "group") {
                    self.set(g0.0 - 2, Rule::Break(clause));
                    self.set(g0.0 - 1, Rule::Flat(Sp::One));
                    self.set(g0.0, Rule::Flat(Sp::One));
                }
                let st = self.cfg.group_by.style();
                self.bare_list(&gr, st, Ind::Col(g0.0, -2), g0.0);
                last = gr.last().unwrap().1;
            }
        }
        // having
        if let Some(h) = &s.having
            && let Some(hr) = self.rng(h)
        {
            if self.is_kw(hr.0 - 1, "having") {
                let ht = hr.0 - 1;
                self.set(ht, Rule::Break(clause));
                self.set(hr.0, Rule::Flat(Sp::One));
                self.condition(h, ht, 2, Ctx::Having);
            } else {
                self.in_ctx(Ctx::Having, |b| b.expr(h));
            }
            last = hr.1;
        }
        let _ = last;
    }

    fn select_item(&mut self, it: &SelectItem, ir: R, a: usize) {
        match it {
            SelectItem::UnnamedExpr(e) => self.expr(e),
            SelectItem::ExprWithAlias { expr, alias } => {
                self.expr(expr);
                let (Some(er), Some(al)) = (self.rng(expr), self.at(alias.span.start)) else {
                    return;
                };
                if al > er.1 {
                    // expr [as] alias: aliases aligned when the list is chopped.
                    // (Found from the alias: the expression's span may stop
                    // short, as for `within group (…)`.)
                    let k = if al - 1 > er.1 && self.is_kw(al - 1, "as") {
                        al - 1
                    } else {
                        al
                    };
                    if k < al {
                        self.set(al, Rule::Flat(Sp::One));
                    }
                    if self.nl_before(k) {
                        self.set(k, Rule::Keep(Ind::Src(ir.0), Sp::One));
                    } else {
                        self.set(k, Rule::Flat(Sp::One));
                        if self.cfg.select_list.align_aliases
                            && matches!(self.cfg.select_list.wrap, Wrap::ChopIfLong | Wrap::Always)
                        {
                            self.align_any(k, a);
                        }
                    }
                } else if al < er.0 {
                    // alias = expr
                    let eq = al + 1;
                    if eq + 1 == er.0 && self.is(eq, &Token::Eq) {
                        self.set(eq, Rule::Flat(Sp::One));
                        self.set(er.0, Rule::Keep(Ind::Src(ir.0), Sp::One));
                    }
                }
            }
            _ => self.generic(it, false),
        }
    }

    /// A `from` item: the table, then each join on its own line at `clause`,
    /// its `on` on the next line 4 in.
    pub fn table_with_joins(&mut self, twj: &TableWithJoins, tr: R, clause: Ind) {
        self.src(tr);
        self.table_factor(&twj.relation);
        for j in &twj.joins {
            let Some(jr) = self.rng(j) else {
                self.generic(j, false);
                continue;
            };
            self.src(jr);
            self.set(jr.0, Rule::Break(clause));
            let Some(rr) = self.rng(&j.relation) else {
                self.generic(j, false);
                continue;
            };
            for i in jr.0 + 1..rr.0 {
                self.set(i, Rule::Flat(Sp::One));
            }
            // A derived table's `(` goes on its own line under the join (or
            // ends the join's line).
            let derived = matches!(j.relation, TableFactor::Derived { .. });
            let own_line = self.cfg.derived_table.lparen_on_new_line;
            if derived && own_line {
                self.set(rr.0, Rule::Break(Ind::Col(jr.0, 0)));
            } else {
                self.set(rr.0, Rule::Flat(Sp::One));
            }
            self.table_factor(&j.relation);
            if derived
                && !own_line
                && let TableFactor::Derived { subquery, .. } = &j.relation
                && let Some(qr) = self.rng(subquery.as_ref())
                && self.is(qr.0 - 1, &Token::LParen)
                && self.is(qr.1 + 1, &Token::RParen)
            {
                let open = qr.0 - 1;
                self.set(qr.0, Rule::Break(Ind::LineOf(open, self.ind())));
                self.set(qr.1 + 1, Rule::Break(Ind::LineOf(open, 0)));
            }
            let constraint = match &j.join_operator {
                JoinOperator::Join(c)
                | JoinOperator::Inner(c)
                | JoinOperator::Left(c)
                | JoinOperator::LeftOuter(c)
                | JoinOperator::Right(c)
                | JoinOperator::RightOuter(c)
                | JoinOperator::FullOuter(c) => Some(c),
                _ => None,
            };
            if let Some(JoinConstraint::On(e)) = constraint {
                if let Some(er) = self.rng(e)
                    && self.is_kw(er.0 - 1, "on")
                {
                    let on = er.0 - 1;
                    self.set(on, Rule::Break(Ind::Col(jr.0, self.ind())));
                    self.set(er.0, Rule::Flat(Sp::One));
                    self.condition_keep(e, on);
                    continue;
                }
                self.in_ctx(Ctx::JoinOn, |b| b.expr(e));
            }
        }
    }

    pub fn table_factor(&mut self, tf: &TableFactor) {
        let Some(tr) = self.rng(tf) else {
            self.generic(tf, false);
            return;
        };
        self.src(tr);
        match tf {
            TableFactor::Derived {
                subquery, alias, ..
            } => {
                if let Some(qr) = self.rng(subquery.as_ref()) {
                    let (open, close) = (qr.0 - 1, qr.1 + 1);
                    if self.is(open, &Token::LParen) && self.is(close, &Token::RParen) {
                        self.subquery_block(open, qr, close);
                    }
                }
                self.query(subquery);
                // `as x (a, b, c)`
                if let Some(a) = alias {
                    let cols: Vec<R> = a
                        .columns
                        .iter()
                        .filter_map(|c| self.at(c.name.span.start).map(|i| (i, i)))
                        .collect();
                    if !cols.is_empty()
                        && cols.len() == a.columns.len()
                        && a.columns.iter().all(|c| c.data_type.is_none())
                    {
                        self.column_list(&cols, crate::config::FilledList::default().style());
                    }
                }
            }
            _ => self.generic(tf, false),
        }
    }

    pub fn order_by(&mut self, ob: &OrderBy, obr: R) {
        if let OrderByKind::Expressions(exprs) = &ob.kind {
            let mut items = Vec::new();
            for e in exprs {
                self.generic(e, false);
                if let Some(er) = self.rng(e) {
                    items.push(er);
                }
            }
            if let Some(&f) = items.first() {
                if f.0 >= obr.0 + 2 {
                    self.set(obr.0 + 1, Rule::Flat(Sp::One));
                    self.set(f.0, Rule::Flat(Sp::One));
                }
                let st = self.cfg.order_by.style();
                self.bare_list(&items, st, Ind::Col(f.0, -2), f.0);
            }
        } else {
            self.generic(ob, false);
        }
    }
}

fn ob_span(q: &Query) -> sqlparser::tokenizer::Span {
    q.order_by
        .as_ref()
        .map_or(sqlparser::tokenizer::Span::empty(), |o| o.span())
}
