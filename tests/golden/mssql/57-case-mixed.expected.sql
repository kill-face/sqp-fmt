SELECT COUNT(*) AS Cnt, GETDATE() AS Now, DATEADD(DAY, 1, x) AS Tomorrow, CAST(a AS int) AS i
FROM T
LEFT JOIN U
    ON 1 = 1
DECLARE @t table
(
    Id int
)
