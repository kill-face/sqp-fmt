select x.Id
     , sum(x.Amount) over (partition by x.CustomerId, x.RegionId, x.ProductCategoryId order by x.CreatedAt
           rows between unbounded preceding and current row) as RunningTotal
from x
select row_number() over (partition by a order by b) as rn
from t
select row_number() over (
           partition by a
           order by b
       ) as rn
from t
