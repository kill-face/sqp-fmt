select [Name], Status, Value, count(*) as Cnt, sum(o.Total) Total, getdate() as Now, @@rowcount as N
from dbo.Orders o
where o.Status is not null
    and dateadd(day, -1, o.CreatedAt) < sysutcdatetime()
group by [Name], Status, Value
declare @x nvarchar(max) = convert(varchar(10), 5), @y dbo.MyType, @z sysname
select dbo.fnCustom(1), cast(1 as decimal(10, 2)), next value for dbo.seqOrder
