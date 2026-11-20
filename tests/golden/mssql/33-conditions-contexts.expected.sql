select *
from t
join u
    on u.Id = t.Id
    and (
            u.Kind = 1
            or u.Kind = 2
        )
select case
           when a = 1 and (
                              b = 2
                              or c = 3
                          ) then 'x'
       end as k
from t
if @a = 1 and (
                  @b = 2
                  or @c = 3
              )
    print 'yes'
while (
          @i < 10
          or @j < 10
      ) and @k = 0
    begin
        set @i = @i + 1
    end
merge dbo.Target t
using dbo.Source s
on t.Id = s.Id and (
                       t.Kind = s.Kind
                       or t.Kind is null
                   )
when matched and (t.a <> s.a or t.b <> s.b) then
    update
    set a = s.a;
create table dbo.Gadget
(
    Id int not null,
    constraint CK_Gadget check (Id > 0 and (
                                               Id < 10
                                               or Id > 100
                                           ))
)
select iif(a = 1 and (
                         b = 2
                         or c = 3
                     ), 1, 0) as f
from t
select k
from t
group by k
having (
           count(*) > 1
           or max(v) > 2
       )
while @i < 10 and @j < 10
    begin
        set @i = @i + 1
    end
