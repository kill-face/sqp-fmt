with Recent as
    (
        select o.OrderId, o.CustomerId
        from dbo.Orders o
        inner join dbo.Customers c
          on o.CustomerId = c.CustomerId
          and o.RegionId = c.RegionId
        where o.Status = 'S'
            and o.Total > 100
             or o.Priority = 1
    )
select *
from Recent
