\set shelf_id 3
select id, title
from book
where shelf_id = :shelf_id \gset
copy book (id, title) from stdin;
1	A Tale of Two Cities
2	Bleak House
\.
create function add_one
(
    i integer
)
returns integer as $$
  SELECT   i + 1;
$$ language sql;
select E'it\'s', $tag$ a ; b $tag$ as t
from book;
select 1 * - - -5;
SELECT (1;
SELECT 2;
