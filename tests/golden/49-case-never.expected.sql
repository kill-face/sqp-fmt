select case when SomeRatherLongColumnName = 1 then 'first value' when AnotherRatherLongColumnName = 2 then 'second value' else 'otherwise' end as Kind
from t
select case
           when a = 1 then 'one'
           when a = 22 then 'twenty-two'
           else 'other'
       end as Kind
from t
