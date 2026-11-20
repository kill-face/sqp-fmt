SELECT Count(*) AS Cnt, GetDate() AS Now, DATEADD(DAY, 1, x) AS Tomorrow, CAST(a AS INT) AS i FROM T LEFT JOIN U ON 1 = 1
DECLARE @t TABLE (Id INT)
