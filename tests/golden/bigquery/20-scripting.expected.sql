-- Procedural code outside T-SQL keeps its source layout; the SQL inside it
-- is formatted.
begin
  declare n int64 default 1;
  if n > 0 then
    select id, title
    from books
    where shelf_id = n;
  end if;
exception when ERROR then
  select 1;
end;
