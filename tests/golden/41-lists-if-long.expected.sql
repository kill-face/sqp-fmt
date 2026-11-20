select OrderIdentifier, CustomerIdentifier,
       RegionIdentifier, ProductIdentifier,
       WarehouseIdentifier
from dbo.Orders
group by OrderIdentifier, CustomerIdentifier
       , RegionIdentifier, ProductIdentifier
       , WarehouseIdentifier
order by OrderIdentifier, CustomerIdentifier
select a, b,
       c
from t
