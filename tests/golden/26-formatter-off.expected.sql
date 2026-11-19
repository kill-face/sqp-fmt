select 1 as a
-- @formatter:off
select   2   as   b,
         3   as   c
-- @formatter:on
select 4 as d
create table #T
(
    Id   int,
    --@formatter:off
    Calc  as   (Id   +   1),
    --@formatter:on
    Name nvarchar(10)
)
