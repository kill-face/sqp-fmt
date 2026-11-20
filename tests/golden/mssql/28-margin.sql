select OrderIdentifier, CustomerIdentifier, RegionIdentifier, ProductIdentifier, WarehouseIdentifier, CarrierIdentifier from dbo.Orders
select case when SomeRatherLongColumnName = 1 then 'first value' when AnotherRatherLongColumnName = 2 then 'second value' else 'otherwise' end as Kind from t
