declare @x nvarchar(max) = convert(varchar(10), 5), @y dbo.MyType, @t table (Id int not null)
select [Name], Status, Count(*) as Cnt, getdate() as Now, @@rowcount as N, cast(a as char(3)) as c
from dbo.Widget w join dbo.Owner o on o.Id = w.OwnerId where exists (select 1 from dbo.X x where x.Id = w.Id)
for xml path
if @x is null
begin
    select case when a = 1 then 'one' when a = 22 then 'twenty-two' else 'other' end as Kind from t
end
go
create table dbo.Widget
(
    Id int not null identity (1,1),
    Code char(3)
        constraint DF_Widget_Code default 'abc' not null
)
