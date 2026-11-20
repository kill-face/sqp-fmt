.headers on
.mode column
# a comment line for the shell
select id, [title], `subtitle`
from book
where pages > 100;
create trigger book_touch after update on book
begin
  update shelf
  set touched = 1
  where id = new.shelf_id;
end;
.quit
