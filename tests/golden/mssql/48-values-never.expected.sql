insert into dbo.Widget (Id, Name)
values (
           1,
           'one'
       ), (
              2,
              'two'
          )
insert into dbo.OrderArchive (
                                 OrderId
                               , CustomerId
                               , Total
                               , CreatedAt
                               , CreatedBy
                               , ModifiedAt
                               , ModifiedBy
                               , ChangeReason
                               , Source
                             )
select *
from dbo.Orders
