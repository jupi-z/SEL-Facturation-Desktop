#!/bin/sh
set -eu

# Runs only when the PostgreSQL volume is initialized for the first time.
# psql variables keep usernames/passwords out of the SQL source itself.
psql --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" \
  --set=app_database="$POSTGRES_DB" \
  --set=app_user="$POSTGRES_APP_USER" \
  --set=app_password="$POSTGRES_APP_PASSWORD" <<'SQL'
SELECT format('CREATE ROLE %I LOGIN PASSWORD %L', :'app_user', :'app_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'app_user')\gexec
GRANT ALL PRIVILEGES ON DATABASE :"app_database" TO :"app_user";
GRANT ALL ON SCHEMA public TO :"app_user";
SQL
