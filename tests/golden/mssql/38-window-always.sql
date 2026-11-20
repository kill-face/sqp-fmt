select row_number() over (partition by col_a, col_b, col_c order by col_x) as rn
     , w.Code
from dbo.Widget w
select rank() over (order by w.FirstSortColumnWithALongName desc, w.SecondSortColumnWithALongName, w.ThirdSortColumnAlsoWithAVeryLongName) as r
from dbo.Widget w
select count(*) over () as n from t
