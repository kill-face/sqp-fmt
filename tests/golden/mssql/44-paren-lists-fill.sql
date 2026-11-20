select x from t where z in ('first value', 'second value', 'third value', 'fourth')
select coalesce(a,
  b, c) as c from t
exec dbo.DoThings @alpha = 1, @beta = 'two', @gamma = @c, @delta = 'four', @epsilon = 5
