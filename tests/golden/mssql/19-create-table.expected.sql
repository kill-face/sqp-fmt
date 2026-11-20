create table dbo.Widget
(
    Id    int                                   not null identity (1,1),
    Name  nvarchar(100)                         not null,
    Price decimal(10, 2)                        null,
    Code  char(3)
        constraint DF_Widget_Code default 'abc' not null,
    constraint PK_Widget primary key clustered (Id),
    constraint FK_Widget_Customer foreign key (Id)
        references dbo.Customers (Id)
)
create table #Tmp
(
    Id      int
        constraint PK_Tmp primary key clustered,
    Created datetime2(2)
        constraint DF_Tmp_Created default sysutcdatetime() not null,
    Note    nvarchar(max)                                  null,
    Total   as (Id * 2) persisted
);
