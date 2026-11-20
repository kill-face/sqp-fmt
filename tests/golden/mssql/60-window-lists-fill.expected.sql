select sum(x.Amount) over (
         order by x.CustomerIdentifier, x.RegionIdentifier,
                  x.ProductCategoryIdentifier, x.Id
       ) as RunningTotal
from x
