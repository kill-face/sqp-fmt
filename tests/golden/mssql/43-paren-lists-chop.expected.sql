select x
from t
where y in (1, 2, 3)
    and z in (
                 'first value',
                 'second value',
                 'third value',
                 'fourth value'
             )
select coalesce(
                   a
                 , b
               ) as c
from t
exec dbo.DoThings @alpha = 1
                , @beta = 'two'
                , @gamma = @c
