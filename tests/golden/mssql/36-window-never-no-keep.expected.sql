select row_number() over (partition by a, b order by c) as rn
from t
select sum(a) over (order by b rows between unbounded preceding and current row) as s
from t
