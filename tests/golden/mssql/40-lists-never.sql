select a.Id
     , a.Name as CustomerName
     , a.CreatedAt as Created
from dbo.Alpha a
group by a.Id, a.Name, a.CreatedAt
order by a.Id, a.Name
select OrderIdentifier, CustomerIdentifier, RegionIdentifier, ProductIdentifier, WarehouseIdentifier, CarrierIdentifier from dbo.Orders
order by OrderIdentifier, CustomerIdentifier, RegionIdentifier, ProductIdentifier, WarehouseIdentifier, CarrierIdentifier
