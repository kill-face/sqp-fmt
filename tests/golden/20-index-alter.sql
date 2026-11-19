create unique nonclustered index UX_Widget_Code on dbo.Widget (Code, Name) include (Price) where Price is not null
alter table dbo.Widget add constraint FK_Widget_Owner foreign key (OwnerId) references dbo.Owner (Id) on delete cascade
alter table dbo.Widget add constraint CK_Widget_Price check (Price >= 0)
