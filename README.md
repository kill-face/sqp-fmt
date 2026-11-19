# tsqlfmt

A T-SQL formatter that only changes whitespace and case, built on a fork of
[sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs). The target
style is in [STYLE.md](STYLE.md).

## Layout

- `../datafusion-sqlparser-rs`: the parser fork (`origin` =
  `kill-face/datafusion-sqlparser-rs`, `upstream` = `apache/...`), worked on
  under the `tsql` bookmark. Each grammar fix is its own change with tests, so
  it can go upstream.
- `src/lib.rs`: splits a file into `GO` batches (SQLCMD `:r` / `:setvar`
  lines are skipped), keeping file line numbers, and parses each batch with
  the MS SQL dialect, keeping comments.
- `src/main.rs`: the `tsqlfmt` command.

## Commands

```sh
nix develop                      # or direnv
cargo run --release -- check corpus/private/           # parse rate + grouped errors
cargo run --release -- check --files corpus/private/   # ... plus every failing file
cargo run --release -- spans corpus/private/           # AST span audit, grouped findings
cargo run --release -- spans --details corpus/private/ # ... with every location
```

`spans` checks, for every batch that parses, that the AST's source spans are
complete enough to map every token back to its node (see `src/spans.rs`):
every token but `;` lies in exactly one top-level statement span, which runs
from the statement's first token to its last; statements, queries, selects,
expressions, table factors, joins, functions (with `OVER`), column
definitions and options, constraints and parameters have non-empty spans
inside their parent's, on token boundaries; list items (select items,
columns, arguments, statements in a block) have only separators between
them; and a node's span is not shorter than its `Display`, nor starts or
ends with a different token.

Formatting itself (`tsqlfmt < in.sql > out.sql`, for conform.nvim) comes once
the whole test bed parses.

## Status

| Date | corpus files parsing |
|---|---|
| 2026-11-19 | 263 / 510 (upstream sqlparser-rs v0.64.0) |
| 2026-11-19 | 292 / 510 (fork: `PRIMARY KEY` / `UNIQUE` `CLUSTERED`) |
| 2026-11-19 | 348 / 510 (fork: `CREATE [UNIQUE] [NON]CLUSTERED [COLUMNSTORE] INDEX`) |
| 2026-11-19 | 414 / 510 (fork `tsql`, 20 changes; 476 / 510 with `require_semicolon_stmt_delimiter: false`) |
| 2026-11-19 | 478 / 510 (harness: optional semicolons on, `go;`, `GO` inside comments/strings ignored). The 32 left: 30 placeholder templates that aren't valid SQL (`values ();`, empty `from (values )`, `check ()`), one file of CREATE LOGIN / USER … FOR LOGIN and one of ALTER DATABASE … SET and CREATE DATABASE file lists |

Span audit (`tsqlfmt spans`, same corpus, 1343 parsed batches, 1777
top-level statements, 298605 nested nodes, 687211 tokens):

| Date | Tokens outside statement spans | Empty spans | Findings |
|---|---|---|---|
| 2026-11-19 | 153712 (22.37%) | 3364 | 15166 in 117 groups (fork `tsql` before the span work) |
| 2026-11-19 | 41 (0.01%) | 0 | 96 in 5 groups (fork `tsql` + 18 span changes): the `USE` keyword (41 statements, 2 checks each), the `)` of table hints (12), `WITHIN GROUP (...)` (1, 2 checks) |

## Decisions

- Block indentation: contents at the `(` column + 2, `)` under the `(`
  (STYLE.md, change 1). Decided 2026-11-19.
- The fork stays local for now; nothing is pushed to GitHub.
