!set variable_substitution=true
select id, title
from book
where shelf_id = 3;
!source more_books.sql
select 1
!= 2;
