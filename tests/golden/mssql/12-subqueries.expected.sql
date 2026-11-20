select *
from dbo.Orders
where CustomerId in (
                        select Id
                        from dbo.Customers
                        where IsActive = 1
                    )
select *
from dbo.Orders o
where not exists
          (
              select 1
              from dbo.Returns r
              where r.OrderId = o.Id
          )
select (
           select max(Total)
           from dbo.Orders
       ) as MaxTotal
select *
from (
         select Id, Total
         from dbo.Orders
     ) as t
where t.Total > 5
