with A as (select 1 as x), B as (select 2 as y) select * from A join B on 1 = 1
with OnlyOne as (select 1 as x) select * from OnlyOne
select * from t where not exists (select 1 from u where u.Id = t.Id) and exists (select 1 from v)
if exists (select 1 from t) print 'x'
select * from t outer apply (select top 1 x.Id from dbo.X x where x.TId = t.Id) lastX
merge dbo.Target t using dbo.Source s on t.Id = s.Id
when matched then update set a = s.a
when not matched then insert (Id, a) values (s.Id, s.a)
when not matched by source then delete;
