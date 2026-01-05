package main

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"time"

	"github.com/gin-contrib/cors"
	"github.com/gin-gonic/gin"
	"github.com/go-redis/redis/v8"
	"github.com/google/uuid"
	"github.com/joho/godotenv"
	_ "github.com/lib/pq"
)

// AnalyzeRequest represents the incoming request to analyze a repository
type AnalyzeRequest struct {
	RepoURL     string            `json:"repo_url" binding:"required"`
	Branch      string            `json:"branch"`
	Options     map[string]string `json:"options"`
}

// AnalysisJob represents a job in the queue
type AnalysisJob struct {
	JobID       string            `json:"job_id"`
	RepoURL     string            `json:"repo_url"`
	Branch      string            `json:"branch"`
	Status      string            `json:"status"`
	Options     map[string]string `json:"options"`
	CreatedAt   time.Time         `json:"created_at"`
}

// JobResponse represents the response after creating a job
type JobResponse struct {
	JobID     string    `json:"job_id"`
	Status    string    `json:"status"`
	Message   string    `json:"message"`
	CreatedAt time.Time `json:"created_at"`
}

var (
	redisClient *redis.Client
	db          *sql.DB
	ctx         = context.Background()
)

func main() {
	// Load environment variables
	if err := godotenv.Load(); err != nil {
		log.Println("No .env file found, using environment variables")
	}

	// Initialize Redis client
	initRedis()

	// Initialize PostgreSQL connection
	initPostgres()

	// Initialize Gin router
	router := setupRouter()

	// Start server
	port := getEnv("PORT", "8080")
	log.Printf("🚀 API Gateway starting on port %s", port)
	if err := router.Run(":" + port); err != nil {
		log.Fatalf("Failed to start server: %v", err)
	}
}

// initRedis initializes the Redis client
func initRedis() {
	redisURL := getEnv("REDIS_URL", "localhost:6379")
	redisClient = redis.NewClient(&redis.Options{
		Addr:     redisURL,
		Password: getEnv("REDIS_PASSWORD", ""),
		DB:       0,
	})

	// Test connection
	if err := redisClient.Ping(ctx).Err(); err != nil {
		log.Fatalf("Failed to connect to Redis: %v", err)
	}
	log.Println("✅ Connected to Redis")
}

// initPostgres initializes the PostgreSQL connection
func initPostgres() {
	dbURL := getEnv("POSTGRES_URL", "postgresql://postgres:postgres@localhost:5432/archmind?sslmode=disable")
	var err error
	db, err = sql.Open("postgres", dbURL)
	if err != nil {
		log.Fatalf("Failed to connect to PostgreSQL: %v", err)
	}

	// Test connection
	if err := db.Ping(); err != nil {
		log.Fatalf("Failed to ping PostgreSQL: %v", err)
	}
	log.Println("✅ Connected to PostgreSQL")
}

// setupRouter configures the Gin router with all routes
func setupRouter() *gin.Engine {
	router := gin.Default()

	// CORS middleware
	router.Use(cors.New(cors.Config{
		AllowOrigins:     []string{"http://localhost:3000"},
		AllowMethods:     []string{"GET", "POST", "PUT", "DELETE", "OPTIONS"},
		AllowHeaders:     []string{"Origin", "Content-Type", "Authorization"},
		ExposeHeaders:    []string{"Content-Length"},
		AllowCredentials: true,
		MaxAge:           12 * time.Hour,
	}))

	// Health check
	router.GET("/health", healthCheck)

	// API routes
	v1 := router.Group("/api/v1")
	{
		// Repository analysis
		v1.POST("/analyze", analyzeRepository)
		v1.GET("/jobs/:id", getJobStatus)
		v1.GET("/jobs", listJobs)

		// Repository management
		v1.GET("/repositories", listRepositories)
		v1.GET("/repositories/:id", getRepository)
	}

	return router
}

// healthCheck returns the health status of the API Gateway
func healthCheck(c *gin.Context) {
	// Check Redis
	redisStatus := "healthy"
	if err := redisClient.Ping(ctx).Err(); err != nil {
		redisStatus = "unhealthy"
	}

	// Check PostgreSQL
	dbStatus := "healthy"
	if err := db.Ping(); err != nil {
		dbStatus = "unhealthy"
	}

	c.JSON(http.StatusOK, gin.H{
		"status": "ok",
		"services": gin.H{
			"redis":    redisStatus,
			"postgres": dbStatus,
		},
		"timestamp": time.Now().UTC(),
	})
}

