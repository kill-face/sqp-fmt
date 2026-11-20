select x
from t
where z in ('first value', 'second value')
select concat(
                 FirstPartOfTheName
             , SecondPartOfTheName
             , ThirdPart
             ) as c
from t
exec dbo.DoThings @alpha = 1,
                  @beta = 'two',
                  @gamma = @c,
                  @delta = 'four'
exec dbo.DoThings @alpha = 1
select a, b
from t
order by a
       , b
