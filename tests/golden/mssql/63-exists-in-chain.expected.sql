select a
from t
where x = 1
    and exists
        (
            select 1
            from u
            where u.id = t.id
        )
     or not exists
         (
             select 1
             from v
         )
