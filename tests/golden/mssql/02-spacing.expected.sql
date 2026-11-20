select a.b, c + d, e * f - g / h, x % 2, len('x'), coalesce(a, b, c)
from t
where a = b
    and c <> d
    and e >= f
    and g != h
select -1, +2, ~3, not 1 = 1, a.b.c, [a b].[c]
