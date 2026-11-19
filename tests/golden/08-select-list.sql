select a, b, c from t
select a.FirstColumn, a.SecondColumn, a.ThirdColumn, a.FourthColumn, a.FifthColumn, a.SixthColumn, a.Seventh from dbo.Alpha a
select a.Id,
  a.Name as CustomerName,
  a.CreatedAt as Created, a.Total TotalAmount
from dbo.Alpha a
select distinct top 10 a.Id
, a.Name as N
from dbo.Alpha a
select
  -- the key
  a.Id
, a.Name
from dbo.Alpha a
select @total = sum(o.Total), Label = 'x' from dbo.Orders o
