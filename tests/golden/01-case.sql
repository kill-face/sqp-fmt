SELECT [Name], Status, Value, COUNT(*) AS Cnt, SUM(o.Total) Total, GETDATE() AS Now, @@ROWCOUNT AS N
FROM dbo.Orders o
WHERE o.Status IS NOT NULL AND DATEADD(DAY, -1, o.CreatedAt) < SYSUTCDATETIME()
GROUP BY [Name], Status, Value
DECLARE @x NVARCHAR(MAX) = CONVERT(VARCHAR(10), 5), @y dbo.MyType, @z SYSNAME
SELECT dbo.fnCustom(1), CAST(1 AS DECIMAL(10, 2)), Next VALUE FOR dbo.seqOrder
