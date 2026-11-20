select sum(x.Amount) over (partition by x.CustomerIdentifier, x.RegionIdentifier, x.ProductCategoryIdentifier order by x.CreatedAt, x.Id) as RunningTotal from x
select sum(x.Amount) over (partition by x.A, x.B order by x.C) as Total from x
