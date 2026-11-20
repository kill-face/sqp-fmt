.headers on
.mode column
# a comment line for the shell
SELECT id, [title], `subtitle` FROM book WHERE pages > 100;
CREATE TRIGGER book_touch AFTER UPDATE ON book
BEGIN
  UPDATE shelf SET touched = 1 WHERE id = new.shelf_id;
END;
.quit
