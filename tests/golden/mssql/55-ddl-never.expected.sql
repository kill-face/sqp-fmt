create index IX_W
    on dbo.Widget
        (NameOfTheWidget, CodeOfTheWidget, Id)
alter table dbo.W
    add constraint A unique (x), add constraint B unique (y)
go
create view dbo.vWidget (WidgetIdentifier, WidgetName) as
    select Id, Name
    from dbo.Widget
