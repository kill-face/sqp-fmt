select *
from dbo.Orders o
inner join dbo.Customers c
    on c.Id = o.CustomerId
left outer join dbo.Regions r
    on r.Id = c.RegionId and r.IsActive = 1
cross join dbo.Calendar cal
select *
from dbo.Orders o
join dbo.Lines l
    on l.OrderId = o.Id
    and l.Qty > 0
cross apply dbo.fnSplit(o.Tags) s
outer apply
(
    select top 1 x.Id
    from dbo.X x
    where x.OrderId = o.Id
) lastX
select *
from dbo.A a, dbo.B b
where a.Id = b.Id
