# ArchMind Backend Quick Start Script (PowerShell)
# This script automates the Docker setup and initial configuration

Write-Host "`n🚀 ArchMind Backend Quick Start" -ForegroundColor Cyan
Write-Host "================================" -ForegroundColor Cyan

# Check if Docker is running
Write-Host "`n📦 Checking Docker..." -ForegroundColor Yellow
try {
    docker ps | Out-Null
    Write-Host "✅ Docker is running" -ForegroundColor Green
} catch {
    Write-Host "❌ Docker is not running. Please start Docker Desktop first." -ForegroundColor Red
    exit 1
}

# Check if .env file exists
Write-Host "`n🔑 Checking environment configuration..." -ForegroundColor Yellow
$envFile = "services\graph-engine\.env"
if (-not (Test-Path $envFile)) {
    Write-Host "⚠️  No .env file found. Creating from example..." -ForegroundColor Yellow
    Copy-Item "services\graph-engine\.env.example" $envFile
    Write-Host "📝 Please edit $envFile and add your GEMINI_API_KEY" -ForegroundColor Yellow
    Write-Host "   Press any key when ready to continue..."
    $null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
}

# Stop existing containers
Write-Host "`n🛑 Stopping existing containers..." -ForegroundColor Yellow
docker-compose down 2>&1 | Out-Null

# Build and start services
Write-Host "`n🔨 Building Docker images (this may take a few minutes)..." -ForegroundColor Yellow
docker-compose build --no-cache

Write-Host "`n🚀 Starting all services..." -ForegroundColor Yellow
docker-compose up -d

# Wait for services to be ready
Write-Host "`n⏳ Waiting for services to start (30 seconds)..." -ForegroundColor Yellow
Start-Sleep -Seconds 30

# Run health checks
Write-Host "`n🏥 Running health checks..." -ForegroundColor Yellow
.\healthcheck.ps1

# Run database migrations
Write-Host "`n📊 Running database migrations..." -ForegroundColor Yellow
$migrations = @(
    "infra\postgres\init\001_schema.sql",
    "infra\postgres\init\002_file_contributions.sql",
    "infra\postgres\init\003_architecture_insights.sql"
)

foreach ($migration in $migrations) {
    if (Test-Path $migration) {
        Write-Host "  Running $migration..." -ForegroundColor Cyan
        Get-Content $migration | docker exec -i archmind-postgres psql -U postgres -d arch-mind
        if ($LASTEXITCODE -eq 0) {
            Write-Host "  ✅ Migration successful" -ForegroundColor Green
        } else {
            Write-Host "  ⚠️  Migration may have already been applied" -ForegroundColor Yellow
        }
    }
}

# Final status
Write-Host "`n================================" -ForegroundColor Cyan
Write-Host "🎉 Setup Complete!" -ForegroundColor Green
Write-Host "`nServices are running at:" -ForegroundColor Cyan
Write-Host "  • API Gateway:  http://localhost:8080" -ForegroundColor White
Write-Host "  • Graph Engine: http://localhost:8000" -ForegroundColor White
Write-Host "  • Neo4j Browser: http://localhost:7474" -ForegroundColor White
Write-Host "  • PostgreSQL:    localhost:5432" -ForegroundColor White
Write-Host "  • Redis:         localhost:6379" -ForegroundColor White

Write-Host "`n📚 Next steps:" -ForegroundColor Cyan
Write-Host "  1. Test API: curl http://localhost:8080/health" -ForegroundColor White
Write-Host "  2. View logs: docker-compose logs -f" -ForegroundColor White
Write-Host "  3. Stop services: docker-compose down" -ForegroundColor White
Write-Host "  4. Deploy to Railway: See RAILWAY_DEPLOYMENT.md`n" -ForegroundColor White
