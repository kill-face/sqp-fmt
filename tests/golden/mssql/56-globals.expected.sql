DECLARE @x NVARCHAR(MAX) = convert(VARCHAR(10), 5), @y dbo.MyType, @t TABLE
(
  Id INT NOT NULL
)
SELECT [Name]
     , Status
     , Count(*)           AS Cnt
     , getdate()          AS Now
     , @@rowcount         AS N
     , cast(a AS CHAR(3)) AS c
FROM dbo.Widget w
JOIN dbo.Owner o
  ON o.Id = w.OwnerId
WHERE EXISTS
        (
          SELECT 1
          FROM dbo.X x
          WHERE x.Id = w.Id
        )
FOR XML PATH
IF @x IS NULL
  BEGIN
    SELECT CASE
             WHEN a = 1  THEN 'one'
             WHEN a = 22 THEN 'twenty-two'
             ELSE 'other'
           END AS Kind
    FROM t
  END
go
CREATE TABLE dbo.Widget
(
  Id   INT                                  NOT NULL IDENTITY (1,1),
  Code CHAR(3)
    CONSTRAINT DF_Widget_Code DEFAULT 'abc' NOT NULL
)
