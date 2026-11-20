select sum(x.Amount) over (
  partition by x.CustomerIdentifier, x.RegionIdentifier, x.ProductCategoryIdentifier, x.Channel
  order by x.CreatedAt,
    x.Id) as RunningTotal from x
