# Target style

The baseline is JetBrains' SQL formatter (Rider / DataGrip) with the code style
in `style/jetbrains-codestyle.xml`, as applied to T-SQL (its
`MSSQLCodeStyleSettings` block). `style/datagrip-reference.sql` is DataGrip's
output for `style/datagrip-reference.input.sql` with that scheme.

On top of that baseline there are two deliberate changes, both about making a
multi-line construct read as one indented block.

The formatter only changes whitespace and keyword/function/type case. It never
adds, removes or reorders tokens: no `as` inserted, no `inner` / `outer` added,
no `;` added, comments kept where they are.

## Baseline highlights

- Keywords, built-in functions and types lowercase; identifiers as written.
- 4-space indent, 120-column right margin.
- Clauses (`from`, `join`, `where`, `group by`, `having`, `order by`) start a
  line; a short clause body stays on the clause's line
  (`select a, b, c`, `group by o.Id, c.[Name]`).
- `join` at the clause level, its `on` on the next line one indent in (`join_on.indent`).
- `where` predicates: one per line, `and` / `or` indented 2 under `where`.
- Leading commas when a list is chopped (`set`, `values`, procedure
  parameters, long select lists).
- `create table` column definitions aligned in columns.

## Change 1: logical blocks in boolean expressions

A parenthesised group inside a long boolean expression is laid out as a block:
`(` ends its line, the contents are indented, and `)` sits on its own line.

```sql
select *
from dbo.ShipmentEvents shpEvt
where shpEvt.EventKindCode = 6005
  and shpEvt.IsShipped = 'L'
  and fnInZone.Result = 'true'
  and (
        (
          shpEvt.RecordedAtUtcTime >= @recordedAtUtcTimeStart
          and shpEvt.RecordedAtUtcTime < @recordedAtUtcTimeEnd
        )
        or (
            other condition
            and other condition
        )
      )
```

The author decides what is a block: a group written with a line break after
its `(` is kept as a block; a group written on one line stays inline unless it
no longer fits in the margin, in which case it becomes a block.

**Indentation:** a block's contents go at the column of its `(` + 2 and its
`)` sits under the `(`, the same everywhere (including `and (` / `or (`).
The hand-written example above drifts from that in the `or (` group; the rule
wins:

```sql
  and (
        (
          shpEvt.RecordedAtUtcTime >= @recordedAtUtcTimeStart
          and shpEvt.RecordedAtUtcTime < @recordedAtUtcTimeEnd
        )
        or (
             other condition
             and other condition
           )
      )
```

## Change 2: window clauses as blocks

A long `over (...)` is laid out as a block: `(` ends the line, each of
`partition by`, `order by` and the frame clause starts its own line indented
2 past the start of the select item, and `)` lines up with the start of the
item.

```sql
select row_number() over (
         partition by col_a, col_b, col_c
         order by col_x
       ) as rn
     , new.Code
     , new.Name
     , new.ShortName
     , new.RegionId
     , new.IsVerified
     , new.ImportBatchId
from NewCarrier new
```

## Test bed

The SQL in a private T-SQL corpus (`corpus/private/`, about 510 files) is mostly in this
style already. It is used read-only, pointed at by path: never copy those
files into this repo. Formatting a file from it should change nothing except
where the two changes above apply or the file was simply out of style.

## Formatter control

- `-- @formatter:off` / `-- @formatter:on` (JetBrains' markers, already used
  in the test bed) leave the lines between them exactly as written.
- A batch that doesn't parse is left exactly as written; the rest of the
  file is still formatted. Placeholder templates (`values ();`, `check ()`)
  stay usable.

## Configuration

Each construct's layout can be changed in a `sqp-fmt.toml` (found by
searching upward from the formatted file's directory; `--config FILE`,
`--stdin-filepath PATH` for stdin). [docs/config.md](docs/config.md) is the
reference: every key, its values, its JetBrains counterpart, examples, and
what's implemented. Without the file, sqp-fmt formats as described here.

The vocabulary is JetBrains':

| TOML value | JetBrains wrapping option |
|---|---|
| `wrap = "never"` | Do not wrap |
| `wrap = "if_long"` | Wrap if long |
| `wrap = "chop_if_long"` | Chop down if long |
| `wrap = "always"` | Wrap always (Chop always) |
| `keep_line_breaks = true` | Keep when reformatting: line breaks |
| `comma = "leading"` / `"trailing"` | Place comma: before / after element (`QUERY_EL_COMMA`, `ROUTINE_ARG_COMMA`) |
| `lparen_on_new_line` | Opening parenthesis on a new line (`SUBQUERY_OPENING`) |

