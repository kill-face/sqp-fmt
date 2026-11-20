select * from t where alpha_column = 1 and beta_column = 2 or gamma_column = 3 and delta_column = 4 and epsilon = 5
select * from t where c = 3 and (alpha_column = 1 or beta_column = 2 or gamma_column = 3 or delta_column = 4)
select * from t where a = 1
  and b = 2
select * from t where (
  a = 1 or b = 2)
select k from t group by k having count(*) > 1
  and max(v) > 2 and (min(v) < 0 or min(v) > 100 or avg(v) between 10 and 20 or sum(v) = 0)
select * from t join u on u.Id = t.Id and u.Kind = 1 and u.Other = t.Other and u.Fourth = t.Fourth and u.Fifth = 5
