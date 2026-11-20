merge dbo.Target as t
using dbo.Source as s
on (t.Id = s.Id)
when matched and t.Name <> s.Name then
    update
    set t.Name       = s.Name
      , t.ModifiedAt = sysutcdatetime()
when not matched by target then
    insert (Id, Name)
    values (s.Id, s.Name)
when not matched by source then
    delete;
