# ArchMind Backend - Docker Commands
# Quick reference for common Docker operations
# Usage on Windows: Just copy/paste the command part after the colon

# Help - Show available commands
help:
	@echo "ArchMind Backend - Docker Commands"
	@echo "==================================="
	@echo ""
	@echo "Development:"
	@echo "  make start         - Start all services"
	@echo "  make stop          - Stop all services"
	@echo "  make restart       - Restart all services"
	@echo "  make build         - Rebuild all images"
	@echo "  make logs          - Follow logs (all services)"
	@echo ""
	@echo "Database:"
	@echo "  make migrate       - Run database migrations"
	@echo "  make db-shell      - Open PostgreSQL shell"
	@echo "  make neo4j-shell   - Open Neo4j browser"
	@echo "  make redis-shell   - Open Redis CLI"
	@echo ""
	@echo "Testing:"
	@echo "  make health        - Run health checks"
	@echo "  make status        - Show container status"
	@echo ""
	@echo "Cleanup:"
	@echo "  make clean         - Stop and remove containers"
	@echo "  make clean-all     - Remove containers and volumes"
	@echo "  make clean-images  - Remove all images"

# Development Commands
start:
	docker-compose up -d

stop:
	docker-compose down

restart:
	docker-compose restart

build:
	docker-compose build --no-cache

rebuild:
	docker-compose up --build -d

logs:
	docker-compose logs -f

logs-api:
	docker-compose logs -f api-gateway

logs-graph:
	docker-compose logs -f graph-engine

logs-worker:
	docker-compose logs -f ingestion-worker

# Database Commands
migrate:
	@echo "Running database migrations..."
	@docker exec archmind-postgres psql -U postgres -d arch-mind < infra/postgres/init/001_schema.sql
	@docker exec archmind-postgres psql -U postgres -d arch-mind < infra/postgres/init/002_file_contributions.sql
	@docker exec archmind-postgres psql -U postgres -d arch-mind < infra/postgres/init/003_architecture_insights.sql
	@echo "Migrations complete!"

db-shell:
	docker exec -it archmind-postgres psql -U postgres -d arch-mind

neo4j-shell:
	@echo "Opening Neo4j browser at http://localhost:7474"
	@echo "Username: neo4j"
	@echo "Password: neo4j123"
	@start http://localhost:7474

redis-shell:
	docker exec -it archmind-redis redis-cli

# Testing Commands
health:
	@powershell -ExecutionPolicy Bypass -File healthcheck.ps1

status:
	docker ps --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}"

# Cleanup Commands
clean:
	docker-compose down

clean-all:
	docker-compose down -v
	@echo "All containers and volumes removed!"

clean-images:
	docker-compose down --rmi all
	@echo "All images removed!"

# Quick Start
quick-start:
	@powershell -ExecutionPolicy Bypass -File quick-start.ps1

.PHONY: help start stop restart build rebuild logs logs-api logs-graph logs-worker migrate db-shell neo4j-shell redis-shell health status clean clean-all clean-images quick-start
