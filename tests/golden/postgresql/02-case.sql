SELECT Count(*) AS n, Shelf_Label(s.id) AS label, public.Shelf_Label(s.id) AS qualified, CAST(s.pages AS VARCHAR(10)) AS p, s.pages::INTEGER AS i, Extract(YEAR FROM s.added) AS y, Current_Timestamp AS now, DATE '2020-01-01' AS d FROM Shelf s;
CREATE TABLE Book (Id SERIAL PRIMARY KEY, Title TEXT NOT NULL, Spot POINT, Mood Book_Mood, Added TIMESTAMP WITH TIME ZONE);
