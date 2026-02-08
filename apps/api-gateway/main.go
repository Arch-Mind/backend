package main

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"time"

	"github.com/gin-contrib/cors"
	"github.com/gin-gonic/gin"
	"github.com/go-redis/redis/v8"
	"github.com/google/uuid"
	"github.com/joho/godotenv"
	_ "github.com/lib/pq"
)

// =============================================================================
// GitHub Webhook Types
// =============================================================================

// GitHubPushPayload represents the payload from a GitHub push event
type GitHubPushPayload struct {
	Ref        string           `json:"ref"`
	Before     string           `json:"before"`
	After      string           `json:"after"`
	Repository GitHubRepository `json:"repository"`
	Pusher     GitHubPusher     `json:"pusher"`
	Commits    []GitHubCommit   `json:"commits"`
	HeadCommit *GitHubCommit    `json:"head_commit"`
}

// GitHubPullRequestPayload represents the payload from a GitHub pull_request event
type GitHubPullRequestPayload struct {
	Action      string            `json:"action"`
	Number      int               `json:"number"`
	PullRequest GitHubPullRequest `json:"pull_request"`
	Repository  GitHubRepository  `json:"repository"`
}

// GitHubRepository represents repository information in webhook payloads
type GitHubRepository struct {
	ID       int64  `json:"id"`
	Name     string `json:"name"`
	FullName string `json:"full_name"`
	CloneURL string `json:"clone_url"`
	HTMLURL  string `json:"html_url"`
	Private  bool   `json:"private"`
	Owner    struct {
		Login string `json:"login"`
	} `json:"owner"`
	DefaultBranch string `json:"default_branch"`
}

// GitHubPusher represents the user who pushed the code
type GitHubPusher struct {
	Name  string `json:"name"`
	Email string `json:"email"`
}

// GitHubCommit represents a commit in the push payload
type GitHubCommit struct {
	ID        string `json:"id"`
	Message   string `json:"message"`
	Timestamp string `json:"timestamp"`
	Author    struct {
		Name  string `json:"name"`
		Email string `json:"email"`
	} `json:"author"`
	Added    []string `json:"added"`
	Removed  []string `json:"removed"`
	Modified []string `json:"modified"`
}

// GitHubPullRequest represents a pull request in the webhook payload
type GitHubPullRequest struct {
	ID     int64  `json:"id"`
	Number int    `json:"number"`
	State  string `json:"state"`
	Title  string `json:"title"`
	Head   struct {
		Ref string `json:"ref"`
		SHA string `json:"sha"`
	} `json:"head"`
	Base struct {
		Ref string `json:"ref"`
	} `json:"base"`
}

// WebhookResponse represents the response sent back to GitHub
type WebhookResponse struct {
	Status  string `json:"status"`
	Message string `json:"message"`
	JobID   string `json:"job_id,omitempty"`
}

// Supported file extensions for code analysis
var analyzableExtensions = map[string]bool{
	".ts":   true,
	".tsx":  true,
	".js":   true,
	".jsx":  true,
	".go":   true,
	".rs":   true,
	".py":   true,
	".java": true,
}

// AnalyzeRequest represents the incoming request to analyze a repository
type AnalyzeRequest struct {
	RepoURL string            `json:"repo_url" binding:"required"`
	Branch  string            `json:"branch"`
	Options map[string]string `json:"options"`
}

// AnalysisJob represents a job in the queue
type AnalysisJob struct {
	JobID     string            `json:"job_id"`
	RepoURL   string            `json:"repo_url"`
	Branch    string            `json:"branch"`
	Status    string            `json:"status"`
	Options   map[string]string `json:"options"`
	CreatedAt time.Time         `json:"created_at"`
}

// JobResponse represents the response after creating a job
type JobResponse struct {
	JobID     string    `json:"job_id"`
	Status    string    `json:"status"`
	Message   string    `json:"message"`
	CreatedAt time.Time `json:"created_at"`
}

// JobUpdateRequest represents the request to update a job
type JobUpdateRequest struct {
	Status        *string                `json:"status,omitempty"`
	Progress      *int                   `json:"progress,omitempty"`
	ResultSummary map[string]interface{} `json:"result_summary,omitempty"`
	Error         *string                `json:"error,omitempty"`
}

