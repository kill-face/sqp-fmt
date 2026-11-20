//! The configuration file, `sqp-fmt.toml`.
//!
//! Every key is optional; a missing key (or a missing file) means today's
//! house style (STYLE.md). The vocabulary follows JetBrains' code-style
//! settings (`style/jetbrains-codestyle.xml`); `docs/config.md` is the
//! reference, with the JetBrains counterpart of every key.
//!
//! Unknown keys and invalid values are errors. So is a value whose layout
//! isn't built yet: `validate` holds the table of what's implemented, and
//! rejects the rest with "not implemented yet for <key>", so no option is
//! ever silently ignored.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sqlparser::dialect::Dialect;

/// The name of the configuration file.
pub const FILE_NAME: &str = "sqp-fmt.toml";

/// How a construct wraps when it has more than one element (JetBrains'
/// "Wrapping" values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wrap {
    /// "Do not wrap": on one line, however long.
    Never,
    /// "Wrap if long": filled, a line break only where the next element
    /// doesn't fit.
    IfLong,
    /// "Chop down if long": on one line if it fits, else every element on a
    /// line of its own.
    ChopIfLong,
    /// "Wrap always" / "Chop always": every element on a line of its own.
    Always,
}

impl Wrap {
    pub fn name(self) -> &'static str {
        match self {
            Wrap::Never => "never",
            Wrap::IfLong => "if_long",
            Wrap::ChopIfLong => "chop_if_long",
            Wrap::Always => "always",
        }
    }
}

/// Where the comma goes when a list is broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comma {
    /// At the start of the line, before the element (`, b`).
    Leading,
    /// At the end of the line, after the element (`a,`).
    Trailing,
}

impl Comma {
    pub fn name(self) -> &'static str {
        match self {
            Comma::Leading => "leading",
            Comma::Trailing => "trailing",
        }
    }
}

/// Letter case for a class of words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WordCase {
    Lower,
    Upper,
    /// As written.
    AsIs,
}

