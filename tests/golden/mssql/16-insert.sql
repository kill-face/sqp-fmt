insert into dbo.Orders(Id,CustomerId,Total) values (1,2,3.5)
insert into dbo.Orders (Id, CustomerId, Total) values (1, 2, 3.5), (2, 3, 4.5)
insert into dbo.OrderArchive (OrderId, CustomerId, Total, CreatedAt, CreatedBy, ModifiedAt, ModifiedBy, ChangeReason, Source)
select Id, CustomerId, Total, CreatedAt, CreatedBy, ModifiedAt, ModifiedBy, ChangeReason, 'archive' from dbo.Orders
insert into dbo.Orders
(
    Id,
    Total
)
values (1, 2)