// JobUpdateResponse represents the response after updating a job
type JobUpdateResponse struct {
	JobID     string    `json:"job_id"`
	Status    string    `json:"status"`
	Message   string    `json:"message"`
	UpdatedAt time.Time `json:"updated_at"`
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

	// Configure HTTP server
	port := getEnv("PORT", "8080")
	srv := &http.Server{
		Addr:    ":" + port,
		Handler: router,
	}

	// Start server in a goroutine
	go func() {
		log.Printf("🚀 API Gateway starting on port %s", port)
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Fatalf("Failed to start server: %v", err)
		}
	}()

	// Setup graceful shutdown
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)

	// Wait for interrupt signal
	<-quit
	log.Println("🛑 Shutting down API Gateway...")

	// Create shutdown context with 30-second timeout
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	// Shutdown HTTP server gracefully
	if err := srv.Shutdown(ctx); err != nil {
		log.Printf("⚠️  Server forced to shutdown: %v", err)
	} else {
		log.Println("✅ HTTP server stopped")
	}

	// Close database connections
	if db != nil {
		if err := db.Close(); err != nil {
			log.Printf("⚠️  Error closing PostgreSQL: %v", err)
		} else {
			log.Println("✅ PostgreSQL connection closed")
		}
	}

	// Close Redis connection
	if redisClient != nil {
		if err := redisClient.Close(); err != nil {
			log.Printf("⚠️  Error closing Redis: %v", err)
		} else {
			log.Println("✅ Redis connection closed")
		}
	}

	log.Println("👋 API Gateway shutdown complete")
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

// initPostgres initializes the PostgreSQL connection with retry logic
func initPostgres() {
	dbURL := getEnv("POSTGRES_URL", "postgresql://postgres:postgres@localhost:5432/archmind?sslmode=disable")
	db = connectPostgresWithRetry(dbURL, 5)
	if db == nil {
		log.Fatal("Failed to connect to PostgreSQL after all retries")
	}
}

// connectPostgresWithRetry attempts to connect to PostgreSQL with exponential backoff
func connectPostgresWithRetry(dbURL string, maxRetries int) *sql.DB {
	var connection *sql.DB
	var err error

	for attempt := 1; attempt <= maxRetries; attempt++ {
		log.Printf("🔄 Attempting to connect to PostgreSQL... (attempt %d/%d)", attempt, maxRetries)

		connection, err = sql.Open("postgres", dbURL)
		if err != nil {
			if attempt < maxRetries {
				waitTime := time.Duration(1<<uint(attempt-1)) * time.Second // 1s, 2s, 4s, 8s, 16s
				log.Printf("⚠️  Failed to open PostgreSQL connection: %v. Retrying in %v (attempt %d/%d)...",
					err, waitTime, attempt, maxRetries)
				time.Sleep(waitTime)
				continue
			}
			log.Printf("❌ Failed to connect to PostgreSQL after %d attempts: %v", maxRetries, err)
			return nil
		}

		// Test the connection
		err = connection.Ping()
		if err != nil {
			connection.Close()
			if attempt < maxRetries {
				waitTime := time.Duration(1<<uint(attempt-1)) * time.Second // 1s, 2s, 4s, 8s, 16s
				log.Printf("⚠️  Failed to ping PostgreSQL: %v. Retrying in %v (attempt %d/%d)...",
					err, waitTime, attempt, maxRetries)
				time.Sleep(waitTime)
				continue
			}
			log.Printf("❌ Failed to ping PostgreSQL after %d attempts: %v", maxRetries, err)
			return nil
		}

		log.Println("✅ Successfully connected to PostgreSQL")
		return connection
	}

	return nil
}

