begin
select 1
begin
select 2
end
end
begin try
select 1 / 0
end try
begin catch
-- handle it
throw;
end catch
