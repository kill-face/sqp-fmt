select * from t where a = 1 and b = 2
select * from t where alpha_column = 1 and beta_column = 2 or gamma_column = 3 and delta_column = 4
select * from t where (a = 1 or b = 2) and c = 3
select k from t group by k having count(*) > 1
  and max(v) > 2
select k from t group by k having (
  count(*) > 1 or max(v) > 2)
select * from t join u on u.Id = t.Id and u.Kind = 1
select * from t join u on u.Id = t.Id and u.Kind = 1 and u.Other = t.Other and u.Fourth = t.Fourth and u.Fifth = 5
