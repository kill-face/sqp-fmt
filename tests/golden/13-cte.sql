with recent as (select CustomerId, max(CreatedAt) as LastOrder from dbo.Orders group by CustomerId) select * from recent
;with a as (select 1 as x), b as (select x from a) select * from b
