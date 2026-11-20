select x
from t
where y in (
             1
           , 2
           )
select concat( FirstPartOfTheName, SecondPartOfTheName
             , ThirdPartOfTheName) as c
from t
declare @n NVarChar(10) = cast(1 as Int)
exec dbo.DoThings @alpha = 1, @beta = 'two'
go
create index IX_W
    on dbo.W (A, B)
go
create view dbo.vW (
                     A
                   , B
                   ) as
    select A, B
    from dbo.W
