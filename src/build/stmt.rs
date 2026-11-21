//! Statements: dispatch, blocks (`begin … end`, `begin try`, `if`, `while`),
//! `declare`, `set`, and routine bodies.
//!
//! The rules for the statements every dialect shares (queries, DML, DDL,
//! routine parameter lists, `set`, `declare`) are the same in each. The
//! procedural rules (STYLE.md R6, R7, the bodies of R22 and R23) are T-SQL's:
//! its `begin … end` blocks, `if` / `while` without `then`, `as begin …
//! end` routine bodies and `create trigger … on … after … as`. Other
//! dialects' procedural code (PL/pgSQL, MySQL compound statements,
//! Snowflake and BigQuery scripting, SQLite triggers) shares some of those
//! AST nodes but not their syntax (`if … then … end if`, `begin …
//! exception … end`, `after update on t`), so it keeps its source layout,
//! with the SQL statements inside it formatted.

use sqlparser::ast::*;
use sqlparser::tokenizer::Token;

use super::{Builder, Ctx, R};
use crate::config::Comma;
use crate::layout::{Ind, Rule, Sp};

impl Builder<'_> {
    /// A statement: its conditions' contexts are its own.
    pub fn statement(&mut self, s: &Statement) {
        self.in_ctx(Ctx::Other, |b| b.statement_inner(s));
    }

    /// Whether the batch is T-SQL (see the module docs).
    fn tsql(&self) -> bool {
        self.cfg.dialect_name() == "mssql"
    }

    fn statement_inner(&mut self, s: &Statement) {
        let Some(r) = self.rng(s) else {
            self.generic(s, true);
            return;
        };
        self.src(r);
        let tsql = self.tsql();
        match s {
            Statement::Query(q) => self.query(q),
            Statement::Insert(i) => self.insert(i, r),
            Statement::Update(u) => self.update(u, r),
            Statement::Delete(d) => self.delete(d, r),
            Statement::Merge(m) => self.merge(m, r),
            Statement::If(i) if tsql && i.if_block.then_token.is_none() => self.if_stmt(i, r),
            Statement::While(w) if tsql && w.while_block.then_token.is_none() => {
                self.while_stmt(w, r)
            }
            Statement::StartTransaction {
                statements,
                has_end_keyword: true,
                end_token,
                ..
            } if tsql => {
                let end = self.att(end_token);
                self.block(r.0, statements, end, r);
            }
            Statement::CreateProcedure { params, body, .. } => {
                self.create_procedure(s, params.as_deref(), body, r)
            }
            Statement::CreateFunction(f) => self.create_function(f, r),
            Statement::CreateView(v) => self.create_view(v, r),
            Statement::CreateTrigger(t) if tsql => self.create_trigger(t, r),
            Statement::CreateTable(t) => self.create_table(t, r),
            Statement::CreateIndex(i) => self.create_index(i, r),
            Statement::AlterTable(a) => self.alter_table(a, r),
            Statement::Declare { stmts, .. } => self.declare(stmts, r),
            Statement::CreateType {
                representation:
                    Some(UserDefinedTypeRepresentation::Table {
                        columns,
                        constraints,
                    }),
                ..
            } => {
                if !self.column_block(r.0, columns, constraints) {
                    self.generic(s, true);
                }
            }
            Statement::Set(set) => self.set_stmt(set, r),
            Statement::Execute {
                name: Some(name),
                parameters,
                has_parentheses: false,
                ..
            } if !parameters.is_empty() => {
                self.generic(s, true);
                self.exec_args(name, parameters.len(), r);
            }
            _ => self.generic(s, true),
        }
    }

    /// `begin [try|catch] … end [try|catch]` starting at token `begin`: the
    /// statements one indent in, `end` under `begin`.
    pub fn block(&mut self, begin: usize, stmts: &[Statement], end: Option<usize>, r: R) {
        self.statements(stmts, Ind::Col(begin, self.ind()));
        let end = end.or_else(|| self.find_end(begin, stmts, r));
        if let Some(e) = end
            && e > begin
            && e <= r.1
        {
            self.set(e, Rule::Break(Ind::Col(begin, 0)));
        }
    }

    /// The `end` of a block when the AST doesn't say: the first `end` after
    /// the last statement.
    fn find_end(&self, begin: usize, stmts: &[Statement], r: R) -> Option<usize> {
        let from = stmts
            .last()
            .and_then(|s| self.rng(s))
            .map_or(begin + 1, |x| x.1 + 1);
        (from..=r.1).find(|&i| self.is_kw(i, "end"))
    }

    /// The body of an `if` / `while` / `else`, after the token `head`.
    fn cond_body(&mut self, head: usize, body: &ConditionalStatements) {
        match body {
            ConditionalStatements::BeginEnd(bes) => {
                let Some(begin) = self.att(&bes.begin_token) else {
                    return;
                };
                self.set(begin, Rule::Break(Ind::Col(head, self.ind())));
                let end = self.att(&bes.end_token);
                let r = (begin, end.unwrap_or(begin));
                self.src(r);
                self.block(begin, &bes.statements, end, r);
            }
            ConditionalStatements::Sequence { statements } => {
                self.statements(statements, Ind::Col(head, self.ind()));
            }
        }
    }

    /// An `if` / `while` condition: an `exists (` in it (at its top level,
    /// or in an `and` / `or` chain) puts the `(` 4 in from `if`.
    fn cond_exists(&mut self, c: &Expr, head: usize, ctx: Ctx) {
        let mut operands = Vec::new();
        let mut ops = Vec::new();
        self.flatten_bool_pub(c, &mut operands, &mut ops);
        self.in_ctx(ctx, |b| {
            for x in operands {
                b.exists_paren = Some(Ind::Col(head, b.ind()));
                b.expr(x);
                b.exists_paren = None;
            }
        });
        // The chain keeps its source layout (spaced operators), or is put on
        // one line.
        let keep = self.chain_style(ctx).keep;
        for op in ops {
            let anchor = self.l.gaps[op].anchor;
            let rule = if keep {
                Rule::Keep(Ind::Src(anchor), Sp::One)
            } else {
                Rule::Flat(Sp::One)
            };
            self.set(op, rule);
            self.set(op + 1, rule);
        }
    }

    fn if_stmt(&mut self, s: &IfStatement, _r: R) {
        let anchor = self.if_anchor.take();
        let Some(own_if) = self.att(&s.if_block.start_token) else {
            return;
        };
        // In `else if`, the body and the next `else` line up with the `else`.
        let if_tok = anchor.unwrap_or(own_if);
        if let Some(c) = &s.if_block.condition
            && let Some(cr) = self.rng(c)
        {
            self.set(cr.0, Rule::Flat(Sp::One));
            self.cond_exists(c, if_tok, Ctx::If);
        }
        self.cond_body(if_tok, &s.if_block.conditional_statements);
        for b in &s.elseif_blocks {
            self.generic(b, false);
        }
        if let Some(e) = &s.else_block {
            let Some(else_tok) = self.att(&e.start_token) else {
                return;
            };
            self.set(else_tok, Rule::Break(Ind::Col(if_tok, 0)));
            // `else if` on one line stays that way.
            if let ConditionalStatements::Sequence { statements } = &e.conditional_statements
                && let [inner @ Statement::If(_)] = statements.as_slice()
                && let Some(ir) = self.rng(inner)
                && !self.nl_before(ir.0)
                && ir.0 == else_tok + 1
            {
                self.if_anchor = Some(if_tok);
                self.statement(inner);
                self.if_anchor = None;
                self.set(ir.0, Rule::Flat(Sp::One));
                return;
            }
            self.cond_body(else_tok, &e.conditional_statements);
        }
    }

    fn while_stmt(&mut self, s: &WhileStatement, _r: R) {
        let b = &s.while_block;
        let Some(w) = self.att(&b.start_token) else {
            return;
        };
        if let Some(c) = &b.condition
            && let Some(cr) = self.rng(c)
        {
            self.set(cr.0, Rule::Flat(Sp::One));
            self.cond_exists(c, w, Ctx::While);
        }
        self.cond_body(w, &b.conditional_statements);
    }

    fn declare(&mut self, stmts: &[Declare], r: R) {
        for d in stmts {
            if let Some(
                DeclareAssignment::Expr(e)
                | DeclareAssignment::Default(e)
                | DeclareAssignment::MsSqlAssignment(e),
            ) = &d.assignment
            {
                self.expr(e);
                // `= value`
                if let Some(er) = self.rng(e.as_ref()) {
                    let eq = er.0 - 1;
                    if eq > r.0 && self.is(eq, &Token::Eq) {
                        self.set(eq, Rule::Flat(Sp::One));
                        let anchor = r.0;
                        self.set(er.0, Rule::Keep(Ind::Src(anchor), Sp::One));
                    }
                }
            } else if let Some(DataType::Table(Some(cols))) = &d.data_type {
                // declare @t table (…)
                if !self.column_block(r.0, cols, &[]) {
                    self.generic(d, false);
                }
            } else {
                self.generic(d, false);
            }
            if let Some(q) = &d.for_query {
                self.query(q);
            }
        }
    }

    fn set_stmt(&mut self, s: &Set, r: R) {
        match s {
            Set::SingleAssignment { values, .. } => {
                for v in values {
                    self.expr(v);
                    if let Some(vr) = self.rng(v) {
                        let eq = vr.0 - 1;
                        if eq > r.0 && self.is(eq, &Token::Eq) {
                            self.set(eq, Rule::Flat(Sp::One));
                            self.set(vr.0, Rule::Keep(Ind::Src(r.0), Sp::One));
                        }
                    }
                }
            }
            _ => self.generic(s, false),
        }
    }

    /// `create procedure name (params) as body`.
    fn create_procedure(
        &mut self,
        s: &Statement,
        params: Option<&[ProcedureParam]>,
        body: &CreateProcedureBody,
        r: R,
    ) {
        let create = r.0;
        if let Some(params) = params {
            self.routine_params(
                create,
                params
                    .iter()
                    .map(|p| (p.name.span, &p.data_type, &p.end_token, p.default.as_ref())),
                r,
            );
        }
        match body {
            CreateProcedureBody::As(cs) if self.tsql() => self.routine_body(create, cs, r),
            _ => self.generic(s, true),
        }
    }

    /// Parameter lists of procedures and functions. In parentheses: `(` on
    /// its own line, one parameter per line indented 2, leading commas at
    /// the `create` column, types aligned, `)` on its own line. Without
    /// parentheses: one per line indented 2.
    pub fn routine_params<'p>(
        &mut self,
        create: usize,
        params: impl Iterator<
            Item = (
                sqlparser::tokenizer::Span,
                &'p DataType,
                &'p helpers::attached_token::AttachedToken,
                Option<&'p Expr>,
            ),
        >,
        r: R,
    ) {
        let mut items = Vec::new();
        let a = self.align_group();
        for (name_span, _dt, end, default) in params {
            let Some(name) = self.at(name_span.start) else {
                return;
            };
            let Some(end) = self.att(end) else { return };
            if end < name {
                return;
            }
            items.push((name, end));
            // Type right after the name, aligned.
            if self.cfg.routine_params.align_types {
                self.align(name + 1, a, name);
            }
            self.set(name + 1, Rule::Flat(Sp::One));
            if let Some(d) = default {
                self.expr(d);
                if let Some(dr) = self.rng(d)
                    && self.is(dr.0 - 1, &Token::Eq)
                {
                    self.set(dr.0 - 1, Rule::Flat(Sp::One));
                    self.set(dr.0, Rule::Flat(Sp::One));
                }
            }
        }
        let Some(&first) = items.first() else { return };
        let last = items.last().unwrap().1;
        if !self.comma_separated(&items) || last > r.1 {
            return;
        }
        let open = first.0 - 1;
        if self.is(open, &Token::LParen) && self.is(last + 1, &Token::RParen) {
            self.set(open, Rule::Break(Ind::Col(create, 0)));
            self.param_lines(create, &items);
            self.set(last + 1, Rule::Break(Ind::Col(create, 0)));
        } else {
            self.param_lines(create, &items);
        }
    }

    /// One parameter per line `routine_params.indent` in from `create`;
    /// leading commas 2 left of the parameters (so the names line up, as in
    /// a select list), or trailing commas.
    fn param_lines(&mut self, create: usize, items: &[R]) {
        let Some(first) = items.first() else { return };
        let configured = self.list_comma_side(items, self.cfg.routine_params.comma);
        // `, @name`: a leading comma and its space need 2 columns (a comment
        // can make a comma lead even when trailing commas are configured).
        let pi = self.ind_or(self.cfg.routine_params.indent).max(2);
        self.set(first.0, Rule::Break(Ind::Col(create, pi)));
        for w in items.windows(2) {
            if self.comma_side(w[0].1 + 1, w[1].0, configured) == Comma::Leading {
                self.set(w[0].1 + 1, Rule::Break(Ind::Col(create, pi - 2)));
                self.set(w[1].0, Rule::Flat(Sp::One));
            } else {
                self.set(w[0].1 + 1, Rule::Flat(Sp::No));
                self.set(w[1].0, Rule::Break(Ind::Col(create, pi)));
            }
        }
    }

    /// `as` + body of a procedure / trigger: `as` where the author put it
    /// (end of the header line or its own line), `begin` at the `create`
    /// column.
    fn routine_body(&mut self, create: usize, cs: &ConditionalStatements, r: R) {
        match cs {
            ConditionalStatements::BeginEnd(bes) => {
                let Some(begin) = self.att(&bes.begin_token) else {
                    return;
                };
                let end = self.att(&bes.end_token);
                if begin > 0 && self.is_kw(begin - 1, "as") && self.rule(begin - 1) == Rule::Src {
                    let as_tok = begin - 1;
                    self.set(as_tok, Rule::Keep(Ind::Col(create, 0), Sp::One));
                }
                self.set(begin, Rule::Break(Ind::Col(create, 0)));
                self.src((begin, end.unwrap_or(r.1)));
                self.block(begin, &bes.statements, end, (begin, end.unwrap_or(r.1)));
            }
            ConditionalStatements::Sequence { statements } => {
                // `as` then statements without begin/end.
                self.statements(statements, Ind::Col(create, 0));
            }
        }
    }

    fn create_function(&mut self, f: &CreateFunction, r: R) {
        let create = r.0;
        if let Some(args) = &f.args {
            let named: Vec<_> = args
                .iter()
                .filter_map(|a| {
                    a.name
                        .as_ref()
                        .map(|n| (n.span, &a.data_type, &a.end_token, a.default_expr.as_ref()))
                })
                .collect();
            if named.len() == args.len() {
                self.routine_params(create, named.into_iter(), r);
            }
        }
        // `returns …` on its own line.
        if f.return_type.is_some() {
            let from = f
                .args
                .as_ref()
                .and_then(|a| a.last())
                .and_then(|a| self.att(&a.end_token))
                .unwrap_or(create);
            if let Some(k) = self.find_kw(from + 1, r.1, "returns") {
                self.set(k, Rule::Break(Ind::Col(create, 0)));
            }
        }
        match &f.function_body {
            Some(CreateFunctionBody::AsBeginEnd(bes)) if self.tsql() => {
                let Some(begin) = self.att(&bes.begin_token) else {
                    return;
                };
                let end = self.att(&bes.end_token);
                self.set(begin, Rule::Break(Ind::Col(create, 0)));
                self.block(begin, &bes.statements, end, (begin, end.unwrap_or(r.1)));
                if let Some(FunctionReturnType::DataType(_)) = &f.return_type {
                    // returns ... as: keep
                }
            }
            Some(body) => self.generic(body, false),
            None => {}
        }
        if let Some(rt) = &f.return_type {
            self.generic(rt, false);
        }
    }

    /// `exec name a, b, c`: the arguments laid out by `exec_args` (by
    /// default as written).
    fn exec_args(&mut self, name: &ObjectName, n: usize, r: R) {
        let st = self.cfg.exec_args.style();
        if st == crate::config::KeptList::default().style() {
            return;
        }
        let Some(nr) = self.rng(name) else { return };
        let end = if self.is(r.1, &Token::SemiColon) {
            r.1 - 1
        } else {
            r.1
        };
        let items = self.split_commas(nr.1, end + 1);
        if items.len() == n && items[0].0 == nr.1 + 1 {
            self.bare_list(&items, st, Ind::Col(items[0].0, -2), r.0);
        }
    }

    fn create_view(&mut self, v: &CreateView, r: R) {
        // `(a, b, c)` after the name.
        let st = self.cfg.view_columns.style();
        if !v.columns.is_empty()
            && st != crate::config::KeptList::default().style()
            && let Some(nr) = self.rng(&v.name)
            && self.is(nr.1 + 1, &Token::LParen)
            && let Some(close) = self.close_paren(nr.1 + 1)
        {
            let open = nr.1 + 1;
            let items = self.split_commas(open, close);
            if items.len() == v.columns.len() {
                self.paren_list(open, &items, close, st, r.0);
            }
        }
        // The query one indent in (on a new line).
        if let Some(qr) = self.rng(v.query.as_ref()) {
            self.set(qr.0, Rule::Break(Ind::Col(r.0, self.ind())));
        }
        self.query(&v.query);
    }

    /// `create trigger X\n    on T\n    after insert, update\n    as\nbegin`.
    fn create_trigger(&mut self, t: &CreateTrigger, r: R) {
        let create = r.0;
        if let Some(tr) = self.rng(&t.table_name)
            && self.is_kw(tr.0 - 1, "on")
        {
            self.set(tr.0 - 1, Rule::Break(Ind::Col(create, self.ind())));
            let k = tr.1 + 1;
            if ["after", "for", "instead"].iter().any(|w| self.is_kw(k, w)) {
                self.set(k, Rule::Break(Ind::Col(create, self.ind())));
            }
        }
        if let Some(ConditionalStatements::BeginEnd(bes)) = &t.statements
            && let Some(begin) = self.att(&bes.begin_token)
            && self.is_kw(begin - 1, "as")
        {
            self.set(begin - 1, Rule::Break(Ind::Col(create, self.ind())));
        }
        match &t.statements {
            Some(cs) => self.routine_body(create, cs, r),
            None => self.generic(t, false),
        }
    }
}
