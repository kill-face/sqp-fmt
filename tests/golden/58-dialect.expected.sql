select a::int, b
from t
where x ilike 'y%'
  and (c = 1 or d = 2)