// setupRouter configures the Gin router with all routes
func setupRouter() *gin.Engine {
	router := gin.Default()

	// CORS middleware
	router.Use(cors.New(cors.Config{
		AllowOrigins:     []string{"http://localhost:3000"},
		AllowMethods:     []string{"GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"},
		AllowHeaders:     []string{"Origin", "Content-Type", "Authorization"},
		ExposeHeaders:    []string{"Content-Length"},
		AllowCredentials: true,
		MaxAge:           12 * time.Hour,
	}))

	// Health check
	router.GET("/health", healthCheck)

	// Webhooks (no auth required, uses signature verification)
	webhooks := router.Group("/webhooks")
	{
		webhooks.POST("/github", handleGitHubWebhook)
	}

	// API routes
	v1 := router.Group("/api/v1")
	{
		// Repository analysis
		v1.POST("/analyze", analyzeRepository)
		v1.GET("/jobs/:id", getJobStatus)
		v1.PATCH("/jobs/:id", updateJob)
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
			"error":   "Invalid request body",
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

// updateJob handles the PATCH /api/v1/jobs/:id endpoint
func updateJob(c *gin.Context) {
	jobID := c.Param("id")

	// Parse request body
	var req JobUpdateRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{
			"error":   "Invalid request body",
			"details": err.Error(),
		})
		return
	}

	// Validate progress range if provided
	if req.Progress != nil && (*req.Progress < 0 || *req.Progress > 100) {
		c.JSON(http.StatusBadRequest, gin.H{
			"error": "Progress must be between 0 and 100",
		})
		return
	}

	// Get current job status from database
	var currentStatus string
	err := db.QueryRow("SELECT status FROM analysis_jobs WHERE job_id = $1", jobID).Scan(&currentStatus)
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

	// Validate status transition if status is being updated
	if req.Status != nil {
		if !validateStatusTransition(currentStatus, *req.Status) {
			c.JSON(http.StatusBadRequest, gin.H{
				"error":          "Invalid status transition",
				"current_status": currentStatus,
				"new_status":     *req.Status,
			})
			return
		}
	}

	// Update job in database
	updatedAt, err := updateJobInDB(jobID, req)
	if err != nil {
		log.Printf("Failed to update job: %v", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": "Failed to update job",
		})
		return
	}

	// Determine final status for response
	finalStatus := currentStatus
	if req.Status != nil {
		finalStatus = *req.Status
	}

	log.Printf("📝 Updated job %s: status=%s", jobID, finalStatus)

	c.JSON(http.StatusOK, JobUpdateResponse{
		JobID:     jobID,
		Status:    finalStatus,
		Message:   "Job updated successfully",
		UpdatedAt: updatedAt,
	})
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

// validateStatusTransition checks if a status transition is valid
func validateStatusTransition(currentStatus, newStatus string) bool {
	// Define valid transitions
	validTransitions := map[string][]string{
		"QUEUED":     {"PROCESSING", "CANCELLED"},
		"PROCESSING": {"COMPLETED", "FAILED", "CANCELLED"},
		"COMPLETED":  {}, // Terminal state
		"FAILED":     {}, // Terminal state
		"CANCELLED":  {}, // Terminal state
	}

	allowedTransitions, exists := validTransitions[currentStatus]
	if !exists {
		return false
	}

	// Check if new status is in allowed transitions
	for _, allowed := range allowedTransitions {
		if allowed == newStatus {
			return true
		}
	}

	return false
}

// updateJobInDB updates a job in the database with the provided fields
func updateJobInDB(jobID string, req JobUpdateRequest) (time.Time, error) {
	// Build dynamic UPDATE query based on provided fields
	updates := []string{}
	args := []interface{}{}
	argIndex := 1

	if req.Status != nil {
		updates = append(updates, fmt.Sprintf("status = $%d", argIndex))
		args = append(args, *req.Status)
		argIndex++

		// Set completed_at if status is COMPLETED or FAILED
		if *req.Status == "COMPLETED" || *req.Status == "FAILED" {
			updates = append(updates, fmt.Sprintf("completed_at = $%d", argIndex))
			args = append(args, time.Now().UTC())
			argIndex++
		}
	}

	if req.Progress != nil {
		updates = append(updates, fmt.Sprintf("progress = $%d", argIndex))
		args = append(args, *req.Progress)
		argIndex++
	}

	if req.ResultSummary != nil {
		resultJSON, err := json.Marshal(req.ResultSummary)
		if err != nil {
			return time.Time{}, fmt.Errorf("failed to marshal result_summary: %w", err)
		}
		updates = append(updates, fmt.Sprintf("result_summary = $%d", argIndex))
		args = append(args, resultJSON)
		argIndex++
	}

	if req.Error != nil {
		updates = append(updates, fmt.Sprintf("error_message = $%d", argIndex))
		args = append(args, *req.Error)
		argIndex++
	}

	if len(updates) == 0 {
		return time.Time{}, fmt.Errorf("no fields to update")
	}

	// Add job_id as the last argument
	args = append(args, jobID)

	// Build and execute query
	query := fmt.Sprintf(
		"UPDATE analysis_jobs SET %s WHERE job_id = $%d RETURNING updated_at",
		strings.Join(updates, ", "),
		argIndex,
	)

	var updatedAt time.Time
	err := db.QueryRow(query, args...).Scan(&updatedAt)
	if err != nil {
		return time.Time{}, fmt.Errorf("failed to execute update: %w", err)
	}

	return updatedAt, nil
}

// getEnv retrieves an environment variable with a fallback default value
func getEnv(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}

// =============================================================================
// GitHub Webhook Handlers
// =============================================================================

