create procedure dbo.GetWidgets(@minPrice decimal(10,2), @name nvarchar(100) = null, @limit int = 10) as
begin
set nocount on;
select top (@limit) * from dbo.Widget where Price >= @minPrice
end
