create index IX_W on dbo.Widget (NameOfTheWidget, CodeOfTheWidget, OwnerOfTheWidget, Id)
alter table dbo.Widget add constraint FK_A foreign key (Id) references dbo.A (Id), add constraint FK_B foreign key (Code) references dbo.B (Code)
alter table dbo.W add constraint A unique (x), add constraint B unique (y)
go
create view dbo.vWidget (WidgetIdentifier, WidgetName, WidgetCodeValue) as
select Id, Name, Code from dbo.Widget