// handleGitHubWebhook processes incoming GitHub webhook events
// POST /webhooks/github
func handleGitHubWebhook(c *gin.Context) {
	// Step 1: Read the raw body for signature verification
	body, err := io.ReadAll(c.Request.Body)
	if err != nil {
		log.Printf("❌ Webhook: Failed to read request body: %v", err)
		c.JSON(http.StatusBadRequest, WebhookResponse{
			Status:  "error",
			Message: "Failed to read request body",
		})
		return
	}

	// Step 2: Verify the signature (security check)
	signature := c.GetHeader("X-Hub-Signature-256")
	if !verifyGitHubSignature(body, signature) {
		log.Printf("❌ Webhook: Invalid signature from IP: %s", c.ClientIP())
		c.JSON(http.StatusUnauthorized, WebhookResponse{
			Status:  "error",
			Message: "Invalid signature",
		})
		return
	}

	// Step 3: Check the event type
	eventType := c.GetHeader("X-GitHub-Event")
	deliveryID := c.GetHeader("X-GitHub-Delivery")

	log.Printf("📥 Webhook received: event=%s, delivery=%s", eventType, deliveryID)

	// Step 4: Route to appropriate handler based on event type
	switch eventType {
	case "push":
		handlePushEvent(c, body)
	case "pull_request":
		handlePullRequestEvent(c, body)
	case "ping":
		// GitHub sends a ping event when webhook is first configured
		c.JSON(http.StatusOK, WebhookResponse{
			Status:  "ok",
			Message: "Pong! Webhook configured successfully",
		})
	default:
		// Ignore other events but return 200 OK to acknowledge receipt
		log.Printf("ℹ️ Webhook: Ignoring event type: %s", eventType)
		c.JSON(http.StatusOK, WebhookResponse{
			Status:  "ignored",
			Message: fmt.Sprintf("Event type '%s' is not processed", eventType),
		})
	}
}

// verifyGitHubSignature validates the X-Hub-Signature-256 header
// This ensures the request actually came from GitHub
func verifyGitHubSignature(payload []byte, signature string) bool {
	secret := getEnv("GITHUB_WEBHOOK_SECRET", "")
	if secret == "" {
		log.Println("⚠️ Warning: GITHUB_WEBHOOK_SECRET not set, skipping signature verification")
		return true // Allow in development, but log warning
	}

	if signature == "" {
		return false
	}

	// Signature format: "sha256=<hex-encoded-signature>"
	if !strings.HasPrefix(signature, "sha256=") {
		return false
	}

	expectedMAC := signature[7:] // Remove "sha256=" prefix

	// Compute HMAC-SHA256
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write(payload)
	actualMAC := hex.EncodeToString(mac.Sum(nil))

	// Constant-time comparison to prevent timing attacks
	return hmac.Equal([]byte(expectedMAC), []byte(actualMAC))
}

// handlePushEvent processes GitHub push events
func handlePushEvent(c *gin.Context, body []byte) {
	var payload GitHubPushPayload
	if err := json.Unmarshal(body, &payload); err != nil {
		log.Printf("❌ Webhook: Failed to parse push payload: %v", err)
		c.JSON(http.StatusBadRequest, WebhookResponse{
			Status:  "error",
			Message: "Invalid push payload",
		})
		return
	}

	// Extract branch name from ref (refs/heads/main -> main)
	branch := extractBranchName(payload.Ref)

	log.Printf("📤 Push event: repo=%s, branch=%s, commits=%d",
		payload.Repository.FullName, branch, len(payload.Commits))

	// Check if any analyzable files were changed
	changedFiles := collectChangedFiles(payload.Commits)
	if !hasAnalyzableFiles(changedFiles) {
		log.Printf("ℹ️ Webhook: No analyzable files changed, skipping analysis")
		c.JSON(http.StatusOK, WebhookResponse{
			Status:  "skipped",
			Message: "No analyzable code files were changed",
		})
		return
	}

	// Create and queue analysis job
	jobID, err := createWebhookAnalysisJob(payload.Repository.CloneURL, branch, "push", changedFiles)
	if err != nil {
		log.Printf("❌ Webhook: Failed to create analysis job: %v", err)
		c.JSON(http.StatusInternalServerError, WebhookResponse{
			Status:  "error",
			Message: "Failed to create analysis job",
		})
		return
	}

	log.Printf("✅ Webhook: Created analysis job %s for push to %s/%s",
		jobID, payload.Repository.FullName, branch)

	// Return 200 OK immediately (must be < 500ms for GitHub)
	c.JSON(http.StatusOK, WebhookResponse{
		Status:  "queued",
		Message: "Analysis job created",
		JobID:   jobID,
	})
}

