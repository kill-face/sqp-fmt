select * from t where a = 1 and b = 2 or c = 3
select * from t where a = 1
select a, count(*) from t group by a having count(*) > 1 and max(b) < 5
select * from t where a between 1 and 10 and b like 'x%' and c in (1, 2, 3) and d is null and not e = 1
