-- A PL/pgSQL body is a dollar-quoted string: kept as written.
DO $$
BEGIN
  IF (SELECT count(*) FROM books) > 0 THEN RAISE NOTICE 'books'; END IF;
END
$$;
