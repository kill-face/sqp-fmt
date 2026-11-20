SELECT Concat(`first`, ' ', `last`) AS name, Convert(pages, CHAR) AS p, Trim(title) AS t, Substring(title, 1, 3) AS s FROM book;