// handlePullRequestEvent processes GitHub pull request events
func handlePullRequestEvent(c *gin.Context, body []byte) {
	var payload GitHubPullRequestPayload
	if err := json.Unmarshal(body, &payload); err != nil {
		log.Printf("❌ Webhook: Failed to parse pull_request payload: %v", err)
		c.JSON(http.StatusBadRequest, WebhookResponse{
			Status:  "error",
			Message: "Invalid pull_request payload",
		})
		return
	}

	// Only process specific actions
	validActions := map[string]bool{
		"opened":      true,
		"synchronize": true, // New commits pushed to PR
		"reopened":    true,
	}

	if !validActions[payload.Action] {
		log.Printf("ℹ️ Webhook: Ignoring pull_request action: %s", payload.Action)
		c.JSON(http.StatusOK, WebhookResponse{
			Status:  "ignored",
			Message: fmt.Sprintf("Pull request action '%s' is not processed", payload.Action),
		})
		return
	}

	branch := payload.PullRequest.Head.Ref

	log.Printf("🔀 Pull request event: repo=%s, PR=#%d, action=%s, branch=%s",
		payload.Repository.FullName, payload.Number, payload.Action, branch)

	// Create and queue analysis job for the PR branch
	jobID, err := createWebhookAnalysisJob(
		payload.Repository.CloneURL,
		branch,
		"pull_request",
		nil, // PR events don't include file changes, analyze everything
	)
	if err != nil {
		log.Printf("❌ Webhook: Failed to create analysis job: %v", err)
		c.JSON(http.StatusInternalServerError, WebhookResponse{
			Status:  "error",
			Message: "Failed to create analysis job",
		})
		return
	}

	log.Printf("✅ Webhook: Created analysis job %s for PR #%d on %s",
		jobID, payload.Number, payload.Repository.FullName)

	c.JSON(http.StatusOK, WebhookResponse{
		Status:  "queued",
		Message: fmt.Sprintf("Analysis job created for PR #%d", payload.Number),
		JobID:   jobID,
	})
}

// extractBranchName extracts the branch name from a git ref
// e.g., "refs/heads/main" -> "main"
func extractBranchName(ref string) string {
	const prefix = "refs/heads/"
	if strings.HasPrefix(ref, prefix) {
		return strings.TrimPrefix(ref, prefix)
	}
	return ref
}

// collectChangedFiles aggregates all changed files from commits
func collectChangedFiles(commits []GitHubCommit) []string {
	fileSet := make(map[string]bool)
	for _, commit := range commits {
		for _, file := range commit.Added {
			fileSet[file] = true
		}
		for _, file := range commit.Modified {
			fileSet[file] = true
		}
		// We might also want to track removed files for cleanup
	}

	files := make([]string, 0, len(fileSet))
	for file := range fileSet {
		files = append(files, file)
	}
	return files
}

// hasAnalyzableFiles checks if any of the changed files are code files we can analyze
func hasAnalyzableFiles(files []string) bool {
	for _, file := range files {
		ext := strings.ToLower(filepath.Ext(file))
		if analyzableExtensions[ext] {
			return true
		}
	}
	return false
}

// createWebhookAnalysisJob creates a new analysis job from a webhook event
func createWebhookAnalysisJob(repoURL, branch, trigger string, changedFiles []string) (string, error) {
	jobID := uuid.New().String()

	// Build options with webhook metadata
	options := map[string]string{
		"trigger": trigger,
		"source":  "webhook",
	}

	if len(changedFiles) > 0 {
		// Store changed files (truncate if too many)
		maxFiles := 100
		if len(changedFiles) > maxFiles {
			changedFiles = changedFiles[:maxFiles]
			options["files_truncated"] = "true"
		}
		filesJSON, _ := json.Marshal(changedFiles)
		options["changed_files"] = string(filesJSON)
	}

	job := AnalysisJob{
		JobID:     jobID,
		RepoURL:   repoURL,
		Branch:    branch,
		Status:    "QUEUED",
		Options:   options,
		CreatedAt: time.Now().UTC(),
	}

	// Store job in PostgreSQL
	if err := storeJob(job); err != nil {
		return "", fmt.Errorf("failed to store job: %w", err)
	}

	// Serialize job to JSON
	jobJSON, err := json.Marshal(job)
	if err != nil {
		return "", fmt.Errorf("failed to marshal job: %w", err)
	}

	// Push job to Redis queue (high priority for webhooks)
	if err := redisClient.LPush(ctx, "analysis_queue", jobJSON).Err(); err != nil {
		return "", fmt.Errorf("failed to queue job: %w", err)
	}

	return jobID, nil
}
