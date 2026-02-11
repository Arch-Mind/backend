#!/bin/bash
# Health Check Script for Docker Deployment

echo "🔍 ArchMind Backend Health Check"
echo "=================================="

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check API Gateway
echo -e "\n${YELLOW}Checking API Gateway...${NC}"
if curl -s -f http://localhost:8080/health > /dev/null 2>&1; then
    echo -e "${GREEN}✅ API Gateway is healthy${NC}"
else
    echo -e "${RED}❌ API Gateway is not responding${NC}"
fi

# Check Graph Engine
echo -e "\n${YELLOW}Checking Graph Engine...${NC}"
if curl -s -f http://localhost:8000/health > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Graph Engine is healthy${NC}"
else
    echo -e "${RED}❌ Graph Engine is not responding${NC}"
fi

# Check PostgreSQL
echo -e "\n${YELLOW}Checking PostgreSQL...${NC}"
if docker exec archmind-postgres pg_isready -U postgres > /dev/null 2>&1; then
    echo -e "${GREEN}✅ PostgreSQL is ready${NC}"
else
    echo -e "${RED}❌ PostgreSQL is not ready${NC}"
fi

# Check Neo4j
echo -e "\n${YELLOW}Checking Neo4j...${NC}"
if curl -s -f http://localhost:7474 > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Neo4j is accessible${NC}"
else
    echo -e "${RED}❌ Neo4j is not accessible${NC}"
fi

# Check Redis
echo -e "\n${YELLOW}Checking Redis...${NC}"
if docker exec archmind-redis redis-cli ping > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Redis is responding${NC}"
else
    echo -e "${RED}❌ Redis is not responding${NC}"
fi

echo -e "\n${YELLOW}=================================${NC}"
echo -e "${GREEN}Health check complete!${NC}"
