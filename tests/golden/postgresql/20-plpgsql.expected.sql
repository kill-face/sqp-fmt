-- A PL/pgSQL body is a dollar-quoted string: kept as written.
do $$
BEGIN
  IF (SELECT count(*) FROM books) > 0 THEN RAISE NOTICE 'books'; END IF;
END
$$;
