select case when a = 1 then 'one' else 'other' end as Kind
from t
select case
           when a = 1  then 'one'
           when a = 22 then 'twenty-two'
           else 'other'
       end as Kind
     , case b when 1 then 'x' end
from t