/// Defines a section: a struct whose missing keys take the section's
/// defaults, with unknown keys rejected.
macro_rules! section {
    (
        $(#[$m:meta])*
        $name:ident {
            $( $(#[$fm:meta])* $field:ident : $ty:ty = $def:expr ),* $(,)?
        }
    ) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Deserialize)]
        #[serde(deny_unknown_fields, default)]
        pub struct $name {
            $( $(#[$fm])* pub $field: $ty, )*
        }

        impl Default for $name {
            fn default() -> Self {
                Self { $( $field: $def, )* }
            }
        }
    };
}

section! {
    /// The whole file.
    Config {
        /// The sqlparser dialect used to parse and, for the safety net, to
        /// tokenize: `mssql` (the default) or any other dialect the crate
        /// has. The layout rules are written for T-SQL; with another dialect
        /// whatever doesn't parse, or wouldn't come back with the same
        /// tokens, is left as written.
        dialect: String = "mssql".to_string(),
        /// Right margin (columns).
        margin: usize = 120,
        /// One indent level (spaces): statement blocks, clause continuation
        /// lines (`on` under a join, `create index … on`), subquery and CTE
        /// contents, `case` branches, table columns, `merge` actions.
        indent: usize = 4,
        /// Keywords (`select`, `from`, `nocount`, date parts).
        keyword_case: WordCase = WordCase::Lower,
        /// Built-in functions (`count`, `getdate`, `cast`, `@@rowcount`).
        function_case: WordCase = WordCase::Lower,
        /// System data types (`int`, `nvarchar`, `sysname`).
        type_case: WordCase = WordCase::Lower,

        /// The `select` list (R8).
        select_list: SelectList = SelectList::default(),
        /// The `group by` list (R11).
        group_by: KeptList = KeptList::default(),
        /// The `order by` list of a query (R11).
        order_by: KeptList = KeptList::default(),

        /// The `where` condition (R10).
        #[serde(rename = "where")]
        where_: ClauseCondition = ClauseCondition::default(),
        /// The `having` condition (R10).
        having: ClauseCondition = ClauseCondition::default(),
        /// The `on` condition of a join (R9).
        join_on: Condition = Condition::default(),
        /// The condition of a `case` expression's `when`.
        case_when: Condition = Condition::default(),
        /// The condition of an `if`.
        if_condition: Condition = Condition::default(),
        /// The condition of a `while`.
        while_condition: Condition = Condition::default(),
        /// The `on` condition of a `merge` (R19).
        merge_on: Condition = Condition::default(),
        /// The `and …` condition of a `merge`'s `when [not] matched`.
        merge_when_condition: Condition = Condition::default(),
        /// The condition of a `check` constraint.
        check_constraint: Condition = Condition::default(),
        /// Any other `and` / `or` expression (in a select item, a function
        /// argument, a `set` value, …).
        other_conditions: Condition = Condition::default(),

        /// Window clauses, `over (…)` (R26).
        window: Window = Window::default(),
        /// `exists (subquery)` (R12).
        exists: Exists = Exists::default(),
        /// `case` expressions (R15).
        case: Case = Case::default(),
        /// The list of `in (a, b, c)`.
        in_list: KeptList = KeptList::default(),
        /// Function call arguments.
        function_args: KeptList = KeptList::default(),
        /// Subqueries in expressions and `from (…)` (R12).
        subquery: Subquery = Subquery::default(),
        /// Derived tables after a join or `apply` (R9).
        derived_table: DerivedTable = DerivedTable::default(),
        /// A CTE's body, `name as (query)` (R13).
        cte: Cte = Cte::default(),
        /// The list of CTEs after `with` (R13).
        cte_list: CteList = CteList::default(),
        /// The rows of `values (…), (…)` (R17).
        values: Values = Values::default(),
        /// The column list of `insert into t (a, b, c)` (R16).
        insert_columns: FilledList = FilledList::default(),
        /// The `when [not] matched … then` branches of a `merge` (R19).
        merge_when: MergeWhen = MergeWhen::default(),
        /// The arguments of `exec procedure …`.
        exec_args: KeptList = KeptList::default(),

        /// `create table` / table type / table variable columns and
        /// constraints (R20).
        create_table: CreateTable = CreateTable::default(),
        /// Foreign keys in `create table` and `alter table` (R20, R21).
        foreign_key: ForeignKey = ForeignKey::default(),
        /// The column list of `create index … on t (a, b)` (R21).
        index_columns: KeptList = KeptList::default(),
        /// The operations of `alter table` (R21).
        alter_table: AlterTable = AlterTable::default(),
        /// Procedure and function parameter lists (R22).
        routine_params: RoutineParams = RoutineParams::default(),
        /// The column list of `create view v (a, b)`.
        view_columns: KeptList = KeptList::default(),
    }
}

section! {
    /// A `select` list.
    SelectList {
        /// How the list wraps.
        wrap: Wrap = Wrap::ChopIfLong,
        /// Keep the author's line breaks between items (under
        /// `chop_if_long`, one of them chops the whole list).
        keep_line_breaks: bool = true,
        /// Comma position when the list is broken.
        comma: Comma = Comma::Leading,
        /// Align the aliases (`as x`) of a chopped list (`chop_if_long`,
        /// `always`; a filled or one-line list has none).
        align_aliases: bool = true,
    }
}

section! {
    /// A comma-separated list that keeps the author's layout by default.
    KeptList {
        /// How the list wraps.
        wrap: Wrap = Wrap::Never,
        /// Keep the author's line breaks between items.
        keep_line_breaks: bool = true,
        /// Comma position when the list is broken by the formatter.
        comma: Comma = Comma::Leading,
    }
}

section! {
    /// A comma-separated list filled to the margin by default.
    FilledList {
        /// How the list wraps.
        wrap: Wrap = Wrap::IfLong,
        /// Keep the author's line breaks between items.
        keep_line_breaks: bool = true,
        /// Comma position when the list is broken.
        comma: Comma = Comma::Leading,
    }
}

section! {
    /// The `where` / `having` condition: its top-level `and` / `or` chain,
    /// and the parenthesised groups inside it.
    ClauseCondition {
        /// How the top-level `and` / `or` chain wraps (one predicate per
        /// line by default).
        wrap: Wrap = Wrap::Always,
        /// Keep the author's line breaks before `and` / `or`.
        keep_line_breaks: bool = true,
        /// Parenthesised `and` / `or` groups (logical blocks, R25).
        groups: Groups = Groups::default(),
    }
}

section! {
    /// A condition laid out as the author wrote it by default: its
    /// top-level `and` / `or` chain, and the parenthesised groups inside it.
    Condition {
        /// How the top-level `and` / `or` chain wraps.
        wrap: Wrap = Wrap::Never,
        /// Keep the author's line breaks around `and` / `or`.
        keep_line_breaks: bool = true,
        /// Parenthesised `and` / `or` groups (logical blocks, R25).
        groups: Groups = Groups::default(),
    }
}

section! {
    /// Parenthesised `and` / `or` groups, `( a and b )` (R25). Broken, a
    /// group is a block: `(` ends its line, one operand per line 2 in from
    /// the `(` with `and` / `or` leading, `)` on its own line under the `(`.
    Groups {
        /// How a group wraps: `never` keeps it on one line, `if_long` fills
        /// it (continuation lines under its first operand, `)` at the end),
        /// `chop_if_long` makes it a block when it doesn't fit, `always`
        /// makes every group a block.
        wrap: Wrap = Wrap::ChopIfLong,
        /// A group written with a line break after its `(` is a block,
        /// whatever `wrap` says.
        keep_line_breaks: bool = true,
    }
}

section! {
    /// `over (partition by … order by … frame)` (R26). Broken, it's a
    /// block: `(` ends the line, each clause on its own line 2 in from the
    /// start of the select item, `)` under the item.
    Window {
        /// How the clauses wrap: `never` keeps them on one line, `if_long`
        /// fills them, `chop_if_long` makes a block when it doesn't fit,
        /// `always` makes every window with a clause a block.
        wrap: Wrap = Wrap::ChopIfLong,
        /// A window written over several lines is a block, whatever `wrap`
        /// says.
        keep_line_breaks: bool = true,
        /// The list after `partition by`.
        partition_by: KeptList = KeptList::default(),
        /// The list after `order by`.
        order_by: KeptList = KeptList::default(),
        /// The frame (`rows between … and …`).
        frame: Frame = Frame::default(),
    }
}

section! {
    /// A window frame.
    Frame {
        /// Keep the author's line breaks inside the frame (else on one
        /// line).
        keep_line_breaks: bool = true,
    }
}

section! {
    /// `[not] exists (subquery)`.
    Exists {
        /// How the subquery wraps (always a block: the query on lines of its
        /// own).
        wrap: Wrap = Wrap::Always,
        /// `(` on a line of its own (4 in from `if` / `where` / `exists`),
        /// the query 4 in from it, `)` under it. Else `(` ends the `exists`
        /// line, the query is one indent in from that line, `)` at its
        /// indent.
        lparen_on_new_line: bool = true,
    }
}

section! {
    /// `case … end`.
    Case {
        /// How `when` / `else` / `end` wrap.
        wrap: Wrap = Wrap::ChopIfLong,
        /// Keep the author's line breaks before `when` / `else` / `end` (one
        /// of them chops the whole `case`).
        keep_line_breaks: bool = true,
        /// Align the `then`s of a chopped `case`.
        align_then: bool = true,
    }
}

section! {
    /// A subquery in an expression or after `from`.
    Subquery {
        /// How the query wraps (always a block: `(` where the author put
        /// it, the query on the next line one indent in from the `(`, `)`
        /// under the `(`).
        wrap: Wrap = Wrap::Always,
    }
}

section! {
    /// A derived table after a join or `apply`.
    DerivedTable {
        /// `(` on a line of its own under the join. Else `(` ends the join's
        /// line and the query is one indent in from that line, `)` at its
        /// indent.
        lparen_on_new_line: bool = true,
    }
}

section! {
    /// A CTE's body.
    Cte {
        /// How the query wraps (always a block).
        wrap: Wrap = Wrap::Always,
        /// `(` on a line of its own (one indent in from `with`, or from the
        /// CTE's name when there's only one), the query one indent in from
        /// it, `)` under it. Else `with x as (` and the query one indent in
        /// from that line, `)` at its indent.
        lparen_on_new_line: bool = true,
    }
}

section! {
    /// The list of CTEs.
    CteList {
        /// Comma position: `   , b as` (names lined up) or `),` with the
        /// next name under the first.
        comma: Comma = Comma::Leading,
    }
}

section! {
    /// `values (…), (…)`.
    Values {
        /// How the rows wrap (several rows: one per line by default).
        wrap: Wrap = Wrap::Always,
        /// Keep the author's line breaks between rows.
        keep_line_breaks: bool = true,
        /// Comma position when the rows are broken.
        comma: Comma = Comma::Leading,
        /// Inside one row.
        row: FilledList = FilledList::default(),
    }
}

section! {
    /// `merge` branches.
    MergeWhen {
        /// The action (`update`, `insert`, `delete`) on its own line, one
        /// indent in from `merge`. Else it stays on the `when … then` line.
        action_on_new_line: bool = true,
    }
}

section! {
    /// `create table` columns and constraints.
    CreateTable {
        /// How the elements wrap (always one per line).
        wrap: Wrap = Wrap::Always,
        /// Comma position.
        comma: Comma = Comma::Trailing,
        /// Align column names, types and nullability in columns.
        align_columns: bool = true,
        /// A named column constraint (`constraint DF_x default 0`) on its
        /// own line one indent in from the column. Else where the author
        /// put it.
        named_constraint_on_new_line: bool = true,
    }
}

section! {
    /// Foreign keys.
    ForeignKey {
        /// `references …` on its own line one indent in from the line with
        /// `foreign key`. Else where the author put it.
        references_on_new_line: bool = true,
    }
}

section! {
    /// `alter table` operations.
    AlterTable {
        /// How the operations after the first wrap (the first always starts
        /// a line one indent in).
        wrap: Wrap = Wrap::Never,
        /// Keep the author's line breaks between operations.
        keep_line_breaks: bool = true,
    }
}

section! {
    /// Procedure and function parameters.
    RoutineParams {
        /// How the parameters wrap (always one per line).
        wrap: Wrap = Wrap::Always,
        /// Comma position.
        comma: Comma = Comma::Leading,
        /// Align the parameters' types.
        align_types: bool = true,
    }
}

/// A list's layout, as the builders use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListStyle {
    pub wrap: Wrap,
    pub keep: bool,
    pub comma: Comma,
}

macro_rules! list_style {
    ($($t:ty),*) => {$(
        impl $t {
            pub fn style(&self) -> ListStyle {
                ListStyle {
                    wrap: self.wrap,
                    keep: self.keep_line_breaks,
                    comma: self.comma,
                }
            }
        }
    )*};
}
list_style!(SelectList, KeptList, FilledList, Values);

/// A parenthesised group's or a chain's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrapStyle {
    pub wrap: Wrap,
    pub keep: bool,
}

