update dbo.Orders set Status='closed', ClosedAt=getdate() where Id=@id
update o set o.Total = l.Sum from dbo.Orders o join dbo.Lines l on l.OrderId = o.Id
delete from dbo.Orders where Id = 1
delete o from dbo.Orders o join dbo.Old x on x.Id = o.Id where x.Gone = 1
