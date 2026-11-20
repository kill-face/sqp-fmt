//! Expressions: operators, boolean conditions and blocks (STYLE.md change
//! 1), `case`, function calls and window clauses (change 2), subqueries.

use sqlparser::ast::*;
use sqlparser::tokenizer::Token;

use super::{Builder, Ctx, R};
use crate::config::{Wrap, WrapStyle};
use crate::layout::{Ind, Rule, Sp};

fn is_bool(e: &Expr) -> bool {
    matches!(
        e,
        Expr::BinaryOp {
            op: BinaryOperator::And | BinaryOperator::Or,
            ..
        }
    )
}

impl Builder<'_> {
    pub fn expr(&mut self, e: &Expr) {
        // A condition's chain setting applies to this expression only (and
        // to the rest of its `and` / `or` chain).
        let chain = self.chain.take();
        let Some(r) = self.rng(e) else {
            self.generic(e, true);
            return;
        };
        self.src(r);
        match e {
            Expr::BinaryOp { left, op, right } => {
                // An `and` / `or` chain not laid out by its condition
                // (`case when`, `merge … on`, `check`, anything else):
                // `never`, with the author's line breaks kept or not.
                let andor = matches!(op, BinaryOperator::And | BinaryOperator::Or);
                let ctx = chain.unwrap_or(Ctx::Other);
                if andor && is_bool(left) {
                    self.chain = Some(ctx);
                }
                self.expr(left);
                if andor && is_bool(right) {
                    self.chain = Some(ctx);
                }
                self.expr(right);
                let (Some(lr), Some(rr)) = (self.rng(left.as_ref()), self.rng(right.as_ref()))
                else {
                    return;
                };
                let rule = if andor && !self.chain_style(ctx).keep {
                    Rule::Flat(Sp::One)
                } else {
                    Rule::Keep(Ind::Src(r.0), Sp::One)
                };
                for i in lr.1 + 1..=rr.0 {
                    self.set(i, rule);
                }
            }
            Expr::UnaryOp { op, expr, .. } => {
                self.expr(expr);
                if let Some(er) = self.rng(expr.as_ref()) {
                    let sp = match op {
                        UnaryOperator::Not => Sp::One,
                        _ => Sp::No,
                    };
                    if er.0 == r.0 + 1 {
                        self.set(er.0, Rule::Flat(sp));
                    }
                }
            }
            Expr::Nested(p) => self.nested(p, r),
            Expr::Function(f) => self.function(f, r),
            Expr::Case {
                operand,
                conditions,
                else_result,
                end_token,
                ..
            } => self.case(
                r,
                operand.as_deref(),
                conditions,
                else_result.as_deref(),
                end_token,
            ),
            Expr::Cast { expr, .. } => {
                self.expr(expr);
                let Some(er) = self.rng(expr.as_ref()) else {
                    return;
                };
                // cast(expr as type)
                if self.is(er.0 - 1, &Token::LParen) && self.is(r.1, &Token::RParen) {
                    self.set(er.0 - 1, Rule::Flat(Sp::No));
                    self.set(er.0, Rule::Flat(Sp::No));
                    if self.is_kw(er.1 + 1, "as") {
                        self.set(er.1 + 1, Rule::Flat(Sp::One));
                        self.set(er.1 + 2, Rule::Flat(Sp::One));
                        self.data_type(er.1 + 2, r.1 - 1);
                    }
                    self.set(r.1, Rule::Flat(Sp::No));
                }
            }
            Expr::Convert { expr, styles, .. } => {
                self.expr(expr);
                for s in styles {
                    self.expr(s);
                }
                // convert(type, expr[, style])
                let open = r.0 + 1;
                if self.is(open, &Token::LParen) && self.is(r.1, &Token::RParen) {
                    self.set(open, Rule::Flat(Sp::No));
                    self.set(open + 1, Rule::Keep(Ind::Src(r.0), Sp::No));
                    if let Some(er) = self.rng(expr.as_ref())
                        && er.0 > open + 2
                        && self.is(er.0 - 1, &Token::Comma)
                    {
                        self.data_type(open + 1, er.0 - 2);
                        self.set(er.0 - 1, Rule::Keep(Ind::Src(r.0), Sp::No));
                        self.set(er.0, Rule::Keep(Ind::Src(r.0), Sp::One));
                    }
                    self.set(r.1, Rule::Keep(Ind::Src(r.0), Sp::No));
                }
            }
            Expr::Exists { subquery, .. } => {
                // `(` on its own line: where the condition says, else 4 in
                // from `[not] exists`.
                let paren = self.exists_paren.take();
                if let Some(qr) = self.rng(subquery.as_ref()) {
                    let open = qr.0 - 1;
                    if self.is(open, &Token::LParen)
                        && self.is(qr.1 + 1, &Token::RParen)
                        && open > r.0
                    {
                        let mut kw = (r.0..open)
                            .rev()
                            .find(|&i| self.is_kw(i, "exists"))
                            .unwrap_or(r.0);
                        if kw > 0 && self.is_kw(kw - 1, "not") {
                            kw -= 1;
                        }
                        if self.cfg.exists.lparen_on_new_line {
                            self.set(open, Rule::Break(paren.unwrap_or(Ind::Col(kw, self.ind()))));
                            self.subquery_block(open, qr, qr.1 + 1);
                        } else {
                            // `exists (` ending the line, the query one indent in
                            // from that line.
                            self.set(open, Rule::Flat(Sp::One));
                            self.set(qr.0, Rule::Break(Ind::LineOf(open, self.ind())));
                            self.set(qr.1 + 1, Rule::Break(Ind::LineOf(open, 0)));
                        }
                    }
                }
                self.query(subquery);
            }
            Expr::Subquery(p) => {
                if let Some(qr) = self.rng(p.content.as_ref()) {
                    self.paren_subquery(qr, r.0);
                }
                self.query(&p.content);
            }
            Expr::In { expr, target, .. } => {
                self.expr(expr);
                let Some(er) = self.rng(expr.as_ref()) else {
                    return;
                };
                for i in er.1 + 1..r.1 {
                    if self.is(i, &Token::LParen) {
                        break;
                    }
                    self.set(i, Rule::Flat(Sp::One));
                }
                match target {
                    InTarget::List(items) => {
                        let mut rs = Vec::new();
                        for it in items {
                            self.expr(it);
                            if let Some(ir) = self.rng(it) {
                                rs.push(ir);
                            }
                        }
                        if let (Some(f), Some(l)) = (rs.first(), rs.last()) {
                            let (open, close) = (f.0 - 1, l.1 + 1);
                            if self.is(open, &Token::LParen) && self.is(close, &Token::RParen) {
                                self.set(open, Rule::Keep(Ind::Src(r.0), Sp::One));
                                let st = self.cfg.in_list.style();
                                if st == crate::config::KeptList::default().style() {
                                    self.set(f.0, Rule::Flat(Sp::No));
                                    self.set(close, Rule::Keep(Ind::Src(r.0), Sp::No));
                                    self.comma_flat(&rs, r.0);
                                } else {
                                    self.paren_list(open, &rs, close, st, r.0);
                                }
                            }
                        }
                    }
                    InTarget::Subquery(q) => {
                        if let Some(qr) = self.rng(q.as_ref()) {
                            self.paren_subquery(qr, r.0);
                        }
                        self.query(q);
                    }
                    _ => self.generic(target, false),
                }
            }
            Expr::Between {
                expr, low, high, ..
            } => {
                for x in [expr, low, high] {
                    self.expr(x);
                }
                let parts = [
                    self.rng(expr.as_ref()),
                    self.rng(low.as_ref()),
                    self.rng(high.as_ref()),
                ];
                if let [Some(a), Some(b), Some(c)] = parts {
                    for i in (a.1 + 1..=b.0).chain(b.1 + 1..=c.0) {
                        self.set(i, Rule::Keep(Ind::Src(r.0), Sp::One));
                    }
                }
            }
            Expr::IsNull(x, _) | Expr::IsNotNull(x, _) => {
                self.expr(x);
                if let Some(xr) = self.rng(x.as_ref()) {
                    for i in xr.1 + 1..=r.1 {
                        self.set(i, Rule::Flat(Sp::One));
                    }
                }
            }
            Expr::Like {
                expr,
                pattern,
                escape_char,
                ..
            } => {
                self.expr(expr);
                self.expr(pattern);
                if let Some(ec) = escape_char {
                    self.expr(ec);
                }
                if let (Some(a), Some(b)) = (self.rng(expr.as_ref()), self.rng(pattern.as_ref())) {
                    for i in a.1 + 1..=b.0 {
                        self.set(i, Rule::Keep(Ind::Src(r.0), Sp::One));
                    }
                }
            }
            _ => self.generic(e, true),
        }
    }

    /// Spacing inside a data type written from token `a` to `b`:
    /// `decimal(10, 2)`, `nvarchar(max)`.
    pub fn data_type(&mut self, a: usize, b: usize) {
        for i in a + 1..=b.min(self.n() - 1) {
            let rule = match self.tok(i).token {
                Token::LParen | Token::RParen | Token::Comma => Rule::Flat(Sp::No),
                _ if self.is(i - 1, &Token::LParen) => Rule::Flat(Sp::No),
                _ => Rule::Flat(Sp::One),
            };
            self.set(i, rule);
        }
    }

    /// A subquery in parentheses (`exists (…)`, `in (…)`, `(select …)`):
    /// the `(` where the author put it, the query on the next line 4 in from
    /// the `(`, `)` under it.
    fn paren_subquery(&mut self, qr: R, anchor: usize) {
        let (open, close) = (qr.0 - 1, qr.1 + 1);
        if !self.is(open, &Token::LParen) || !self.is(close, &Token::RParen) {
            return;
        }
        if open > anchor {
            self.set(open, Rule::Keep(Ind::Src(anchor), Sp::One));
        }
        self.subquery_block(open, qr, close);
    }

    /// The top-level `and` / `or` chain of a `where` / `having` condition
    /// (`ctx`): by default one predicate per line, `and` at `kw`'s column +
    /// `off`, `or` one further right so the predicates line up.
    pub fn condition(&mut self, e: &Expr, kw: usize, off: isize, ctx: Ctx) {
        let mut operands = Vec::new();
        let mut ops = Vec::new();
        self.flatten_bool(e, &mut operands, &mut ops);
        self.in_ctx(ctx, |b| {
            for (k, x) in operands.iter().enumerate() {
                // `and exists (`: the `(` one indent in from the clause
                // keyword.
                if k > 0 {
                    b.exists_paren = Some(Ind::Col(kw, b.ind()));
                }
                b.expr(x);
                b.exists_paren = None;
            }
        });
        let inds: Vec<(usize, Ind)> = ops
            .iter()
            .map(|&op| {
                let o = if self.is_kw(op, "or") { off + 1 } else { off };
                (op, Ind::Col(kw, o))
            })
            .collect();
        let st = self.chain_style(ctx);
        self.chain_ops(&inds, self.rng(e), st, Rule::Flat(Sp::One));
    }

    /// A join's `on` condition: by default `and` / `or` start lines under
    /// `on` where the author broke the line.
    pub fn condition_keep(&mut self, e: &Expr, on: usize) {
        let mut operands = Vec::new();
        let mut ops = Vec::new();
        self.flatten_bool(e, &mut operands, &mut ops);
        self.in_ctx(Ctx::JoinOn, |b| {
            for x in &operands {
                b.expr(x);
            }
        });
        let inds: Vec<(usize, Ind)> = ops.iter().map(|&op| (op, Ind::Col(on, 0))).collect();
        let st = self.chain_style(Ctx::JoinOn);
        self.chain_ops(&inds, self.rng(e), st, Rule::Keep(Ind::Src(on), Sp::One));
    }

    /// The operators of an `and` / `or` chain spanning `er`, laid out by
    /// `st`: broken, each starts a line at its `Ind`. `after` is the rule
    /// for the gap after an operator (unless everything is on one line).
    fn chain_ops(&mut self, ops: &[(usize, Ind)], er: Option<R>, st: WrapStyle, after: Rule) {
        let Some(er) = er else { return };
        if ops.is_empty() {
            return;
        }
        let g = match st.wrap {
            Wrap::ChopIfLong | Wrap::IfLong => self.group(er.0, er.1, st.keep),
            _ => 0,
        };
        for &(op, ind) in ops {
            let (rule, after) = match st.wrap {
                Wrap::Always => (Rule::Break(ind), after),
                Wrap::ChopIfLong => (Rule::BreakIf(g, ind, Sp::One), after),
                Wrap::IfLong => (Rule::Fill(g, ind, Sp::One), after),
                Wrap::Never if st.keep => (Rule::Keep(ind, Sp::One), after),
                Wrap::Never => (Rule::Flat(Sp::One), Rule::Flat(Sp::One)),
            };
            self.set(op, rule);
            self.set(op + 1, after);
        }
    }

    pub fn flatten_bool_pub<'e>(
        &self,
        e: &'e Expr,
        operands: &mut Vec<&'e Expr>,
        ops: &mut Vec<usize>,
    ) {
        self.flatten_bool(e, operands, ops)
    }

    /// Splits a top-level `and` / `or` chain into operands and the operator
    /// tokens between them. Gives up (one operand) if the spans don't line
    /// up.
    fn flatten_bool<'e>(&self, e: &'e Expr, operands: &mut Vec<&'e Expr>, ops: &mut Vec<usize>) {
        if let Expr::BinaryOp {
            left,
            op: BinaryOperator::And | BinaryOperator::Or,
            right,
        } = e
            && let (Some(lr), Some(rr)) = (self.rng(left.as_ref()), self.rng(right.as_ref()))
        {
            // The operator is the token before the right operand (the
            // left operand's span may stop short, as for `like … escape`).
            if lr.1 + 2 <= rr.0 && (self.is_kw(rr.0 - 1, "and") || self.is_kw(rr.0 - 1, "or")) {
                self.flatten_bool(left, operands, ops);
                ops.push(rr.0 - 1);
                self.flatten_bool(right, operands, ops);
                return;
            }
            if lr.1 + 2 == rr.0 {
                self.flatten_bool(left, operands, ops);
                ops.push(lr.1 + 1);
                self.flatten_bool(right, operands, ops);
                return;
            }
        }
        operands.push(e);
    }

    /// `( … )` around an expression. A boolean group (STYLE.md change 1)
    /// is laid out by its condition's `groups` settings; as a block: `(`
    /// ends its line, contents 2 in from `(`, one operand per line with
    /// `and` / `or` leading, `)` under `(`.
    fn nested(&mut self, p: &Parens<Box<Expr>>, r: R) {
        let inner = p.content.as_ref();
        let Some(ir) = self.rng(inner) else {
            self.generic(inner, true);
            return;
        };
        let (open, close) = (r.0, r.1);
        if !self.is(open, &Token::LParen)
            || !self.is(close, &Token::RParen)
            || ir.0 != open + 1
            || ir.1 + 1 != close
        {
            self.expr(inner);
            return;
        }
        if is_bool(inner) {
            let st = self.groups_style();
            // A group written with a line break after `(` is a block.
            let block = st.wrap == Wrap::Always || (st.keep && self.nl_before(ir.0));
            let g = if block || matches!(st.wrap, Wrap::ChopIfLong | Wrap::IfLong) {
                let g = self.group(open, close, false);
                self.l.groups[g].force = block;
                Some(g)
            } else {
                None
            };
            let mut operands = Vec::new();
            let mut ops = Vec::new();
            self.flatten_bool(inner, &mut operands, &mut ops);
            for x in &operands {
                self.expr(x);
            }
            match g {
                Some(g) if block || st.wrap == Wrap::ChopIfLong => {
                    self.set(ir.0, Rule::BreakIf(g, Ind::Col(open, 2), Sp::No));
                    for op in ops {
                        self.set(op, Rule::BreakIf(g, Ind::Col(open, 2), Sp::One));
                        self.set(op + 1, Rule::Flat(Sp::One));
                    }
                    self.set(close, Rule::BreakIf(g, Ind::Col(open, 0), Sp::No));
                }
                Some(g) => {
                    // Filled: continuation lines under the first operand.
                    self.set(ir.0, Rule::Flat(Sp::No));
                    for op in ops {
                        self.set(op, Rule::Fill(g, Ind::Col(open, 1), Sp::One));
                        self.set(op + 1, Rule::Flat(Sp::One));
                    }
                    self.set(close, Rule::Flat(Sp::No));
                }
                None => {
                    self.set(ir.0, Rule::Flat(Sp::No));
                    for op in ops {
                        self.set(op, Rule::Flat(Sp::One));
                        self.set(op + 1, Rule::Flat(Sp::One));
                    }
                    self.set(close, Rule::Flat(Sp::No));
                }
            }
        } else {
            self.expr(inner);
            self.set(ir.0, Rule::Keep(Ind::Src(open), Sp::No));
            self.set(close, Rule::Keep(Ind::Src(open), Sp::No));
        }
    }

    fn function(&mut self, f: &Function, r: R) {
        let Some(nr) = self.rng(&f.name) else {
            self.generic(f, false);
            return;
        };
        let open = nr.1 + 1;
        if self.is(open, &Token::LParen) {
            self.set(open, Rule::Keep(Ind::Src(r.0), Sp::No));
            if let FunctionArguments::List(list) = &f.args {
                let mut items = Vec::new();
                let mut ok = true;
                for a in &list.args {
                    match a {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => self.expr(e),
                        _ => self.generic(a, false),
                    }
                    match self.rng(a) {
                        Some(ar) => items.push(ar),
                        None => ok = false,
                    }
                }
                for c in &list.clauses {
                    self.generic(c, false);
                }
                if ok && list.clauses.is_empty() && list.duplicate_treatment.is_none() {
                    let st = self.cfg.function_args.style();
                    if st != crate::config::KeptList::default().style() {
                        if let (Some(fi), Some(la)) = (items.first(), items.last())
                            && fi.0 == open + 1
                            && self.is(la.1 + 1, &Token::RParen)
                        {
                            self.paren_list(open, &items, la.1 + 1, st, r.0);
                        }
                    } else {
                        if let (Some(fi), Some(la)) = (items.first(), items.last())
                            && fi.0 == open + 1
                            && self.is(la.1 + 1, &Token::RParen)
                        {
                            self.set(fi.0, Rule::Keep(Ind::Src(r.0), Sp::No));
                            self.set(la.1 + 1, Rule::Keep(Ind::Src(r.0), Sp::No));
                        }
                        self.comma_flat(&items, r.0);
                    }
                } else if self.is(open + 1, &Token::RParen) {
                    self.set(open + 1, Rule::Flat(Sp::No));
                }
            } else if self.is(open + 1, &Token::RParen) {
                self.set(open + 1, Rule::Flat(Sp::No));
            } else {
                self.generic(&f.args, false);
            }
        } else {
            self.generic(&f.args, false);
        }
        for o in &f.within_group {
            self.generic(o, false);
        }
        if let Some(fl) = &f.filter {
            self.expr(fl);
        }
        match f.over.as_deref() {
            Some(WindowType::WindowSpec(spec)) => self.window(spec, r),
            Some(w) => self.generic(w, false),
            None => {}
        }
    }

    /// `over (partition by … order by … frame)` (STYLE.md change 2), laid
    /// out by `window`; as a block: `(` ends the line, each clause on its
    /// own line 2 in from the select item, `)` under the item. A window
    /// written over several lines is a block (`keep_line_breaks`).
    fn window(&mut self, spec: &WindowSpec, fr: R) {
        let (Some(open), Some(close)) = (
            self.att(&spec.opening_paren_token),
            self.att(&spec.closing_paren_token),
        ) else {
            self.generic(spec, false);
            return;
        };
        if !(fr.0 < open && open < close && close <= fr.1) {
            self.generic(spec, false);
            return;
        }
        if self.is_kw(open - 1, "over") {
            self.set(open - 1, Rule::Flat(Sp::One));
            self.set(open, Rule::Flat(Sp::One));
        }
        let ws = self.cfg.window.clone();
        let item = self.item.unwrap_or(fr.0);
        let g = self.group(open, close, false);
        // Written over several lines: a block. (Filled, only a line break
        // right after `(` counts: the fill makes the others.)
        let written = if ws.wrap == Wrap::IfLong {
            self.nl_before(open + 1)
        } else {
            self.multiline((open, close))
        };
        let block = ws.wrap == Wrap::Always || (ws.keep_line_breaks && written);
        if block {
            self.l.groups[g].force = true;
        }
        let mut clauses: Vec<usize> = Vec::new();
        let mut parts: Vec<(Vec<R>, crate::config::ListStyle)> = Vec::new();
        if !spec.partition_by.is_empty() {
            let mut rs = Vec::new();
            for e in &spec.partition_by {
                self.expr(e);
                if let Some(er) = self.rng(e) {
                    rs.push(er);
                }
            }
            if let Some(f) = rs.first()
                && f.0 >= 2
                && self.is_kw(f.0 - 2, "partition")
            {
                clauses.push(f.0 - 2);
                self.set(f.0 - 1, Rule::Flat(Sp::One));
                self.set(f.0, Rule::Flat(Sp::One));
            }
            parts.push((rs, ws.partition_by.style()));
        }
        if !spec.order_by.is_empty() {
            let mut rs = Vec::new();
            for o in &spec.order_by {
                self.generic(o, false);
                if let Some(or) = self.rng(o) {
                    rs.push(or);
                }
            }
            if let Some(f) = rs.first()
                && f.0 >= 2
                && self.is_kw(f.0 - 2, "order")
            {
                clauses.push(f.0 - 2);
                self.set(f.0 - 1, Rule::Flat(Sp::One));
                self.set(f.0, Rule::Flat(Sp::One));
            }
            parts.push((rs, ws.order_by.style()));
        }
        if let Some(wf) = &spec.window_frame {
            self.generic(wf, false);
            let after = parts
                .iter()
                .flat_map(|p| p.0.iter())
                .map(|x| x.1)
                .max()
                .unwrap_or(open);
            if after + 1 < close {
                clauses.push(after + 1);
                if !ws.frame.keep_line_breaks {
                    for i in after + 2..close {
                        if self.rule(i) == Rule::Src {
                            self.set(i, Rule::Flat(Sp::Src));
                        }
                    }
                }
            }
        }
        for (rs, st) in &parts {
            // Line breaks in a list kept, relative to the clause keyword
            // (by default).
            if let Some(f) = rs.first() {
                let kw = f.0.saturating_sub(2);
                self.bare_list(rs, *st, Ind::Col(f.0, -2), kw);
            }
        }
        if clauses.first() != Some(&(open + 1)) {
            // Something we don't know (a window name): keep the source.
            return;
        }
        match ws.wrap {
            _ if block || ws.wrap == Wrap::ChopIfLong => {
                for (k, &c) in clauses.iter().enumerate() {
                    let flat = if k == 0 { Sp::No } else { Sp::One };
                    self.set(c, Rule::BreakIf(g, Ind::Col(item, 2), flat));
                }
                self.set(close, Rule::BreakIf(g, Ind::Col(item, 0), Sp::No));
            }
            Wrap::IfLong => {
                // Filled: a clause that doesn't fit starts a line 2 in from
                // the item; `)` stays at the end.
                for (k, &c) in clauses.iter().enumerate() {
                    if k == 0 {
                        self.set(c, Rule::Flat(Sp::No));
                    } else {
                        self.set(c, Rule::Fill(g, Ind::Col(item, 2), Sp::One));
                    }
                }
                self.set(close, Rule::Flat(Sp::No));
            }
            _ => {
                for (k, &c) in clauses.iter().enumerate() {
                    self.set(c, Rule::Flat(if k == 0 { Sp::No } else { Sp::One }));
                }
                self.set(close, Rule::Flat(Sp::No));
            }
        }
    }

    /// `case [x] when … then … else … end`, laid out by `case`: by default
    /// on one line if the author wrote it so and it fits; else `when` /
    /// `else` one per line one indent in from `case`, `then` aligned, `end`
    /// under `case`.
    fn case(
        &mut self,
        r: R,
        operand: Option<&Expr>,
        conditions: &[CaseWhen],
        else_result: Option<&Expr>,
        end_token: &helpers::attached_token::AttachedToken,
    ) {
        let case = r.0;
        let Some(end) = self.att(end_token) else {
            self.generic_case(operand, conditions, else_result);
            return;
        };
        if !self.is_kw(case, "case") || !self.is_kw(end, "end") {
            self.generic_case(operand, conditions, else_result);
            return;
        }
        let cs = self.cfg.case.clone();
        let g = self.group(case, end, cs.keep_line_breaks);
        let a = self.align_group();
        // A window block inside lines up with its own function, not the
        // select item.
        let saved_item = self.item.take();
        // The gaps before `when`, `else` and `end`.
        let mut branches = Vec::new();
        if let Some(op) = operand {
            self.in_ctx(Ctx::Other, |b| b.expr(op));
            if let Some(or) = self.rng(op) {
                self.set(or.0, Rule::Flat(Sp::One));
            }
        }
        for cw in conditions {
            self.in_ctx(Ctx::CaseWhen, |b| {
                b.chain = Some(Ctx::CaseWhen);
                b.expr(&cw.condition);
            });
            self.in_ctx(Ctx::Other, |b| b.expr(&cw.result));
            let (Some(cr), Some(rr)) = (self.rng(&cw.condition), self.rng(&cw.result)) else {
                continue;
            };
            let (when, then) = (cr.0 - 1, rr.0 - 1);
            if !self.is_kw(when, "when") || !self.is_kw(then, "then") || then != cr.1 + 1 {
                continue;
            }
            self.set(when, Rule::BreakIf(g, Ind::Col(case, self.ind()), Sp::One));
            branches.push(when);
            self.set(cr.0, Rule::Keep(Ind::Src(when), Sp::One));
            if self.nl_before(then) {
                // On its own line as written, while the condition takes more
                // than one line.
                let gc = self.group(cr.0, then, false);
                self.set(then, Rule::BreakIf(gc, Ind::Src(when), Sp::One));
            } else {
                self.set(then, Rule::Flat(Sp::One));
            }
            if cs.align_then {
                self.align(then, a, when);
            }
            self.set(rr.0, Rule::Keep(Ind::Src(when), Sp::One));
        }
        if let Some(e) = else_result {
            self.in_ctx(Ctx::Other, |b| b.expr(e));
            if let Some(er) = self.rng(e) {
                let el = er.0 - 1;
                if self.is_kw(el, "else") {
                    self.set(el, Rule::BreakIf(g, Ind::Col(case, self.ind()), Sp::One));
                    branches.push(el);
                    self.set(er.0, Rule::Keep(Ind::Src(el), Sp::One));
                }
            }
        }
        self.set(end, Rule::BreakIf(g, Ind::Col(case, 0), Sp::One));
        branches.push(end);
        self.item = saved_item;
        match cs.wrap {
            Wrap::Always => self.l.groups[g].force = true,
            Wrap::Never => {
                // On one line, unless the author broke it (and that's kept).
                if cs.keep_line_breaks && branches.iter().any(|&i| self.nl_before(i)) {
                    self.l.groups[g].force = true;
                } else {
                    for i in branches {
                        self.set(i, Rule::Flat(Sp::One));
                    }
                }
            }
            _ => {}
        }
    }

    fn generic_case(
        &mut self,
        operand: Option<&Expr>,
        conditions: &[CaseWhen],
        else_result: Option<&Expr>,
    ) {
        if let Some(o) = operand {
            self.expr(o);
        }
        for c in conditions {
            self.in_ctx(Ctx::CaseWhen, |b| {
                b.chain = Some(Ctx::CaseWhen);
                b.expr(&c.condition);
            });
            self.expr(&c.result);
        }
        if let Some(e) = else_result {
            self.expr(e);
        }
    }
}
