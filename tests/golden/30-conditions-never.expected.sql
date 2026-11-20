select *
from t
where c = 3 and (a_rather_long_column_name = 1 or another_rather_long_column_name = 2 or third_column_name = 3)
select *
from t
where (a = 1 or b = 2) and c = 3
select k, count(*)
from t
group by k
having count(*) > 1
  and (
        max(v) > 2
        or min(v) < 0
      )
select *
from t
join u
    on u.Id = t.Id and u.Kind = 1
