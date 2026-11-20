SELECT CONCAT(`first`, ' ', `last`) AS name, CONVERT(pages, char) AS p, TRIM(title) AS t, SUBSTRING(title, 1, 3) AS s
FROM book;
