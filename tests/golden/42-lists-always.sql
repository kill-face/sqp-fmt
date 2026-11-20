select a.Id, a.Name as CustomerName, a.CreatedAt as Created from dbo.Alpha a
group by a.Id,
  a.Name
order by a.Id,
  a.Name
select
    -- the key
    a.Id, a.Name from dbo.Alpha a
