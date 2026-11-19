declare @a int = 1
declare @b nvarchar(20) = 'x', @c int
set @a = @a + 1
select @a = count(*)
from dbo.Widget
exec dbo.DoThing @id = @a, @name = N'x'