impl Groups {
    pub fn style(&self) -> WrapStyle {
        WrapStyle {
            wrap: self.wrap,
            keep: self.keep_line_breaks,
        }
    }
}

impl ClauseCondition {
    pub fn style(&self) -> WrapStyle {
        WrapStyle {
            wrap: self.wrap,
            keep: self.keep_line_breaks,
        }
    }
}

impl Condition {
    pub fn style(&self) -> WrapStyle {
        WrapStyle {
            wrap: self.wrap,
            keep: self.keep_line_breaks,
        }
    }
}

/// The dialect names `dialect` accepts.
pub const DIALECTS: &[&str] = &[
    "mssql",
    "ansi",
    "bigquery",
    "clickhouse",
    "databricks",
    "duckdb",
    "generic",
    "hive",
    "mysql",
    "oracle",
    "postgresql",
    "redshift",
    "snowflake",
    "spark",
    "sqlite",
    "teradata",
];

impl Config {
    /// The parser dialect.
    pub fn dialect(&self) -> Box<dyn Dialect> {
        sqlparser::dialect::dialect_from_str(&self.dialect)
            .unwrap_or_else(|| Box::new(sqlparser::dialect::MsSqlDialect {}))
    }

    /// Parses and validates the text of a configuration file. `path` is
    /// only used in error messages.
    pub fn parse(text: &str, path: Option<&Path>) -> Result<Config, ConfigError> {
        let err = |line: Option<(usize, usize)>, message: String| ConfigError {
            path: path.map(Path::to_path_buf),
            at: line,
            message,
        };
        let cfg: Config = toml::from_str(text).map_err(|e| {
            let at = e.span().map(|s| line_col(text, s.start));
            err(at, e.message().trim_end().to_string())
        })?;
        let problems = cfg.validate();
        if let Some((key, msg)) = problems.into_iter().next() {
            return Err(err(locate(text, &key), msg));
        }
        Ok(cfg)
    }

