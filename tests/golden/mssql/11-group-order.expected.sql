select a, b, count(*)
from t
group by a, b
order by a desc, b
select a
from t
order by a,
    b,
    c
