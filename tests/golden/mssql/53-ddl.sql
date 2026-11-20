create table dbo.Widget
(
    Id int not null identity (1,1),
    Name nvarchar(100) not null,
    Code char(3) constraint DF_Widget_Code default 'abc' not null,
    Note nvarchar(200)
        constraint DF_Widget_Note default '' null,
    constraint FK_Widget_Customer foreign key (Id) references dbo.Customers (Id),
    constraint FK_Widget_Owner foreign key (Code)
        references dbo.Owner (Code)
)
go
create procedure dbo.GetWidgets (@minPrice decimal(10, 2), @name nvarchar(100) = null) as
begin
    set nocount on;
end
go
create index IX_Widget_Name on dbo.Widget (Name, Code, Id) include (Price)
alter table dbo.Widget add constraint FK_A foreign key (Id) references dbo.A (Id), add constraint FK_B foreign key (Code) references dbo.B (Code)
go
create view dbo.vWidget (WidgetIdentifier, WidgetName, WidgetCodeValue, WidgetOwnerIdentifier, WidgetCreatedAt, WidgetNote) as
select Id, Name, Code, OwnerId, CreatedAt, Note from dbo.Widget