For a parenthesised construct, "chopped" is the block layout of changes 1
and 2 (`(` ends its line, contents one indent (4) in from the `(` column, `)` on its own
line under the `(`): it stands for JetBrains' separate "new line after `(`",
"place `)` on new line" and "align when multiline" options. Keeping line
breaks is how blocks already work: the author's line break after `(` (for a
window, anywhere in it) makes the block. The defaults are the rules below,
R1 to R27.

## Rules

What `sqp-fmt` implements, rule by rule. Each has a golden test in
`tests/golden/` (input `NN-name.sql`, output `NN-name.expected.sql`). Where a
rule comes from the test bed rather than the JetBrains scheme, the counts are
from `corpus/private/` (house style); "keeps" means the author's line break is
kept, and anything not covered here keeps its source layout, re-indented with
its surroundings.

Formatting is per `GO` batch. Batches that don't parse, `@formatter:off`
regions, `GO` lines and SQLCMD lines (`:r`, `:setvar`) come out exactly as
written; so does any batch whose output wouldn't tokenize back to the same
tokens (the safety net). Line endings (LF / CRLF), a BOM and a missing final
newline are kept.

### R1 Case (`01-case`)

Keywords, built-in functions, system types and `@@` functions lower-case.
Identifiers exactly as written, decided by the AST (a word that starts an
identifier node is one), so `Status`, `Name`, `Value` keep their case; so do
user-defined functions and types (`dbo.fnCustom`, `dbo.MyType`). Date parts
in `dateadd`-style functions count as keywords.

```sql
select [Name], Status, count(*) as Cnt, getdate() as Now, @@rowcount as N
declare @x nvarchar(max) = convert(varchar(10), 5), @y dbo.MyType, @z sysname
```

### R2 Spacing (`02-spacing`)

