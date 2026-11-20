SELECT COUNT(*) AS n, SHELF_LABEL(s.id) AS label, public.Shelf_Label(s.id) AS qualified, CAST(s.pages AS varchar(10)) AS p, s.pages::integer AS i, EXTRACT(YEAR FROM s.added) AS y, CURRENT_TIMESTAMP AS now, date '2020-01-01' AS d
FROM Shelf s;
CREATE TABLE Book
(
    Id    serial PRIMARY KEY,
    Title text NOT NULL,
    Spot  point,
    Mood  Book_Mood,
    Added timestamp with time zone
);
