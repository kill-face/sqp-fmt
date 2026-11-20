select a
     , row_number() over (
         partition by a
         order by b
       ) as rn
from t
select a
     , case
           when x = 1 then sum(a) over (
                             partition by a
                             order by b
                           )
       end as y
from t
select case
           when (
                  a = 1
                  or b = 2
                ) then 1
       end as z
from t
