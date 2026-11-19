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
- `src/format.rs`: formats a file: line endings and BOM, `GO` / SQLCMD lines
  (kept as written), `@formatter:off` regions, one batch at a time; and the
  safety net (below).
- `src/layout.rs`: the layout model and printer. A batch is its tokens, the
  trivia (comments, line breaks) between them, and a rule per gap between two
  tokens (keep the source's line break / one line / break / break with a
  group / fill to the margin), with indents relative to tokens printed
  earlier and alignment columns. The printer only ever writes whitespace
  between the tokens, so it can't drop or reorder anything.
- `src/build/`: sets the rules from the AST (`stmt.rs` statements and
  blocks, `query.rs` queries, `expr.rs` expressions, `dml.rs`, `ddl.rs`).
  Every gap starts as "keep the source layout, re-indented with its
  surroundings"; handlers replace that for the constructs they know, so
  anything unknown keeps its layout.
- `src/case.rs`: keyword / built-in function / system type case, deciding
  identifiers by the AST.
- `src/corpus.rs`: the `corpus` progress measure.
- `src/main.rs`: the `tsqlfmt` command.

## Formatting

`tsqlfmt [FILE]` formats FILE (or stdin) to stdout. For conform.nvim:

```lua
require("conform").setup({
  formatters_by_ft = { sql = { "tsqlfmt" } },
  formatters = { tsqlfmt = { command = "tsqlfmt", stdin = true } },
})
```

Safety: each batch's output is tokenized again and compared with the
input's tokens, comments included; only whitespace may differ, and case only
for the words the formatter lower-cased. If they differ, or formatting
panics, that batch is emitted exactly as written. Batches that don't parse,
`@formatter:off` … `@formatter:on` lines, `GO` lines and SQLCMD lines are
left as written too. CRLF / LF, a BOM and a missing final newline are kept
(a file with mixed line endings is only formatted if every `\r` is inside a
comment or string, else left alone).

## Commands

```sh
nix develop                      # or direnv
cargo run --release -- corpus corpus/private/          # format the test bed in memory: lines unchanged, fallbacks, idempotency
cargo run --release -- corpus --list corpus/private/   # ... with every changed file
cargo run --release -- corpus --diff FILE              # unified diff of what formatting FILE changes
cargo test                                             # golden tests (tests/golden), token and idempotency checks
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

`corpus` formats every file in memory and reports the files and lines left
unchanged, the batches the safety net left as written (should be 0), the
files where formatting twice differs from formatting once (should be 0), the
slowest file and the files with the biggest diffs. Golden tests: each
`tests/golden/NAME.sql` must format to `NAME.expected.sql`
(`TSQLFMT_BLESS=1 cargo test` rewrites the expected files; review the diff).

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

Formatter (`tsqlfmt corpus`, same corpus: 510 files, 65365 lines; the
largest file, 15539 lines / 1.4 MB, formats in about
0.33 s). One generated seed file (12712 lines of `values` rows written with
upper-case `NULL` and trailing commas) accounts for most changed lines; the
last column leaves it out.

| Date | Files unchanged | Lines unchanged | Safety fallbacks | Not idempotent | Lines unchanged without the seed file | Step |
|---|---|---|---|---|---|---|
| 2026-11-19 | 317 / 510 | 49825 / 65365 (76.23%) | 0 | 0 | 49787 / 52653 (94.56%) | Core: printer, safety net, case; source layout kept, statements at column 0 |
| 2026-11-19 | 443 / 510 | 52292 / 65365 (80.00%) | 0 | 0 | 52253 / 52653 (99.24%) | Statement families: select / from / join / where, CTEs, subqueries, insert / values / update / delete / merge, case, create table / index / alter table, procedures / functions / views / triggers, declare / set, if / while / try blocks (STYLE.md rules R1-R24, R27) |

## Decisions

- Block indentation: contents at the `(` column + 2, `)` under the `(`
  (STYLE.md, change 1). Decided 2026-11-19.
- The fork stays local for now; nothing is pushed to GitHub.
