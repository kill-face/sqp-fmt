# Configuration: `sqp-fmt.toml`

Every layout choice in [STYLE.md](../STYLE.md) can be set per construct in a
`sqp-fmt.toml`. Without the file (or with an empty one) sqp-fmt formats
exactly as STYLE.md describes: every key's default is today's house style.

## Finding the file

- `sqp-fmt FILE`: the first `sqp-fmt.toml` in FILE's directory or a
  directory above it.
- `sqp-fmt` (stdin): the same search from the directory of
  `--stdin-filepath PATH` (the path the buffer will be saved to; it needn't
  exist), else from the current directory. Editors pass the buffer's path
  this way, as with rustfmt's and prettier's `--stdin-filepath`.
- `--config FILE` uses FILE and skips the search.
- The nearest file is the whole configuration: files further up are not
  merged in.
- `SQP_FMT_CEILING_DIRECTORIES` (`:`-separated absolute paths, like Git's
  `GIT_CEILING_DIRECTORIES`) stops the search from going up into those
  directories: neither they nor anything above them is searched.
- `--write` and `--check` search separately for each file they format.
- `sqp-fmt corpus` uses `--config FILE` or the defaults; it doesn't search.

A configuration error stops sqp-fmt before it writes anything (exit code 2)
with a message that points at the key:

```text
sqp-fmt: proj/sqp-fmt.toml:3:1: unknown field `warp`, expected one of `wrap`, `keep_line_breaks`, `partition_by`, `order_by`, `frame`
sqp-fmt: proj/sqp-fmt.toml:2:8: unknown variant `sometimes`, expected one of `never`, `if_long`, `chop_if_long`, `always`
sqp-fmt: proj/sqp-fmt.toml:2:8: exists.wrap = "never" is not implemented yet for exists.wrap (implemented: "always")
```

