-- Procedural code outside T-SQL keeps its source layout; the SQL inside it
-- is formatted.
BEGIN
  DECLARE n INT64 DEFAULT 1;
  IF n > 0 THEN
    SELECT id,title FROM books WHERE shelf_id=n;
  END IF;
EXCEPTION WHEN ERROR THEN
  SELECT 1;
END;
