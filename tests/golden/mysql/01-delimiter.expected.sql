select id, name
from shelf
where id = 1;
DELIMITER //
CREATE PROCEDURE count_books(IN min_pages INT)
BEGIN
  SELECT COUNT(*) FROM book WHERE pages >= min_pages;
END //
DELIMITER ;
source extra_books.sql
select `title`, "subtitle"
from book
where title like 'A\'s%' /*!50100 AND pages > 0 */;
select broken from from where;
select 1;
DELIMITER $$
select id
from shelf
where id = 2$$
select 'a$$b' as s
from shelf $$
DELIMITER ;