    /// Reads, parses and validates a configuration file.
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|e| ConfigError {
            path: Some(path.to_path_buf()),
            at: None,
            message: e.to_string(),
        })?;
        Config::parse(&text, Some(path))
    }

    /// The configuration for formatting a file in `dir`: the first
    /// `sqp-fmt.toml` in `dir` or a directory above it, or the defaults.
    pub fn discover(dir: &Path) -> Result<(Config, Option<PathBuf>), ConfigError> {
        match find(dir) {
            Some(p) => Config::load(&p).map(|c| (c, Some(p))),
            None => Ok((Config::default(), None)),
        }
    }

    /// Checks every value against what's implemented. Returns (key,
    /// message) per problem, in key order.
    pub fn validate(&self) -> Vec<(String, String)> {
        use Comma::*;
        use Wrap::*;
        const ALL: &[Wrap] = &[Never, IfLong, ChopIfLong, Always];
        const COMMAS: &[Comma] = &[Leading, Trailing];
        const BOOLS: &[bool] = &[true, false];
        let mut v = Validator::default();
        if !DIALECTS.contains(&self.dialect.as_str()) {
            v.problem(
                "dialect",
                format!(
                    "unknown dialect \"{}\" (one of: {})",
                    self.dialect,
                    DIALECTS
                        .iter()
                        .map(|d| format!("\"{d}\""))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
        if !(20..=1000).contains(&self.margin) {
            v.problem(
                "margin",
                format!("margin {} is out of range (20 to 1000)", self.margin),
            );
        }
        if !(1..=16).contains(&self.indent) {
            v.problem(
                "indent",
                format!("indent {} is out of range (1 to 16)", self.indent),
            );
        }

        let s = &self.select_list;
        v.wrap("select_list.wrap", s.wrap, ALL);
        v.flag("select_list.keep_line_breaks", s.keep_line_breaks, BOOLS);
        v.comma("select_list.comma", s.comma, COMMAS);
        v.flag("select_list.align_aliases", s.align_aliases, BOOLS);
        v.list("group_by", &self.group_by, ALL, COMMAS);
        v.list("order_by", &self.order_by, ALL, COMMAS);

        // Conditions: every context's groups take every value; the chain
        // has a broken layout for `where`, `having` and `join … on`.
        for (key, c) in [("where", &self.where_), ("having", &self.having)] {
            v.wrap(&format!("{key}.wrap"), c.wrap, ALL);
            v.flag(
                &format!("{key}.keep_line_breaks"),
                c.keep_line_breaks,
                BOOLS,
            );
            v.groups(&format!("{key}.groups"), &c.groups);
        }
        v.condition("join_on", &self.join_on, ALL);
        for (key, c) in [
            ("case_when", &self.case_when),
            ("if_condition", &self.if_condition),
            ("while_condition", &self.while_condition),
            ("merge_on", &self.merge_on),
            ("merge_when_condition", &self.merge_when_condition),
            ("check_constraint", &self.check_constraint),
            ("other_conditions", &self.other_conditions),
        ] {
            v.condition(key, c, &[Never]);
        }

        let w = &self.window;
        v.wrap("window.wrap", w.wrap, ALL);
        v.flag("window.keep_line_breaks", w.keep_line_breaks, BOOLS);
        v.list("window.partition_by", &w.partition_by, ALL, COMMAS);
        v.list("window.order_by", &w.order_by, ALL, COMMAS);
        v.flag(
            "window.frame.keep_line_breaks",
            w.frame.keep_line_breaks,
            BOOLS,
        );

        v.wrap("exists.wrap", self.exists.wrap, &[Always]);
        v.flag(
            "exists.lparen_on_new_line",
            self.exists.lparen_on_new_line,
            BOOLS,
        );
        v.wrap("case.wrap", self.case.wrap, &[Never, ChopIfLong, Always]);
        v.flag("case.keep_line_breaks", self.case.keep_line_breaks, BOOLS);
        v.flag("case.align_then", self.case.align_then, BOOLS);
        v.list("in_list", &self.in_list, ALL, COMMAS);
        v.list("function_args", &self.function_args, ALL, COMMAS);
        v.wrap("subquery.wrap", self.subquery.wrap, &[Always]);
        v.flag(
            "derived_table.lparen_on_new_line",
            self.derived_table.lparen_on_new_line,
            BOOLS,
        );
        v.wrap("cte.wrap", self.cte.wrap, &[Always]);
        v.flag("cte.lparen_on_new_line", self.cte.lparen_on_new_line, BOOLS);
        v.comma("cte_list.comma", self.cte_list.comma, COMMAS);
        let vs = &self.values;
        v.wrap("values.wrap", vs.wrap, ALL);
        v.flag("values.keep_line_breaks", vs.keep_line_breaks, BOOLS);
        v.comma("values.comma", vs.comma, COMMAS);
        v.list("values.row", &vs.row, ALL, COMMAS);
        v.list("insert_columns", &self.insert_columns, ALL, COMMAS);
        v.flag(
            "merge_when.action_on_new_line",
            self.merge_when.action_on_new_line,
            BOOLS,
        );
        v.list("exec_args", &self.exec_args, ALL, COMMAS);

        let ct = &self.create_table;
        v.wrap("create_table.wrap", ct.wrap, &[Always]);
        v.comma("create_table.comma", ct.comma, COMMAS);
        v.flag("create_table.align_columns", ct.align_columns, BOOLS);
        v.flag(
            "create_table.named_constraint_on_new_line",
            ct.named_constraint_on_new_line,
            BOOLS,
        );
        v.flag(
            "foreign_key.references_on_new_line",
            self.foreign_key.references_on_new_line,
            BOOLS,
        );
        v.list("index_columns", &self.index_columns, ALL, COMMAS);
        v.wrap(
            "alter_table.wrap",
            self.alter_table.wrap,
            &[Never, ChopIfLong, Always],
        );
        v.flag(
            "alter_table.keep_line_breaks",
            self.alter_table.keep_line_breaks,
            BOOLS,
        );
        let rp = &self.routine_params;
        v.wrap("routine_params.wrap", rp.wrap, &[Always]);
        v.comma("routine_params.comma", rp.comma, COMMAS);
        v.flag("routine_params.align_types", rp.align_types, BOOLS);
        v.list("view_columns", &self.view_columns, ALL, COMMAS);
        v.problems
    }
}

#[derive(Default)]
struct Validator {
    problems: Vec<(String, String)>,
}

impl Validator {
    fn problem(&mut self, key: &str, msg: String) {
        self.problems.push((key.to_string(), msg));
    }

    fn not_implemented(&mut self, key: &str, value: &str, ok: &[String]) {
        self.problem(
            key,
            format!(
                "{key} = {value} is not implemented yet for {key} (implemented: {})",
                ok.join(", ")
            ),
        );
    }

    fn wrap(&mut self, key: &str, v: Wrap, ok: &[Wrap]) {
        if !ok.contains(&v) {
            let ok: Vec<String> = ok.iter().map(|w| format!("\"{}\"", w.name())).collect();
            self.not_implemented(key, &format!("\"{}\"", v.name()), &ok);
        }
    }

    fn comma(&mut self, key: &str, v: Comma, ok: &[Comma]) {
        if !ok.contains(&v) {
            let ok: Vec<String> = ok.iter().map(|c| format!("\"{}\"", c.name())).collect();
            self.not_implemented(key, &format!("\"{}\"", v.name()), &ok);
        }
    }

    fn flag(&mut self, key: &str, v: bool, ok: &[bool]) {
        if !ok.contains(&v) {
            let ok: Vec<String> = ok.iter().map(|b| b.to_string()).collect();
            self.not_implemented(key, &v.to_string(), &ok);
        }
    }

    fn list(&mut self, key: &str, l: &impl AsList, wraps: &[Wrap], commas: &[Comma]) {
        let s = l.list_style();
        self.wrap(&format!("{key}.wrap"), s.wrap, wraps);
        self.flag(&format!("{key}.keep_line_breaks"), s.keep, &[true, false]);
        self.comma(&format!("{key}.comma"), s.comma, commas);
    }

    fn groups(&mut self, key: &str, g: &Groups) {
        use Wrap::*;
        self.wrap(
            &format!("{key}.wrap"),
            g.wrap,
            &[Never, IfLong, ChopIfLong, Always],
        );
        self.flag(
            &format!("{key}.keep_line_breaks"),
            g.keep_line_breaks,
            &[true, false],
        );
    }

    fn condition(&mut self, key: &str, c: &Condition, chain: &[Wrap]) {
        self.wrap(&format!("{key}.wrap"), c.wrap, chain);
        self.flag(
            &format!("{key}.keep_line_breaks"),
            c.keep_line_breaks,
            &[true, false],
        );
        self.groups(&format!("{key}.groups"), &c.groups);
    }
}

trait AsList {
    fn list_style(&self) -> ListStyle;
}

macro_rules! as_list {
    ($($t:ty),*) => {$(
        impl AsList for $t {
            fn list_style(&self) -> ListStyle {
                self.style()
            }
        }
    )*};
}
as_list!(KeptList, FilledList);

/// The first `sqp-fmt.toml` in `dir` or a directory above it.
pub fn find(dir: &Path) -> Option<PathBuf> {
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    let abs = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    abs.ancestors()
        .map(|d| d.join(FILE_NAME))
        .find(|p| p.is_file())
}

/// An error in a configuration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub path: Option<PathBuf>,
    /// 1-based line and column of the offending key or value.
    pub at: Option<(usize, usize)>,
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.path {
            Some(p) => write!(f, "{}", p.display())?,
            None => write!(f, "{FILE_NAME}")?,
        }
        if let Some((line, col)) = self.at {
            write!(f, ":{line}:{col}")?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for ConfigError {}

/// 1-based line and column of byte offset `at`.
fn line_col(text: &str, at: usize) -> (usize, usize) {
    let before = &text[..at.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, col)
}

/// Where the value of dotted `key` is written in the file.
fn locate(text: &str, key: &str) -> Option<(usize, usize)> {
    let doc = toml::de::DeTable::parse(text).ok()?;
    let mut table = doc.get_ref();
    let mut parts = key.split('.').peekable();
    while let Some(part) = parts.next() {
        let (_, value) = table.iter().find(|(k, _)| k.get_ref().as_ref() == part)?;
        if parts.peek().is_none() {
            return Some(line_col(text, value.span().start));
        }
        match value.get_ref() {
            toml::de::DeValue::Table(t) => table = t,
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(text: &str) -> String {
        Config::parse(text, Some(Path::new("t/sqp-fmt.toml")))
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn empty_file_is_the_defaults() {
        assert_eq!(Config::parse("", None).unwrap(), Config::default());
        assert!(Config::default().validate().is_empty());
    }

    #[test]
    fn values_are_read() {
        let c = Config::parse(
            "margin = 100\nkeyword_case = \"upper\"\n[where.groups]\nwrap = \"always\"\n[window]\nwrap = \"never\"\nkeep_line_breaks = false\n[select_list]\ncomma = \"trailing\"\n",
            None,
        )
        .unwrap();
        assert_eq!(c.margin, 100);
        assert_eq!(c.keyword_case, WordCase::Upper);
        assert_eq!(c.where_.groups.wrap, Wrap::Always);
        // Keys left out keep their section's defaults.
        assert_eq!(c.where_.wrap, Wrap::Always);
        assert!(c.where_.groups.keep_line_breaks);
        assert_eq!(c.window.wrap, Wrap::Never);
        assert!(!c.window.keep_line_breaks);
        assert_eq!(c.select_list.comma, Comma::Trailing);
        assert_eq!(c.select_list.wrap, Wrap::ChopIfLong);
        assert_eq!(c.group_by.wrap, Wrap::Never);
    }

    #[test]
    fn dotted_keys_and_inline_tables() {
        let c = Config::parse(
            "where.groups.wrap = \"never\"\nwindow = { wrap = \"always\" }\n",
            None,
        )
        .unwrap();
        assert_eq!(c.where_.groups.wrap, Wrap::Never);
        assert_eq!(c.window.wrap, Wrap::Always);
    }

    #[test]
    fn unknown_key() {
        let e = err("margin = 100\n[window]\nwarp = \"never\"\n");
        assert!(
            e.starts_with("t/sqp-fmt.toml:3:1: unknown field `warp`"),
            "{e}"
        );
        assert!(e.contains("`wrap`"), "{e}");
    }

    #[test]
    fn unknown_section() {
        let e = err("[selcet_list]\nwrap = \"never\"\n");
        assert!(
            e.starts_with("t/sqp-fmt.toml:1:2: unknown field `selcet_list`"),
            "{e}"
        );
    }

    #[test]
    fn invalid_value() {
        let e = err("[where.groups]\nwrap = \"sometimes\"\n");
        assert!(
            e.starts_with("t/sqp-fmt.toml:2:8: unknown variant `sometimes`"),
            "{e}"
        );
        assert!(e.contains("`chop_if_long`"), "{e}");
        let e = err("margin = \"wide\"\n");
        assert!(e.starts_with("t/sqp-fmt.toml:1:10: invalid type"), "{e}");
        let e = err("[case]\nalign_then = 1\n");
        assert!(e.starts_with("t/sqp-fmt.toml:2:14: invalid type"), "{e}");
    }

    #[test]
    fn out_of_range() {
        let e = err("margin = 5\n");
        assert_eq!(
            e,
            "t/sqp-fmt.toml:1:10: margin 5 is out of range (20 to 1000)"
        );
        let e = err("indent = 0\n");
        assert_eq!(e, "t/sqp-fmt.toml:1:10: indent 0 is out of range (1 to 16)");
    }

    #[test]
    fn unknown_dialect() {
        let e = err("dialect = \"tsql\"\n");
        assert!(
            e.starts_with("t/sqp-fmt.toml:1:11: unknown dialect \"tsql\" (one of: \"mssql\""),
            "{e}"
        );
        for d in DIALECTS {
            let c = Config::parse(&format!("dialect = \"{d}\"\n"), None).unwrap();
            let _ = c.dialect();
            assert!(sqlparser::dialect::dialect_from_str(d).is_some(), "{d}");
        }
    }

    #[test]
    fn not_implemented() {
        let e = err("[exists]\nwrap = \"never\"\n");
        assert_eq!(
            e,
            "t/sqp-fmt.toml:2:8: exists.wrap = \"never\" is not implemented yet for exists.wrap (implemented: \"always\")"
        );
        let e = err("[if_condition]\nwrap = \"always\"\n");
        assert!(
            e.contains(
                "if_condition.wrap = \"always\" is not implemented yet for if_condition.wrap"
            ),
            "{e}"
        );
        let e = err("case = { wrap = \"if_long\" }\n");
        assert!(
            e.starts_with("t/sqp-fmt.toml:1:17: case.wrap = \"if_long\" is not implemented yet"),
            "{e}"
        );
    }

    #[test]
    fn find_walks_up() {
        let root = std::env::temp_dir().join(format!("sqp-fmt-find-{}", std::process::id()));
        let deep = root.join("a/b/c");
        std::fs::create_dir_all(&deep).unwrap();
        assert!(find(&deep).is_none_or(|p| !p.starts_with(&root)));
        std::fs::write(root.join("a").join(FILE_NAME), "margin = 80\n").unwrap();
        assert_eq!(find(&deep), Some(root.join("a").join(FILE_NAME)));
        std::fs::write(root.join("a/b").join(FILE_NAME), "margin = 90\n").unwrap();
        assert_eq!(find(&deep), Some(root.join("a/b").join(FILE_NAME)));
        let (c, p) = Config::discover(&deep).unwrap();
        assert_eq!(c.margin, 90);
        assert_eq!(p, Some(root.join("a/b").join(FILE_NAME)));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
