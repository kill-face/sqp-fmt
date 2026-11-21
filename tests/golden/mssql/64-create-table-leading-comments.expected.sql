create table dbo.Upload
(
    --- Surrogate key.
    UploadId  bigint       not null
        constraint PK__Upload primary key clustered,

    --- Name of the object.
    Name      varchar(100) not null,
    Size      bigint       not null, -- bytes
    WrittenAt datetime2(2) not null
);
