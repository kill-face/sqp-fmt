\set shelf_id 3
SELECT id, title FROM book WHERE shelf_id = :shelf_id \gset
COPY book (id, title) FROM stdin;
1	A Tale of Two Cities
2	Bleak House
\.
CREATE FUNCTION add_one(i integer) RETURNS integer AS $$
  SELECT   i + 1;
$$ LANGUAGE sql;
SELECT E'it\'s', $tag$ a ; b $tag$ AS t FROM book;
SELECT 1 * - - - 5;
SELECT (1;
SELECT 2;
