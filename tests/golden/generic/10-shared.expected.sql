-- Shared rules: the same layout in every dialect.
with recent as
    (
        select reader_id, max(returned_on) as last_return
        from loans
        group by reader_id
    )
   , busy as
    (
        select reader_id
        from loans
        where returned_on is null
        group by reader_id
        having count(*) > 3
    )
select r.id
     , r."full name"
     , rc.last_return
     , case
           when b.reader_id is not null            then 'busy'
           when rc.last_return < date '2020-01-01' then 'lapsed'
           else 'active'
       end                                                                       as standing
     , row_number() over (partition by r.branch_id order by rc.last_return desc) as rn
from readers r
left join recent rc
    on rc.reader_id = r.id and rc.last_return is not null
join branches br
    on br.id = r.branch_id
left join busy b
    on b.reader_id = r.id
where r.active = 1
    and (r.joined_on >= date '2019-01-01' or r.vip = 1)
    and exists
        (
            select 1
            from loans l
            where l.reader_id = r.id
        )
order by rn, r.id;

select id
from books
where shelf_id in (
                      select id
                      from shelves
                      where floor = 2
                  )
union all
select id
from archive;

insert into loans (reader_id, book_id, lent_on)
values (1, 10, date '2024-05-01')
     , (2, 11, date '2024-05-02');

update books
set shelf_id = 3
  , moved_on = current_date
where shelf_id = 2
    and pages > 500;

delete from loans
where returned_on < date '2000-01-01';

create table shelves
(
    id    int not null,
    floor int not null,
    label varchar(20),
    primary key (id)
);

-- Lexical: -- and /* */ comments; - - 5 isn't --5.
select 1 * - -5 as n, /* inline */ 2 as m -- trailing
from shelves;
