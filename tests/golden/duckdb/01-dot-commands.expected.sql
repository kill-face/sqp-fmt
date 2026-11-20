.mode line
select id, title
from 'books.parquet'
where pages > 100;
