create function dbo.fnAdd
(
  @a int
, @b int
)
returns int as
begin
    return @a + @b
end
go
create trigger trWidget
    on dbo.Widget
    after insert, update
    as
begin
    set nocount on;
    update w
    set w.ModifiedAt = sysutcdatetime()
    from dbo.Widget w
    join inserted i
        on i.Id = w.Id
end
go
create view dbo.vWidget as
    select w.Id, w.Name
    from dbo.Widget w
go
