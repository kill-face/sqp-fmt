if @a > 0 begin print 'a' end else if @a < 0 begin print 'b' end else print 'c'
while @i < 10
begin
set @i = @i + 1
if @i = 5 break
end
if exists (select 1 from dbo.Orders) print 'some'