One space between tokens; none before `,` `;` `)` `.` or after `(` `.`;
binary operators spaced (`a = b`, `c + d`), unary ones not (`-1`); no space
between a function and its `(` or in a type (`decimal(10, 2)`); a space
before `(` after `in`, `exists`, `over`, a table name or `identity`.
`identity (1,1)` keeps its inside as written (the test bed's 8 of 8).

### R3 Statements (`03-statements`)

Each statement starts a line, at the indent of the block it's in; `;` stays
on the statement's last line; `;with` stays together.

### R4 Blank lines (`04-blank-lines`)

A blank line the author wrote between two lines is kept; runs of blank lines
become one. No blank lines are added.

### R5 Comments (`05-comments`)

Comments never move relative to the tokens around them. A trailing comment
stays on its line, as far from the code before it as in the source (so
aligned comments stay aligned). A comment on its own line keeps its offset
from the line after it. A comment right after a comma keeps the comma at the
end of its line (a chopped list can't move that comma to the next line).

```sql
select a,       -- trailing
       b        -- another
from t -- after from
```

### R6 Blocks (`06-blocks`)

`begin … end`, `begin try … end try`, `begin catch … end catch`: contents
one indent (4) in, `end` under `begin`.

### R7 if / else / while (`07-if-while`)

Condition on the `if` line; the body on the next line one indent in; a
`begin … end` body is itself indented under `if` (test bed: 35 of 35), its
contents one more. `else` under `if`; `else if` stays on one line.

```sql
if @a > 0
    begin
        print 'a'
    end
else
    print 'b'
```

### R8 Select list (`08-select-list`)

On the `select` line when the author wrote it on one line and it fits in 120
columns; otherwise chopped, one item per line with leading commas 2 left of
the first item, and aliases (`as x`, or a bare alias) aligned. A comment
before the first item puts it on its own line 4 in from `select`, the commas
staying where they'd be (test bed: 4 of 4).

```sql
select a.Id
     , a.Name      as CustomerName
     , a.CreatedAt as Created
from dbo.Alpha a
select distinct top 10 a.Id
                     , a.Name as N
```

### R9 from / join (`09-from-join`)

Each clause keyword (`from`, `where`, `group by`, `having`, `order by`,
`into`, `option`) starts a line at the `select` column; each join too, with
its `on` on the next line one indent in (`join_on.indent`). `and` / `or` in an `on` condition keep the
author's line breaks, starting the line under `on` (test bed: 28 of 28). A
derived table after `join` / `apply` has its `(` on its own line under the
join (19 of 23).

```sql
from dbo.Orders o
join dbo.Lines l
    on l.OrderId = o.Id
    and l.Qty > 0
outer apply
(
    select top 1 x.Id
    from dbo.X x
) lastX
```

### R10 where / having (`10-where`)

One predicate per line: `and` one indent (4) in from `where` (`where.indent`),
`or` one more. (The test bed uses `and` +2 and `or` +3, so predicates line up
under `where`; `where.indent = 2` gives that.)

```sql
where a = 1
    and b = 2
     or c = 3
```

### R11 group by / order by (`11-group-order`)

On the clause line; the author's line breaks in the list are kept.

### R12 Subqueries (`12-subqueries`)

The query starts on the line after `(`, 4 in from the `(`; `)` under the
`(`. The `(` of an `exists` subquery goes on its own line (test bed: 108 of
108): 4 in from `if` for `if exists`, from the line its `and` / `or` starts
for `and exists` in a `where` chain, else from `[not] exists`. Other subqueries keep their `(`
where it was written (`in (`, `= (`, `from (`).

```sql
where not exists
          (
              select 1
              from dbo.Returns r
          )
```

### R13 CTEs (`13-cte`)

`with A as`, then `(` on its own line, the query 4 in from `(`, `)` under
`(`; later CTEs start `   , B as` so the names line up; the main query at
the `with` column. The `(` is 4 in from `with`, however many CTEs there are.
(DataGrip, and most of the test bed, put a single CTE's `(` 4 in from its
name instead; that indents the whole query by the name's length, so it was
dropped.) `cte.lparen_on_new_line = false` puts `(` on the `as` line.

### R14 Set operations (`14-union`)

`union [all]`, `except`, `intersect` on their own line at the query column.

### R15 case (`15-case`)

On one line if the author wrote it so and it fits; otherwise `when` / `else`
one per line 4 in from `case`, `then` aligned, `end` under `case`. A `then`
the author put on its own line stays there while its condition takes more
than one line.

```sql
select case
           when a = 1  then 'one'
           when a = 22 then 'twenty-two'
           else 'other'
       end as Kind
```

### R16 insert (`16-insert`)

The column list fills the line; when it wraps, `( ` opens it and leading
commas go under the `(`; the source query or `values` starts the next line
at the `insert` column. `insert` / `into` / `(` keep the author's line
breaks.

```sql
insert into dbo.OrderArchive ( OrderId, CustomerId, Total, CreatedAt, CreatedBy, ModifiedAt, ModifiedBy, ChangeReason
                             , Source)
select …
```

### R17 values (`16-insert`)

Rows one per line with leading commas (2 left of the first row); a row
that wraps is laid out like a column list.

### R18 update / delete (`17-update-delete`)

`set`, `from`, `where`, `output` at the `update` / `delete` column.
Several assignments: one per line with leading commas, `=` aligned.

```sql
update dbo.Orders
set Status   = 'closed'
  , ClosedAt = getdate()
where Id = @id
```

### R19 merge (`18-merge`)

`using`, `on` and each `when` at the `merge` column (test bed: `on` at the
column 40 of 70); the action (`update`, `insert`, `values`, `delete`) and
`set` 4 in (73 of 74).

### R20 create table, table types, table variables (`19-create-table`)

`(` and `)` on their own lines at the `create` (`declare`) column, one
column or constraint per line 4 in. Names, types and nullability aligned
in columns (a computed column's `as` in the type column). A named column
constraint on its own line 4 in from the column (288 of 298). A foreign
key's `references` on its own line 4 in from the line with `foreign key`.

```sql
create table dbo.Widget
(
    Id    int                                   not null identity (1,1),
    Name  nvarchar(100)                         not null,
    Code  char(3)
        constraint DF_Widget_Code default 'abc' not null,
    constraint FK_Widget_Customer foreign key (Id)
        references dbo.Customers (Id)
)
```

### R21 create index, alter table (`20-index-alter`)

`on` 4 in; `include` / `where` / `with` where the author put them, 4 in
when on their own line (`include` on the `on` line 8 of 11). `alter table`
operations 4 in.

### R22 Procedures and functions (`21-procedure`, `22-function-trigger-view`)

A parenthesised parameter list: `(` and `)` on their own lines, parameters one
indent in (`routine_params.indent`) with leading commas 2 left of them, types
aligned. `as` where the
author put it (`) as` or its own line); `begin` / `end` at the `create`
column. `returns` starts a line.

```sql
create procedure dbo.GetWidgets
(
  @minPrice decimal(10, 2)
, @name     nvarchar(100) = null
) as
begin
    set nocount on;
end
```

### R23 Triggers and views (`22-function-trigger-view`)

Trigger: `on`, `after …` and `as` each on a line 4 in, `begin` at the
`create` column. View: the query on the next line 4 in.

### R24 declare / set / exec (`23-declare-set-exec`)

`=` spaced, including named arguments (`@id = @a`; 919 of 934).

### R25 Logical blocks (`24-boolean-blocks`, STYLE.md change 1)

As in "Change 1" above: a parenthesised `and` / `or` group is a block when
the author put a line break after `(`, or it doesn't fit; contents one indent in from
`(` (`<condition>.groups.indent`), one operand per line with `and` / `or` leading, `)` under `(`.
A block can't be on one line, so a list, chain or `case` around it is broken
too (`29-window-line-break`).

### R26 Window blocks (`25-window-blocks`, `29-window-line-break`, STYLE.md change 2)

As in "Change 2" above, for an `over (…)` the author wrote over several
lines or that doesn't fit; a short one written on one line stays there. A
window written over several lines is a block wherever it is: a select list,
`case` or other group around it can't stay on one line then, so it is broken
too (`29-window-line-break`; before, a window inside a one-line select list
or `case` stayed inline). Inside a `case`, the block lines up with its
function rather than the select item. `[window] wrap = "always"` makes every
window a block, as in the example above.

### R27 Formatter control (`26-formatter-off`, `27-unparsed`)

`@formatter:off` … `@formatter:on` lines (both included) are verbatim, also
inside a statement or across `GO`; the code after them continues at the
indent it would have.

### Open questions

Places where the test bed and the JetBrains scheme (or STYLE.md's own
baseline text) disagree, or the test bed doesn't settle it; the rule above
is what's implemented.

- `or` under `where` is 3 in (test bed), not 2 (baseline text above).
- A CTE's `(` used to depend on how many CTEs there are (DataGrip's output,
  followed by the test bed); since 2026-11-20 it's always 4 in from `with` (R13).
- A select's first item after a comment goes 4 in from `select`, left of the
  other items (test bed, 4 of 4).
- "A long `over (…)`" is read as "doesn't fit, or written over several
  lines" (`[window] wrap = "chop_if_long"`, the default, which keeps
  `25-window-blocks` and the test bed as they are). The example in "Change 2"
  turns a short `over (partition by a, b, c order by x)` written on one line
  into a block, which is `[window] wrap = "always"`.
- The JetBrains scheme says `ALIGN_AS_IN_SELECT_STATEMENT = false`,
  `ALIGN_EQ_INSIDE_SET_CLAUSE = false`, `ALIGN_TYPE_IN_CREATE_STATEMENT =
  false` and `ALIGN_TYPE_IN_ARGUMENT_DEFINITION = false`, but R8 aligns
  aliases, R18 aligns `=` in `set`, R20 aligns column types and R22
  parameter types (`ROUTINE_ARG_ALIGN_TYPES = true`; DataGrip's reference
  output aligns the table's types too, but not `set`'s `=`). sqp-fmt follows
  the test bed; `select_list.align_aliases`, `create_table.align_columns`
  and `routine_params.align_types` switch it off (`set` has no key yet).
- JetBrains keeps each line break the author wrote; sqp-fmt's
  `keep_line_breaks` instead makes a group with a line break after its `(`
  (a window with any line break) a whole block, and chops a whole
  `chop_if_long` list with a line break in it.
- Subquery contents go 4 in from their `(`, as do boolean groups (change 1
  was 2 in until 2026-11-20).
- Change 1 also reshapes the seed scripts' `when matched and (` predicates
  (written with `(` ending the line, contents 4 in from `merge` and trailing
  `or`): contents now go one indent in from the `(`, far right of `merge`. If blocks
  whose `(` ends a line should indent from that line instead, that's a
  one-line change in `nested()` (`src/build/expr.rs`).
- `in (subquery)` keeps `(` on the `in` line (the reference output; the test
  bed has 2 cases, one each way).
- `merge … on` sits at the `merge` column (40) though 27 statements indent
  it 4.
