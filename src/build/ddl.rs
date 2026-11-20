//! `create table`, `create index`, `alter table`.

use sqlparser::ast::*;
use sqlparser::tokenizer::Token;

use super::{Builder, Ctx, R};
use crate::config::{Comma, Wrap};
use crate::layout::{Ind, Rule, Sp};

impl Builder<'_> {
    /// ```sql
    /// create table dbo.T
    /// (
    ///     Id   int           not null,
    ///     Name nvarchar(100)
    ///         constraint DF_T_Name default ''   not null,
    ///     constraint PK_T primary key (Id)
    /// );
    /// ```
    /// Column names, types and nullability aligned; named constraints where
    /// the author put them (own line 4 in from the column, or inline).
    pub fn create_table(&mut self, t: &CreateTable, r: R) {
        if !self.column_block(r.0, &t.columns, &t.constraints) {
            self.generic(t, false);
        }
    }

    /// The `( columns, constraints )` of a table, table type or table
    /// variable: `(` and `)` on their own lines at `base`'s column, one
    /// element per line 4 in. Returns false if the spans don't line up.
    pub fn column_block(
        &mut self,
        base: usize,
        columns: &[ColumnDef],
        constraints: &[TableConstraint],
    ) -> bool {
        let types = self.align_group();
        let nulls = self.align_group();
        let mut elems: Vec<R> = Vec::new();
        for c in columns {
            let Some(cr) = self.rng(c) else { return false };
            self.src(cr);
            self.column_def(c, cr, types, nulls);
            elems.push(cr);
        }
        for c in constraints {
            let Some(cr) = self.rng(c) else { return false };
            self.src(cr);
            if matches!(c, TableConstraint::Check(_)) {
                self.check_generic(c);
            } else {
                self.generic(c, false);
            }
            self.references_break(cr);
            elems.push(cr);
        }
        if elems.is_empty() {
            return false;
        }
        elems.sort();
        let (first, last) = (elems[0], *elems.last().unwrap());
        let open = first.0 - 1;
        let mut close = last.1 + 1;
        if self.is(close, &Token::Comma) {
            // Trailing comma.
            self.set(close, Rule::Flat(Sp::No));
            close += 1;
        }
        if !self.is(open, &Token::LParen) || !self.is(close, &Token::RParen) {
            return true;
        }
        self.set(open, Rule::Break(Ind::Col(base, 0)));
        for e in &elems {
            self.set(e.0, Rule::Break(Ind::Col(base, self.ind())));
        }
        let leading = self.cfg.create_table.comma == Comma::Leading;
        for w in elems.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a.1 + 2 == b.0 && self.is(a.1 + 1, &Token::Comma) {
                if leading {
                    // `  , Name …`
                    self.set(a.1 + 1, Rule::Break(Ind::Col(base, self.ind() - 2)));
                    self.set(b.0, Rule::Flat(Sp::One));
                } else {
                    self.set(a.1 + 1, Rule::Flat(Sp::No));
                }
            }
        }
        self.set(close, Rule::Break(Ind::Col(base, 0)));
        true
    }

    fn column_def(&mut self, c: &ColumnDef, cr: R, types: usize, nulls: usize) {
        let name = cr.0;
        // No type (a computed column): the type column holds its `as`.
        let type_end = self.att(&c.data_type_end_token).unwrap_or(name);
        if type_end < name || type_end > cr.1 {
            self.generic(c, false);
            return;
        }
        let ty = name + 1;
        if ty <= cr.1 {
            // The type (or a computed column's `as`) lines up.
            self.set(ty, Rule::Flat(Sp::One));
            if self.cfg.create_table.align_columns {
                self.align(ty, types, name);
            }
        }
        let computed = type_end == name;
        if !computed {
            self.data_type(ty, type_end);
        }
        for o in &c.options {
            let (Some(os), Some(oe)) = (self.att(&o.start_token), self.att(&o.end_token)) else {
                self.generic(o, false);
                continue;
            };
            if os > oe || oe > cr.1 {
                continue;
            }
            self.src((os, oe));
            match &o.option {
                ColumnOption::Default(e) => self.expr(e),
                ColumnOption::Null | ColumnOption::NotNull => {}
                opt @ ColumnOption::Check(_) => self.check_generic(opt),
                opt => self.generic(opt, false),
            }
            let identity = matches!(o.option, ColumnOption::Identity(_));
            for i in os + 1..=oe {
                if self.rule(i) == crate::layout::Rule::Src && !self.nl_before(i) {
                    let sp = if self.is(i, &Token::LParen)
                        && (self.is_kw(i - 1, "default") || self.is_kw(i - 1, "identity"))
                    {
                        Sp::One
                    } else if identity {
                        // `identity (1,1)` as written.
                        Sp::Asis
                    } else {
                        Sp::Src
                    };
                    self.set(i, Rule::Flat(sp));
                }
            }
            if o.name.is_some() && self.cfg.create_table.named_constraint_on_new_line {
                // A named constraint: on its own line one indent in.
                self.set(os, Rule::Break(Ind::Col(name, self.ind())));
            } else if o.name.is_some() {
                self.set(os, Rule::Keep(Ind::Col(name, self.ind()), Sp::One));
            } else {
                self.set(os, Rule::Keep(Ind::Col(name, self.ind()), Sp::One));
                if matches!(o.option, ColumnOption::Null | ColumnOption::NotNull)
                    && !self.nl_before(os)
                    && !computed
                    && self.cfg.create_table.align_columns
                {
                    self.align_any(os, nulls);
                }
            }
        }
    }

    /// A foreign key's `references` on its own line 4 in from the line with
    /// `foreign key`; its `on delete` / `on update` where the author put
    /// them, at the same column.
    fn references_break(&mut self, r: R) {
        if let Some(k) = self.find_kw(r.0 + 1, r.1, "references") {
            let base = self.find_kw(r.0, k, "foreign").unwrap_or(r.0);
            let ind = Ind::LineOf(base, self.ind());
            if self.cfg.foreign_key.references_on_new_line {
                self.set(k, Rule::Break(ind));
            } else {
                self.set(k, Rule::Keep(ind, Sp::One));
            }
            let mut i = k + 1;
            while let Some(on) = self.find_kw(i, r.1, "on") {
                self.set(on, Rule::Keep(ind, Sp::One));
                i = on + 1;
            }
        }
    }

    /// `create index X\n    on T (cols)\n    include (…)\n    where …`.
    pub fn create_index(&mut self, ix: &CreateIndex, r: R) {
        let create = r.0;
        let Some(tr) = self.rng(&ix.table_name) else {
            self.generic(ix, false);
            return;
        };
        if self.is_kw(tr.0 - 1, "on") {
            self.set(tr.0 - 1, Rule::Break(Ind::Col(create, self.ind())));
            self.set(tr.0, Rule::Flat(Sp::One));
        }
        // ( columns ): on the `on` line, or the next line 4 in if it doesn't
        // fit (or the author put it there).
        let open = tr.1 + 1;
        if self.is(open, &Token::LParen)
            && let Some(close) = self.close_paren(open)
        {
            let g = self.group(tr.0, close, true);
            self.set(
                open,
                Rule::BreakIf(g, Ind::LineOf(tr.0, self.ind()), Sp::One),
            );
            self.set(open + 1, Rule::Flat(Sp::No));
            self.set(close, Rule::Flat(Sp::No));
            for i in open + 2..close {
                if self.is(i, &Token::Comma) {
                    self.set(i, Rule::Flat(Sp::No));
                }
            }
            let st = self.cfg.index_columns.style();
            if st != crate::config::KeptList::default().style() {
                let items = self.split_commas(open, close);
                self.paren_list(open, &items, close, st, tr.0);
                if st.wrap == crate::config::Wrap::Always {
                    // Always a block: its `(` stays on the `on` line.
                    self.set(open, Rule::Flat(Sp::One));
                }
            }
            for kw in ["include", "where", "with"] {
                if let Some(k) = self.find_kw(close + 1, r.1, kw) {
                    self.set(k, Rule::Keep(Ind::Col(create, self.ind()), Sp::One));
                }
            }
        }
        if let Some(p) = &ix.predicate {
            self.expr(p);
        }
    }

    /// `alter table X\n    add constraint …\n        references …`: each
    /// operation on its own line 4 in; inside it the source layout.
    pub fn alter_table(&mut self, a: &AlterTable, r: R) {
        let alter = r.0;
        let mut ops = Vec::new();
        for op in &a.operations {
            let Some(or) = self.rng(op) else {
                self.generic(op, false);
                continue;
            };
            self.src(or);
            if matches!(
                op,
                AlterTableOperation::AddConstraint {
                    constraint: TableConstraint::Check(_),
                    ..
                }
            ) {
                self.check_generic(op);
            } else {
                self.generic(op, false);
            }
            self.references_break(or);
            ops.push(or);
        }
        // The first operation starts a line one indent in; the others
        // follow `alter_table`.
        let st = self.cfg.alter_table.clone();
        let ind = Ind::Col(alter, self.ind());
        let g = match (ops.first(), ops.last()) {
            (Some(f), Some(l)) if st.wrap == Wrap::ChopIfLong => {
                Some(self.group(f.0, l.1, st.keep_line_breaks))
            }
            _ => None,
        };
        for (k, or) in ops.iter().enumerate() {
            if k == 0 {
                self.set(or.0, Rule::Break(ind));
            } else if self.is(or.0 - 1, &Token::Comma) {
                self.set(or.0 - 1, Rule::Flat(Sp::No));
                let rule = match (st.wrap, g) {
                    (Wrap::Always, _) => Rule::Break(ind),
                    (_, Some(g)) => Rule::BreakIf(g, ind, Sp::One),
                    _ if st.keep_line_breaks => Rule::Keep(ind, Sp::One),
                    _ => Rule::Flat(Sp::One),
                };
                self.set(or.0, rule);
            }
        }
    }

    /// A `check` constraint (column, table or `alter table … add`): its
    /// condition is laid out by `check_constraint`.
    fn check_generic<V: Visit>(&mut self, node: &V) {
        self.in_ctx(Ctx::Check, |b| {
            b.chain = Some(Ctx::Check);
            b.generic(node, false);
            b.chain = None;
        });
    }
}