Unknown keys and sections, values of the wrong type, out-of-range numbers
and unknown dialects are errors. So is a value whose layout isn't built yet
(see [Implemented values](#implemented-values)): no option is ever silently
ignored. Keys can be written as sections (`[where.groups]`), dotted keys
(`where.groups.wrap = "always"`) or inline tables.

## Vocabulary

The names follow JetBrains' code-style settings (`style/jetbrains-codestyle.xml`
is the scheme sqp-fmt's house style starts from).

### `wrap`

| Value | JetBrains | Meaning |
|---|---|---|
| `"never"` | Do not wrap | On one line, however long. |
| `"if_long"` | Wrap if long | Filled: a line break only where the next element doesn't fit in the margin. |
| `"chop_if_long"` | Chop down if long | On one line if it fits, else every element on a line of its own. |
| `"always"` | Wrap always / Chop always | Every element on a line of its own. |

For a parenthesised construct, "chopped" means the **block** layout
sqp-fmt already has: `(` ends its line, the contents are indented 2 from the
`(` column, `)` is on its own line under the `(`. That one layout stands for
JetBrains' separate "new line after `(`", "place `)` on new line" and
"align when multiline" options, which sqp-fmt doesn't split up (yet).

### `keep_line_breaks`

JetBrains' "Keep when reformatting: line breaks". In sqp-fmt the author's
line break is a signal:

- in a **list** (select items, arguments, rows): a line break between
  elements is kept where it is (`never`, `if_long`), and under
  `chop_if_long` it chops the whole list (the author already decided it
  doesn't belong on one line);
- in a **block** (a parenthesised `and` / `or` group, a window): a line
  break right after the `(` (for a window: anywhere inside it, or right
  after the `(` under `if_long`) makes it a block, whatever `wrap` says.
  This is how blocks behave by default (STYLE.md changes 1 and 2).

`keep_line_breaks = false` ignores the author's line breaks.

### `comma`

`"leading"` (`, b` starts the line) or `"trailing"` (`a,` ends it): JetBrains'
"Place comma: before / after element" (`QUERY_EL_COMMA`, `ROUTINE_ARG_COMMA`,
`CORTEGE_COMMA_1ST`). It applies to the line breaks the formatter makes; a
line break kept from the source stays on the side of the comma the author put
it, except that a filled list (`if_long`) moves it to its own comma side.

## Every key, with its default

This file is the defaults: formatting with it is the same as formatting
without it.

```toml
dialect = "mssql"
margin = 120
indent = 4
keyword_case = "lower"
function_case = "lower"
type_case = "lower"

[select_list]
wrap = "chop_if_long"
keep_line_breaks = true
comma = "leading"
align_aliases = true

[group_by]
wrap = "never"
keep_line_breaks = true
comma = "leading"

[order_by]
wrap = "never"
keep_line_breaks = true
comma = "leading"

[where]
wrap = "always"
keep_line_breaks = true
indent = 2                # `and` 2 in from `where` (`or` 3, so predicates line up)
[where.groups]
wrap = "chop_if_long"
keep_line_breaks = true

[having]
wrap = "always"
keep_line_breaks = true
indent = 2
[having.groups]
wrap = "chop_if_long"
keep_line_breaks = true

[join_on]
wrap = "never"
keep_line_breaks = true
# indent = 4              # `on` this far in from `join`; unset: `indent`
[join_on.groups]
wrap = "chop_if_long"
keep_line_breaks = true

# The same keys as join_on (but no `indent`), with the same defaults, for
# each of these conditions: case_when, if_condition, while_condition,
# merge_on, merge_when_condition, check_constraint, other_conditions.

[case_when]
wrap = "never"
keep_line_breaks = true
[case_when.groups]
wrap = "chop_if_long"
keep_line_breaks = true

[if_condition]
wrap = "never"
keep_line_breaks = true
[if_condition.groups]
wrap = "chop_if_long"
keep_line_breaks = true

[while_condition]
wrap = "never"
keep_line_breaks = true
[while_condition.groups]
wrap = "chop_if_long"
keep_line_breaks = true

[merge_on]
wrap = "never"
keep_line_breaks = true
[merge_on.groups]
wrap = "chop_if_long"
keep_line_breaks = true

[merge_when_condition]
wrap = "never"
keep_line_breaks = true
[merge_when_condition.groups]
wrap = "chop_if_long"
keep_line_breaks = true

[check_constraint]
wrap = "never"
keep_line_breaks = true
[check_constraint.groups]
wrap = "chop_if_long"
keep_line_breaks = true

[other_conditions]
wrap = "never"
keep_line_breaks = true
[other_conditions.groups]
wrap = "chop_if_long"
keep_line_breaks = true

[window]
wrap = "chop_if_long"
keep_line_breaks = true
[window.partition_by]
wrap = "never"
keep_line_breaks = true
comma = "leading"
[window.order_by]
wrap = "never"
keep_line_breaks = true
comma = "leading"
[window.frame]
keep_line_breaks = true

[exists]
wrap = "always"
lparen_on_new_line = true

[case]
wrap = "chop_if_long"
keep_line_breaks = true
align_then = true

[in_list]
wrap = "never"
keep_line_breaks = true
comma = "leading"

[function_args]
wrap = "never"
keep_line_breaks = true
comma = "leading"

[subquery]
wrap = "always"

[derived_table]
lparen_on_new_line = true

[cte]
wrap = "always"
lparen_on_new_line = true

[cte_list]
comma = "leading"

[values]
wrap = "always"
keep_line_breaks = true
comma = "leading"
[values.row]
wrap = "if_long"
keep_line_breaks = true
comma = "leading"

[insert_columns]
wrap = "if_long"
keep_line_breaks = true
comma = "leading"

[merge_when]
action_on_new_line = true

[exec_args]
wrap = "never"
keep_line_breaks = true
comma = "leading"

[create_table]
wrap = "always"
comma = "trailing"
align_columns = true
named_constraint_on_new_line = true

[foreign_key]
references_on_new_line = true

[index_columns]
wrap = "never"
keep_line_breaks = true
comma = "leading"

[alter_table]
wrap = "never"
keep_line_breaks = true

[routine_params]
wrap = "always"
comma = "leading"
align_types = true

[view_columns]
wrap = "never"
keep_line_breaks = true
comma = "leading"
```

## Implemented values

What each key accepts today. Any other value of a known key is rejected with
"not implemented yet for <key>". `ALL` is `never`, `if_long`,
`chop_if_long`, `always`.

| Key | Implemented | Default |
|---|---|---|
| `dialect` | `mssql`, `ansi`, `bigquery`, `clickhouse`, `databricks`, `duckdb`, `generic`, `hive`, `mysql`, `oracle`, `postgresql`, `redshift`, `snowflake`, `spark`, `sqlite`, `teradata` | `mssql` |
| `margin` | 20 to 1000 | 120 |
| `indent` | 1 to 16 | 4 |
| `keyword_case`, `function_case`, `type_case` | `lower`, `upper`, `as_is` | `lower` |
| `select_list.wrap` | ALL | `chop_if_long` |
| `select_list.keep_line_breaks`, `.align_aliases` | `true`, `false` | `true` |
| `select_list.comma` | `leading`, `trailing` | `leading` |
| `group_by.*`, `order_by.*` (`wrap`, `keep_line_breaks`, `comma`) | ALL; `true`, `false`; `leading`, `trailing` | `never`, `true`, `leading` |
| `where.wrap`, `having.wrap` | ALL | `always` |
| `join_on.wrap` | ALL | `never` |
| `where.indent`, `having.indent` | 0 to 16 (`and` this far in from the keyword, `or` one more) | 2 |
| `join_on.indent` | 0 to 16 (`on` this far in from `join`; `and` / `or` under `on`) | unset: `indent` |
| `case_when.wrap`, `if_condition.wrap`, `while_condition.wrap`, `merge_on.wrap`, `merge_when_condition.wrap`, `check_constraint.wrap`, `other_conditions.wrap` | `never` only (no broken layout for these chains yet) | `never` |
| `<condition>.keep_line_breaks` (all ten conditions) | `true`, `false` | `true` |
| `<condition>.groups.wrap` (all ten) | ALL | `chop_if_long` |
| `<condition>.groups.keep_line_breaks` (all ten) | `true`, `false` | `true` |
| `window.wrap` | ALL | `chop_if_long` |
| `window.keep_line_breaks`, `window.frame.keep_line_breaks` | `true`, `false` | `true` |
| `window.partition_by.*`, `window.order_by.*` | ALL; `true`, `false`; `leading`, `trailing` | `never`, `true`, `leading` |
| `exists.wrap` | `always` only | `always` |
| `exists.lparen_on_new_line` | `true`, `false` | `true` |
| `case.wrap` | `never`, `chop_if_long`, `always` (not `if_long`) | `chop_if_long` |
| `case.keep_line_breaks`, `case.align_then` | `true`, `false` | `true` |
| `in_list.*`, `function_args.*`, `exec_args.*` | ALL; `true`, `false`; `leading`, `trailing` | `never`, `true`, `leading` |
| `subquery.wrap` | `always` only | `always` |
| `derived_table.lparen_on_new_line` | `true`, `false` | `true` |
| `cte.wrap` | `always` only | `always` |
| `cte.lparen_on_new_line` | `true`, `false` | `true` |
| `cte_list.comma` | `leading`, `trailing` | `leading` |
| `values.*` | ALL; `true`, `false`; `leading`, `trailing` | `always`, `true`, `leading` |
| `values.row.*`, `insert_columns.*` | ALL; `true`, `false`; `leading`, `trailing` | `if_long`, `true`, `leading` |
| `merge_when.action_on_new_line` | `true`, `false` | `true` |
| `create_table.wrap` | `always` only | `always` |
| `create_table.comma` | `leading`, `trailing` | `trailing` |
| `create_table.align_columns`, `.named_constraint_on_new_line` | `true`, `false` | `true` |
| `foreign_key.references_on_new_line` | `true`, `false` | `true` |
| `index_columns.*`, `view_columns.*` | ALL; `true`, `false`; `leading`, `trailing` | `never`, `true`, `leading` |
| `alter_table.wrap` | `never`, `chop_if_long`, `always` (not `if_long`) | `never` |
| `alter_table.keep_line_breaks` | `true`, `false` | `true` |
| `routine_params.wrap` | `always` only | `always` |
| `routine_params.comma` | `leading`, `trailing` | `leading` |
| `routine_params.align_types` | `true`, `false` | `true` |

Every implemented non-default value has a golden test in `tests/golden/`
(`NN-name.sql` with its `NN-name.toml`, formatted to
`NN-name.expected.sql`): 30 to 34 conditions, 35 to 39 windows, 40 to 42
query lists, 43 to 45 parenthesised lists and `exec`, 46 to 48 `values` and
`insert`, 49 to 51 `case`, 52 subqueries / CTEs / `merge`, 53 to 55 DDL, 56
and 57 the globals and case, 58 the dialect.

## Reference

Each section: its keys, the JetBrains counterpart, and before (input) /
after (output) examples. Unless a key is named, the examples use the
defaults.

### Globals

| Key | Values | JetBrains |
|---|---|---|
| `dialect` | see the table above | none (DataGrip picks the dialect per data source) |
| `margin` | columns | Hard wrap at (`RIGHT_MARGIN`); the scheme's visual guide `SOFT_MARGINS` = 120 |
| `indent` | spaces | Tabs and Indents: Indent (`INDENT_SIZE`) |
| `keyword_case` | `lower`, `upper`, `as_is` | Case: Keywords (`KEYWORD_CASE`) |
| `function_case` | `lower`, `upper`, `as_is` | Case: built-in functions (Database Navigator's `FUNCTION_CASE`) |
| `type_case` | `lower`, `upper`, `as_is` | Case: Built-in types (`TYPE_CASE`) |

`dialect` selects the sqlparser dialect for parsing and for the safety net's
re-tokenizing. The layout rules are written for T-SQL (and `GO` batches,
SQLCMD lines and `[quoted]` identifiers are T-SQL's); with another dialect
whatever doesn't parse, or wouldn't come back with the same tokens, is left
exactly as written, so nothing breaks, but the result may look odd (with
`postgresql`, `[a]` is an array subscript and gets a space: `[ a]`).

`indent` is one level: statement blocks (`begin … end`, `if` bodies),
clause continuation lines (a join's `on`, `create index … on`, `alter table`
operations, trigger lines), subquery and CTE contents, `case` branches, table
columns and `merge` actions. Alignment offsets (`and` 2 in under `where`,
`or` 3 in, leading commas 2 left of the items) and block contents (2 in from
the `(`) don't change with it.

The case keys apply to what `case.rs` already re-cases: keywords (and date
parts in `dateadd`-style functions), built-in function names (and `@@`
functions, `cast` / `convert`), system type names. Identifiers are never
touched. A word is classed by the AST where it's an identifier (a function
name, a type name); otherwise by name and position: a system type name is a
type (`table` only after a `@variable`, `xml` not after `for`), a built-in
function name right before `(` is a function (not `identity (1,1)`), other
keywords are keywords.

```sql
-- before
declare @x nvarchar(max) = convert(varchar(10), 5)
select Count(*) as Cnt, getdate() as Now from dbo.Widget w
-- after: indent = 2, keyword_case = "upper", function_case = "as_is", type_case = "upper"
DECLARE @x NVARCHAR(MAX) = convert(VARCHAR(10), 5)
SELECT Count(*) AS Cnt, getdate() AS Now
FROM dbo.Widget w
```

### `[select_list]` (R8)

| Key | JetBrains |
|---|---|
| `wrap` | Queries > SELECT: elements wrapping (`WRAP_INSIDE_SELECT`; `NEW_LINE_AFTER_SELECT_ITEM`) |
| `keep_line_breaks` | Keep when reformatting: line breaks |
| `comma` | Place comma (`QUERY_EL_COMMA`) |
| `align_aliases` | Align `AS` in SELECT statement (`ALIGN_AS_IN_SELECT_STATEMENT`); only a chopped list (`chop_if_long`, `always`) has aligned aliases |

Broken, the items start lines at the first item's column, leading commas 2
left of it.

```sql
-- before
select a.Id, a.Name as CustomerName, a.CreatedAt as Created from dbo.Alpha a
-- after: wrap = "always" (default: unchanged, it fits)
select a.Id
     , a.Name      as CustomerName
     , a.CreatedAt as Created
from dbo.Alpha a
-- after: wrap = "always", comma = "trailing", align_aliases = false
select a.Id,
       a.Name as CustomerName,
       a.CreatedAt as Created
from dbo.Alpha a
-- after: margin = 60, wrap = "if_long", comma = "trailing" (filled)
select OrderIdentifier, CustomerIdentifier,
       RegionIdentifier, ProductIdentifier,
       WarehouseIdentifier
from dbo.Orders
```

### `[group_by]`, `[order_by]` (R11)

| Key | JetBrains |
|---|---|
| `wrap` | `WRAP_INSIDE_GROUP_BY`, `WRAP_INSIDE_ORDER_BY` |
| `keep_line_breaks` | Keep when reformatting: line breaks |
| `comma` | `QUERY_EL_COMMA` |

`order_by` is a query's `order by`; a window's is `window.order_by`.

```sql
-- before
select a.Id from dbo.Alpha a group by a.Id, a.Name order by a.Id,
  a.Name
-- after (defaults: on the clause line, the author's line break kept, at
-- its source offset)
group by a.Id, a.Name
order by a.Id,
  a.Name
-- after: group_by.wrap = "always", order_by.keep_line_breaks = false
group by a.Id
       , a.Name
order by a.Id, a.Name
```

### Conditions: `[where]`, `[having]`, `[join_on]`, `[case_when]`, `[if_condition]`, `[while_condition]`, `[merge_on]`, `[merge_when_condition]`, `[check_constraint]`, `[other_conditions]`

Each condition has two parts:

- `wrap` / `keep_line_breaks`: its top-level `and` / `or` chain;
- `[<condition>.groups]` `wrap` / `keep_line_breaks`: the parenthesised
  `and` / `or` groups inside it (logical blocks, R25), anywhere in the
  condition except inside nested queries and `case` expressions (which have
  their own conditions).

`merge_when_condition` is the `and …` of `when [not] matched and …`;
`other_conditions` is any `and` / `or` outside these (a select item, a
function argument such as `iif(…)`, a `set` value).

| Key | JetBrains |
|---|---|
| `where.wrap`, `having.wrap` | Queries > WHERE: `WHERE_EL_LINE`, `WRAP_INSIDE_WHERE` |
| `join_on.wrap` | Queries > FROM: `FROM_PLACE_ON`, `WRAP_INSIDE_JOIN_EXPRESSION` |
| other chains' `wrap` | Expressions: binary expressions (`ALIGN_INSIDE_BINARY_EXPRESSION`) |
| `*.keep_line_breaks` | Keep when reformatting: line breaks |
| `*.groups.wrap` | Wrapping: parenthesized expression (the common "Parenthesized expression" option; the SQL scheme has none of its own) |

The chain, broken: `where` / `having` put `and` 2 in from the keyword and
`or` 3 in (R10); `join … on` puts them under `on` (R9). The other chains
have only `never` (on one line, or as written with `keep_line_breaks`).

```sql
-- before
select * from t where a = 1 and b = 2
-- after (default where.wrap = "always": one predicate per line)
where a = 1
  and b = 2
-- after: where.wrap = "never" (or "chop_if_long": it fits)
where a = 1 and b = 2
-- before
select * from t where alpha = 1 and beta = 2 and gamma = 3 and delta = 4
-- after: margin = 50, where.wrap = "if_long"
where alpha = 1 and beta = 2 and gamma = 3
  and delta = 4
-- before
select * from t join u on u.Id = t.Id and u.Kind = 1
-- after: join_on.wrap = "always"
join u
    on u.Id = t.Id
    and u.Kind = 1
-- before
if @a = 1
  and @b = 2
-- after: if_condition.keep_line_breaks = false
if @a = 1 and @b = 2
```

The groups, broken, are blocks: `(` ends its line, one operand per line 2 in
from the `(` with `and` / `or` leading, `)` under the `(`.

```sql
-- before
select * from t where c = 3 and (alpha = 1 or beta = 2 or gamma = 3 or delta = 4)
-- after: margin = 50 (default where.groups.wrap = "chop_if_long")
where c = 3
  and (
        alpha = 1
        or beta = 2
        or gamma = 3
        or delta = 4
      )
-- after: margin = 50, where.groups.wrap = "if_long" (continuation under the first operand)
where c = 3
  and (alpha = 1 or beta = 2 or gamma = 3
       or delta = 4)
-- after: margin = 50, where.groups.wrap = "never"
where c = 3
  and (alpha = 1 or beta = 2 or gamma = 3 or delta = 4)
-- before
select * from t where (a = 1 or b = 2) and c = 3
-- after: where.groups.wrap = "always"
where (
        a = 1
        or b = 2
      )
  and c = 3
-- before
select * from t where (
  a = 1 or b = 2)
-- after (default keep_line_breaks = true: written as a block, stays one)
where (
        a = 1
        or b = 2
      )
-- after: where.groups.keep_line_breaks = false
where (a = 1 or b = 2)
```

A block can't be on one line, so a list, chain or `case` around it is
broken too.

### `[window]` (R26)

| Key | JetBrains |
|---|---|
| `wrap`, `keep_line_breaks` | none (closest: function call arguments, `WRAP_INSIDE_CALL_EXPRESSION`) |
| `partition_by.*`, `order_by.*` | as `[group_by]` / `[order_by]` |
| `frame.keep_line_breaks` | Keep when reformatting: line breaks |

Broken, the window is a block: `(` ends the line, `partition by`, `order by`
and the frame each start a line 2 in from the select item, `)` under the
item. Filled (`if_long`), a clause that doesn't fit starts a line at the
same column and `)` stays at the end. Inside a `case`, the block lines up
with its function instead of the select item. The lists after `partition
by` / `order by`, broken, continue under their first item.

```sql
-- before
select sum(a) over (partition by a order by b rows between unbounded preceding and current row) as s from t
-- after: margin = 60 (default "chop_if_long")
select sum(a) over (
         partition by a
         order by b
         rows between unbounded preceding and current row
       ) as s
from t
-- after: margin = 60, wrap = "if_long"
select sum(a) over (partition by a order by b
         rows between unbounded preceding and current row) as s
from t
-- after: margin = 60, wrap = "never"
select sum(a) over (partition by a order by b rows between unbounded preceding and current row) as s
from t
-- before
select row_number() over (partition by a, b order by c) as rn from t
-- after: wrap = "always" (STYLE.md change 2's example)
select row_number() over (
         partition by a, b
         order by c
       ) as rn
from t
-- after: wrap = "always", partition_by.wrap = "always", partition_by.comma = "trailing"
select row_number() over (
         partition by a,
                      b
         order by c
       ) as rn
from t
-- before
select row_number() over (
  partition by a order by b) as rn from t
-- after (default: written over several lines, so a block)
select row_number() over (
         partition by a
         order by b
       ) as rn
from t
-- after: keep_line_breaks = false
select row_number() over (partition by a order by b) as rn
from t
-- before
select sum(a) over (order by b rows between
    unbounded preceding and current row) as s from t
-- after: keep_line_breaks = false, frame.keep_line_breaks = false
select sum(a) over (order by b rows between unbounded preceding and current row) as s
from t
```

### `[exists]` (R12)

| Key | JetBrains |
|---|---|
| `wrap` | Subqueries (`SUBQUERY_OPENING`, `SUBQUERY_CONTENT`, `SUBQUERY_CLOSING`) |
| `lparen_on_new_line` | Subqueries: opening parenthesis (`SUBQUERY_OPENING`) |

```sql
-- before
select * from t where not exists (select 1 from u where u.Id = t.Id)
-- after (default)
where not exists
          (
              select 1
              from u
              where u.Id = t.Id
          )
-- after: lparen_on_new_line = false
where not exists (
    select 1
    from u
    where u.Id = t.Id
)
```

### `[case]` (R15)

| Key | JetBrains |
|---|---|
| `wrap` | Expressions > CASE: `NEW_LINE_BEFORE_THEN`, `NEW_LINE_BEFORE_ELSE`, `EXPR_CASE_END` |
| `keep_line_breaks` | Keep when reformatting: line breaks |
| `align_then` | Align THEN (`EXPR_CASE_THEN_ALIGN`) |

```sql
-- before
select case when a = 1 then 'one' when a = 22 then 'twenty-two' else 'other' end as k from t
-- after: wrap = "always"
select case
           when a = 1  then 'one'
           when a = 22 then 'twenty-two'
           else 'other'
       end as k
from t
-- after: wrap = "always", align_then = false
select case
           when a = 1 then 'one'
           when a = 22 then 'twenty-two'
           else 'other'
       end as k
-- before
select case when a = 1 then 'one'
  else 'other' end as k from t
-- after (default: written over several lines, so chopped)
select case
           when a = 1 then 'one'
           else 'other'
       end as k
-- after: wrap = "never", keep_line_breaks = false
select case when a = 1 then 'one' else 'other' end as k
```

### Parenthesised lists: `[in_list]`, `[function_args]`, `[insert_columns]`, `[values.row]`, `[index_columns]`, `[view_columns]`

| Section | JetBrains |
|---|---|
| `in_list` | Tuples (`CORTEGE_COMMA_1ST`) |
| `function_args` | Function call arguments (`WRAP_INSIDE_CALL_EXPRESSION`) |
| `insert_columns` | INSERT column list (tuples, `CORTEGE_COMMA_1ST`) |
| `values.row` | `WRAP_INSIDE_VALUES_EXPRESSION`, `WRAP_PARENTHESIZED_EXPRESSION_INSIDE_VALUES` |
| `index_columns` | none |
| `view_columns` | none |

All take `wrap`, `keep_line_breaks`, `comma`. Filled (`if_long`, the
default for `insert_columns` and `values.row`, R16), continuation lines start
under the first item, leading commas under the `(` (which gets a space after
it). Chopped, the list is a block, items 2 in from the `(`. For
`index_columns`, the `(` still moves to the next line when `on t (…)`
doesn't fit (R21), except under `always`.

```sql
-- before
select x from t where z in ('first value', 'second value', 'third value', 'fourth')
-- after: margin = 60, in_list.wrap = "if_long"
where z in ( 'first value', 'second value', 'third value'
           , 'fourth')
-- after: margin = 60, in_list.wrap = "chop_if_long", in_list.comma = "trailing"
where z in (
             'first value',
             'second value',
             'third value',
             'fourth'
           )
-- before
select coalesce(a, b) as c from t
-- after: function_args.wrap = "always"
select coalesce(
                 a
               , b
               ) as c
-- before
insert into dbo.Widget (Id, Name) values (1, 'one')
-- after: insert_columns.wrap = "always", insert_columns.comma = "trailing"
insert into dbo.Widget (
                         Id,
                         Name
                       )
values (1, 'one')
```

### `[values]` (R17)

| Key | JetBrains |
|---|---|
| `wrap`, `keep_line_breaks` | `WRAP_VALUES_EXPRESSION` |
| `comma` | `CORTEGE_COMMA_1ST` |
| `row` | see parenthesised lists |

The rows of `values`; with one row there's nothing to wrap.

```sql
-- before
insert into dbo.Widget (Id, Name) values (1, 'one'), (2, 'two')
-- after (default "always")
values (1, 'one')
     , (2, 'two')
-- after: wrap = "chop_if_long" (fits), or "never"
values (1, 'one'), (2, 'two')
-- after: margin = 60, wrap = "chop_if_long", comma = "trailing", with four rows
values (1, 'first'),
       (2, 'second'),
       (3, 'third'),
       (4, 'fourth')
```

### `[exec_args]`

`exec procedure a, b, c` (not `exec ('…')`). JetBrains: function call
arguments (`WRAP_INSIDE_CALL_EXPRESSION`). Broken, arguments start lines at
the first argument's column.

```sql
-- before
exec dbo.DoThings @alpha = 1, @beta = 'two', @gamma = @c
-- after: exec_args.wrap = "always"
exec dbo.DoThings @alpha = 1
                , @beta = 'two'
                , @gamma = @c
```

### `[subquery]`, `[derived_table]`, `[cte]`, `[cte_list]` (R9, R12, R13)

| Key | JetBrains |
|---|---|
| `subquery.wrap`, `cte.wrap` | Subqueries (`SUBQUERY_OPENING`, `SUBQUERY_CONTENT`, `SUBQUERY_CLOSING`) |
| `derived_table.lparen_on_new_line`, `cte.lparen_on_new_line` | `SUBQUERY_OPENING` |
| `cte_list.comma` | `QUERY_EL_COMMA` (WITH: `WITH_ALIGN_AS`) |

A query in parentheses is always on lines of its own (`wrap = "always"`).
With `lparen_on_new_line = false` the `(` ends the join / `as` line, the
query is one indent in from that line and `)` is at its indent.

```sql
-- before
select * from t outer apply (select top 1 x.Id from dbo.X x) lastX
-- after (default)
outer apply
(
    select top 1 x.Id
    from dbo.X x
) lastX
-- after: derived_table.lparen_on_new_line = false
outer apply (
    select top 1 x.Id
    from dbo.X x
) lastX
-- before
with A as (select 1 as x), B as (select 2 as y) select * from A join B on 1 = 1
-- after (default)
with A as
    (
        select 1 as x
    )
   , B as
    (
        select 2 as y
    )
select *
-- after: cte.lparen_on_new_line = false, cte_list.comma = "trailing"
with A as (
    select 1 as x
),
     B as (
         select 2 as y
     )
select *
```

### `[merge_when]` (R19)

`action_on_new_line`: the action (`update`, `insert`, `delete`) on its own
line one indent in from `merge`, or on the `when … then` line. JetBrains:
none.

```sql
-- before
merge dbo.Target t using dbo.Source s on t.Id = s.Id
when matched then update set a = s.a
when not matched by source then delete;
-- after (default)
when matched then
    update
    set a = s.a
when not matched by source then
    delete;
-- after: action_on_new_line = false
when matched then update
    set a = s.a
when not matched by source then delete;
```

### `[create_table]`, `[foreign_key]` (R20)

| Key | JetBrains |
|---|---|
| `create_table.wrap` | `WRAP_INSIDE_CREATE_TABLE`, `TABLE_OPENING`, `TABLE_CLOSING` |
| `create_table.comma` | Place comma (`QUERY_EL_COMMA`) |
| `create_table.align_columns` | Align types in CREATE (`ALIGN_TYPE_IN_CREATE_STATEMENT`) |
| `create_table.named_constraint_on_new_line` | Constraints wrapping (`CONSTRAINT_WRAP_1`, `CONSTRAINT_WRAP_3`) |
| `foreign_key.references_on_new_line` | Constraints wrapping (`CONSTRAINT_WRAP_*`) |

Also table types and table variables. `false` for the two `*_on_new_line`
keys means "where the author put it".

```sql
-- before
create table dbo.Widget
(
    Id int not null,
    Code char(3) constraint DF_Widget_Code default 'abc' not null,
    constraint FK_Widget_Customer foreign key (Id) references dbo.Customers (Id)
)
-- after (default)
create table dbo.Widget
(
    Id   int                                    not null,
    Code char(3)
        constraint DF_Widget_Code default 'abc' not null,
    constraint FK_Widget_Customer foreign key (Id)
        references dbo.Customers (Id)
)
-- after: comma = "leading", align_columns = false,
--        named_constraint_on_new_line = false, foreign_key.references_on_new_line = false
create table dbo.Widget
(
    Id int not null
  , Code char(3) constraint DF_Widget_Code default 'abc' not null
  , constraint FK_Widget_Customer foreign key (Id) references dbo.Customers (Id)
)
```

### `[alter_table]` (R21)

`wrap` / `keep_line_breaks` for the operations after the first (which
always starts a line one indent in). JetBrains: `TABLE_ALTER_ITEM_WRAP`. The
parser gives several operations only for `add …, add …` (T-SQL's
`add a, b` doesn't parse yet).

```sql
-- before
alter table dbo.W add constraint A unique (x), add constraint B unique (y)
-- after (default)
alter table dbo.W
    add constraint A unique (x), add constraint B unique (y)
-- after: wrap = "always"
alter table dbo.W
    add constraint A unique (x),
    add constraint B unique (y)
```

### `[routine_params]` (R22)

| Key | JetBrains |
|---|---|
| `wrap` | `ROUTINE_ARG_WRAP`, `ROUTINE_ARG_OPENING`, `ROUTINE_ARG_CONTENT`, `ROUTINE_ARG_CLOSING` |
| `comma` | `ROUTINE_ARG_COMMA` |
| `align_types` | `ROUTINE_ARG_ALIGN_TYPES` |

```sql
-- before
create procedure dbo.GetWidgets (@minPrice decimal(10, 2), @name nvarchar(100) = null) as
-- after (default)
create procedure dbo.GetWidgets
(
  @minPrice decimal(10, 2)
, @name     nvarchar(100) = null
) as
-- after: comma = "trailing", align_types = false
create procedure dbo.GetWidgets
(
  @minPrice decimal(10, 2),
  @name nvarchar(100) = null
) as
```
