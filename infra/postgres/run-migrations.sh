#!/bin/bash
# Run PostgreSQL migrations

set -e

echo "Running PostgreSQL migrations..."

# Wait for PostgreSQL to be ready
until PGPASSWORD=$POSTGRES_PASSWORD psql -h localhost -U postgres -d archmind -c '\q'; do
  echo "PostgreSQL is unavailable - sleeping"
  sleep 1
done

echo "PostgreSQL is up - executing migrations"

# Run migration files in order
for file in /docker-entrypoint-initdb.d/*.sql; do
    echo "Running migration: $file"
    PGPASSWORD=$POSTGRES_PASSWORD psql -h localhost -U postgres -d archmind -f "$file"
done

echo "Migrations completed successfully!"
