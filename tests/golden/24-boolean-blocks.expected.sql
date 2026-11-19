select *
from dbo.Event e
where e.TypeCode = 6005
  and e.IsShipped = 'L'
  and (
        (e.TimestampUtc >= @start and e.TimestampUtc < @end)
        or (e.Other = 1 and e.Other2 = 2)
      )
select *
from t
where (a = 1 or b = 2)
  and c = 3
select *
from t
where (a_rather_long_column_name = 1 or another_rather_long_column_name = 2 or third_column_name = 3)
  and c = 3
select *
from t
where c = 3
  and (
        a_rather_long_column_name = 1
        or another_rather_long_column_name = 2
        or third_column_name = 3
        or fourth_column_name = 4
      )
select *
from t
where (
        a = 1
        or b = 2
      )
