select top 10 o.Id, c.[Name], sum(o.Total) as total, count(*) cnt from dbo.Orders o with (nolock) inner join dbo.Customers c on c.Id=o.CustomerId and c.IsActive=1 left join dbo.Regions r on r.Id=c.RegionId where o.CreatedAt>=@from and (o.Status='open' or o.Status='pending') group by o.Id, c.[Name] having sum(o.Total)>100 order by total desc, c.[Name] asc
GO
SELECT Id FROM dbo.Orders WHERE Id IN (SELECT OrderId FROM dbo.OrderLines WHERE Qty > 5)
GO
update dbo.Orders set Status='closed', ClosedAt=getdate() where Id=@id
insert into dbo.Orders (Id, CustomerId, Total) values (1, 2, 3.5), (2, 3, 4.5)
GO
create table dbo.Widget (Id int not null identity(1,1), Name nvarchar(100) not null, Price decimal(10,2) null, constraint PK_Widget primary key clustered (Id), constraint FK_Widget_Customer foreign key (Id) references dbo.Customers (Id))
GO
create procedure dbo.GetWidgets @minPrice decimal(10,2), @name nvarchar(100) = null as begin set nocount on; if @name is null begin select * from dbo.Widget where Price >= @minPrice end else begin select Id, case when Price > 100 then 'expensive' when Price > 10 then 'normal' else 'cheap' end as Band from dbo.Widget where Name like @name + '%' end end
GO
with recent as (select CustomerId, max(CreatedAt) as LastOrder from dbo.Orders group by CustomerId) select c.Id, r.LastOrder from dbo.Customers c join recent r on r.CustomerId = c.Id
