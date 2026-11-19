-- header comment
/* block
   comment */
select a,       -- trailing, kept at its column
       b        -- another
from t -- after from
-- own-line comment before where
where a = 1 /* inline */
  and b = 2
select x
from y -- last
