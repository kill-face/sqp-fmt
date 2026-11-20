SELECT id, name FROM shelf WHERE id = 1;
DELIMITER //
CREATE PROCEDURE count_books(IN min_pages INT)
BEGIN
  SELECT COUNT(*) FROM book WHERE pages >= min_pages;
END //
DELIMITER ;
source extra_books.sql
SELECT `title`, "subtitle" FROM book WHERE title LIKE 'A\'s%' /*!50100 AND pages > 0 */;
select broken from from where;
SELECT   1;
DELIMITER $$
SELECT id FROM shelf WHERE id = 2$$
SELECT   'a$$b' AS s FROM shelf $$
DELIMITER ;