// analyzeRepository handles the POST /api/v1/analyze endpoint
func analyzeRepository(c *gin.Context) {
	var req AnalyzeRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{
			"error": "Invalid request body",
			"details": err.Error(),
		})
		return
	}

	// Set default branch if not provided
	if req.Branch == "" {
		req.Branch = "main"
	}

	// Create job ID
	jobID := uuid.New().String()

	// Create job object
	job := AnalysisJob{
		JobID:     jobID,
		RepoURL:   req.RepoURL,
		Branch:    req.Branch,
		Status:    "QUEUED",
		Options:   req.Options,
		CreatedAt: time.Now().UTC(),
	}

	// Store job in PostgreSQL
	if err := storeJob(job); err != nil {
		log.Printf("Failed to store job in database: %v", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": "Failed to create analysis job",
		})
		return
	}

	// Serialize job to JSON
	jobJSON, err := json.Marshal(job)
	if err != nil {
		log.Printf("Failed to marshal job: %v", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": "Failed to create analysis job",
		})
		return
	}

	// Push job to Redis queue
	if err := redisClient.LPush(ctx, "analysis_queue", jobJSON).Err(); err != nil {
		log.Printf("Failed to push job to Redis: %v", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": "Failed to queue analysis job",
		})
		return
	}

	log.Printf("📝 Created analysis job: %s for repo: %s", jobID, req.RepoURL)

	// Return response
	c.JSON(http.StatusCreated, JobResponse{
		JobID:     jobID,
		Status:    "QUEUED",
		Message:   "Analysis job created successfully",
		CreatedAt: job.CreatedAt,
	})
}

// getJobStatus retrieves the status of a specific job
func getJobStatus(c *gin.Context) {
	jobID := c.Param("id")

	// Query database for job
	var job AnalysisJob
	var optionsJSON []byte
	err := db.QueryRow(`
		SELECT job_id, repo_url, branch, status, options, created_at 
		FROM analysis_jobs 
		WHERE job_id = $1
	`, jobID).Scan(&job.JobID, &job.RepoURL, &job.Branch, &job.Status, &optionsJSON, &job.CreatedAt)

	if err == sql.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{
			"error": "Job not found",
		})
		return
	} else if err != nil {
		log.Printf("Database error: %v", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": "Failed to retrieve job",
		})
		return
	}

	// Parse options JSON
	if len(optionsJSON) > 0 {
		json.Unmarshal(optionsJSON, &job.Options)
	}

	c.JSON(http.StatusOK, job)
}

// listJobs retrieves all analysis jobs
func listJobs(c *gin.Context) {
	rows, err := db.Query(`
		SELECT job_id, repo_url, branch, status, options, created_at 
		FROM analysis_jobs 
		ORDER BY created_at DESC 
		LIMIT 50
	`)
	if err != nil {
		log.Printf("Database error: %v", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": "Failed to retrieve jobs",
		})
		return
	}
	defer rows.Close()

	jobs := []AnalysisJob{}
	for rows.Next() {
		var job AnalysisJob
		var optionsJSON []byte
		if err := rows.Scan(&job.JobID, &job.RepoURL, &job.Branch, &job.Status, &optionsJSON, &job.CreatedAt); err != nil {
			log.Printf("Scan error: %v", err)
			continue
		}
		if len(optionsJSON) > 0 {
			json.Unmarshal(optionsJSON, &job.Options)
		}
		jobs = append(jobs, job)
	}

	c.JSON(http.StatusOK, gin.H{
		"jobs":  jobs,
		"total": len(jobs),
	})
}

// listRepositories retrieves all tracked repositories
func listRepositories(c *gin.Context) {
	rows, err := db.Query(`
		SELECT id, url, owner_id, created_at 
		FROM repositories 
		ORDER BY created_at DESC
	`)
	if err != nil {
		log.Printf("Database error: %v", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": "Failed to retrieve repositories",
		})
		return
	}
	defer rows.Close()

	repos := []map[string]interface{}{}
	for rows.Next() {
		var id int
		var url string
		var ownerID int
		var createdAt time.Time
		if err := rows.Scan(&id, &url, &ownerID, &createdAt); err != nil {
			log.Printf("Scan error: %v", err)
			continue
		}
		repos = append(repos, map[string]interface{}{
			"id":         id,
			"url":        url,
			"owner_id":   ownerID,
			"created_at": createdAt,
		})
	}

	c.JSON(http.StatusOK, gin.H{
		"repositories": repos,
		"total":        len(repos),
	})
}

// getRepository retrieves a specific repository by ID
func getRepository(c *gin.Context) {
	id := c.Param("id")

	var repoID int
	var url string
	var ownerID int
	var createdAt time.Time
	err := db.QueryRow(`
		SELECT id, url, owner_id, created_at 
		FROM repositories 
		WHERE id = $1
	`, id).Scan(&repoID, &url, &ownerID, &createdAt)

	if err == sql.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{
			"error": "Repository not found",
		})
		return
	} else if err != nil {
		log.Printf("Database error: %v", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": "Failed to retrieve repository",
		})
		return
	}

	c.JSON(http.StatusOK, gin.H{
		"id":         repoID,
		"url":        url,
		"owner_id":   ownerID,
		"created_at": createdAt,
	})
}

// storeJob stores an analysis job in PostgreSQL
func storeJob(job AnalysisJob) error {
	optionsJSON, err := json.Marshal(job.Options)
	if err != nil {
		return fmt.Errorf("failed to marshal options: %w", err)
	}

	_, err = db.Exec(`
		INSERT INTO analysis_jobs (job_id, repo_url, branch, status, options, created_at)
		VALUES ($1, $2, $3, $4, $5, $6)
	`, job.JobID, job.RepoURL, job.Branch, job.Status, optionsJSON, job.CreatedAt)

	return err
}

// getEnv retrieves an environment variable with a fallback default value
func getEnv(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}
