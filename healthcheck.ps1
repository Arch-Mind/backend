# Health Check Script for Docker Deployment (PowerShell)

Write-Host "`n🔍 ArchMind Backend Health Check" -ForegroundColor Cyan
Write-Host "==================================" -ForegroundColor Cyan

# Check API Gateway
Write-Host "`nChecking API Gateway..." -ForegroundColor Yellow
try {
    $response = Invoke-WebRequest -Uri "http://localhost:8080/health" -Method Get -TimeoutSec 5 -UseBasicParsing
    if ($response.StatusCode -eq 200) {
        Write-Host "✅ API Gateway is healthy" -ForegroundColor Green
    }
} catch {
    Write-Host "❌ API Gateway is not responding" -ForegroundColor Red
}

# Check Graph Engine
Write-Host "`nChecking Graph Engine..." -ForegroundColor Yellow
try {
    $response = Invoke-WebRequest -Uri "http://localhost:8000/health" -Method Get -TimeoutSec 5 -UseBasicParsing
    if ($response.StatusCode -eq 200) {
        Write-Host "✅ Graph Engine is healthy" -ForegroundColor Green
    }
} catch {
    Write-Host "❌ Graph Engine is not responding" -ForegroundColor Red
}

# Check PostgreSQL
Write-Host "`nChecking PostgreSQL..." -ForegroundColor Yellow
try {
    $pgCheck = docker exec archmind-postgres pg_isready -U postgres 2>&1
    if ($LASTEXITCODE -eq 0) {
        Write-Host "✅ PostgreSQL is ready" -ForegroundColor Green
    } else {
        Write-Host "❌ PostgreSQL is not ready" -ForegroundColor Red
    }
} catch {
    Write-Host "❌ PostgreSQL container is not running" -ForegroundColor Red
}

# Check Neo4j
Write-Host "`nChecking Neo4j..." -ForegroundColor Yellow
try {
    $response = Invoke-WebRequest -Uri "http://localhost:7474" -Method Get -TimeoutSec 5 -UseBasicParsing
    Write-Host "✅ Neo4j is accessible" -ForegroundColor Green
} catch {
    Write-Host "❌ Neo4j is not accessible" -ForegroundColor Red
}

# Check Redis
Write-Host "`nChecking Redis..." -ForegroundColor Yellow
try {
    $redisCheck = docker exec archmind-redis redis-cli ping 2>&1
    if ($redisCheck -match "PONG") {
        Write-Host "✅ Redis is responding" -ForegroundColor Green
    } else {
        Write-Host "❌ Redis is not responding" -ForegroundColor Red
    }
} catch {
    Write-Host "❌ Redis container is not running" -ForegroundColor Red
}

Write-Host "`n==================================" -ForegroundColor Cyan
Write-Host "Health check complete!`n" -ForegroundColor Green
