-- Shared rules: the same layout in every dialect.
WITH recent AS (SELECT reader_id, max(returned_on) AS last_return FROM loans GROUP BY reader_id), busy AS (SELECT reader_id FROM loans WHERE returned_on IS NULL GROUP BY reader_id HAVING count(*) > 3)
SELECT r.id, r."full name", rc.last_return, CASE WHEN b.reader_id IS NOT NULL THEN 'busy' WHEN rc.last_return < DATE '2020-01-01' THEN 'lapsed' ELSE 'active' END AS standing, row_number() OVER (PARTITION BY r.branch_id ORDER BY rc.last_return DESC) AS rn
FROM readers r
LEFT JOIN recent rc ON rc.reader_id = r.id AND rc.last_return IS NOT NULL
JOIN branches br ON br.id = r.branch_id
LEFT JOIN busy b ON b.reader_id = r.id
WHERE r.active = 1 AND (r.joined_on >= DATE '2019-01-01' OR r.vip = 1) AND EXISTS (SELECT 1 FROM loans l WHERE l.reader_id = r.id)
ORDER BY rn, r.id;

SELECT id FROM books WHERE shelf_id IN (SELECT id FROM shelves WHERE floor = 2) UNION ALL SELECT id FROM archive;

INSERT INTO loans (reader_id, book_id, lent_on) VALUES (1, 10, DATE '2024-05-01'), (2, 11, DATE '2024-05-02');

UPDATE books SET shelf_id = 3, moved_on = CURRENT_DATE WHERE shelf_id = 2 AND pages > 500;

DELETE FROM loans WHERE returned_on < DATE '2000-01-01';

CREATE TABLE shelves (id INT NOT NULL, floor INT NOT NULL, label VARCHAR(20), PRIMARY KEY (id));
