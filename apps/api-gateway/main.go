package main

import (
	"bytes"
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
	"net/url"
	"os"
	"os/signal"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/gin-contrib/cors"
	"github.com/gin-gonic/gin"
	"github.com/go-redis/redis/v8"
	"github.com/google/uuid"
	"github.com/gorilla/websocket"
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

// WebhookConfig represents stored webhook configuration
type WebhookConfig struct {
	ID        int       `json:"id"`
	RepoID    int       `json:"repo_id"`
	RepoURL   string    `json:"repo_url,omitempty"`
	URL       string    `json:"url"`
	Secret    *string   `json:"secret,omitempty"`
	Events    []string  `json:"events"`
	Active    bool      `json:"active"`
	CreatedAt time.Time `json:"created_at"`
	UpdatedAt time.Time `json:"updated_at"`
}

// WebhookCreateRequest represents incoming webhook configuration
type WebhookCreateRequest struct {
	RepoID  *int     `json:"repo_id,omitempty"`
	RepoURL string   `json:"repo_url,omitempty"`
	URL     string   `json:"url"`
	Secret  string   `json:"secret,omitempty"`
	Events  []string `json:"events"`
}

type WebhookListResponse struct {
	Webhooks []WebhookConfig `json:"webhooks"`
}

// ExportRequest represents export request parameters
type ExportRequest struct {
	Formats           []string `json:"formats"`
	IncludeLLMSummary bool     `json:"include_llm_summary"`
	IncludeHeatmap    bool     `json:"include_heatmap"`
	MaxNodes          int      `json:"max_nodes"`
	MaxEdges          int      `json:"max_edges"`
}

// ExportResponse represents server-side export payload
type ExportResponse struct {
	RepoID   string                 `json:"repo_id"`
	Exports  map[string]interface{} `json:"exports"`
	Warnings []string               `json:"warnings,omitempty"`
}

// CommitHistoryItem represents a commit record stored in Postgres
type CommitHistoryItem struct {
	SHA               string   `json:"sha"`
	AuthorName        string   `json:"author_name"`
	AuthorEmail       string   `json:"author_email"`
	Message           string   `json:"message"`
	AuthoredAt        string   `json:"authored_at"`
	ChangedFiles      []string `json:"changed_files"`
	FilesChangedCount int      `json:"files_changed_count"`
}

type CommitHistoryResponse struct {
	RepoID  string              `json:"repo_id"`
	Commits []CommitHistoryItem `json:"commits"`
}

// Graph engine response types
type GraphEngineNode struct {
	ID    string                 `json:"id"`
	Label string                 `json:"label"`
	Type  string                 `json:"type"`
	Props map[string]interface{} `json:"properties"`
}

type GraphEngineEdge struct {
	Source string `json:"source"`
	Target string `json:"target"`
	Type   string `json:"type"`
}

type GraphEngineGraphResponse struct {
	Nodes      []GraphEngineNode `json:"nodes"`
	Edges      []GraphEngineEdge `json:"edges"`
	TotalNodes int               `json:"total_nodes"`
	TotalEdges int               `json:"total_edges"`
	Limit      int               `json:"limit"`
	Offset     int               `json:"offset"`
	HasMore    bool              `json:"has_more"`
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
	JobID       string            `json:"job_id"`
	RepoID      string            `json:"repo_id"` // Deterministic ID based on RepoURL
	RepoURL     string            `json:"repo_url"`
	Branch      string            `json:"branch"`
	Status      string            `json:"status"`
	Progress    int               `json:"progress"` // 0-100
	Options     map[string]string `json:"options"`
	CreatedAt   time.Time         `json:"created_at"`
	subscribers []chan JobUpdate  `json:"-"` // WebSocket subscribers for this job
}

// JobUpdate represents real-time updates sent via WebSocket
type JobUpdate struct {
	Type          string                 `json:"type"` // "progress", "status", "graph_update", "error"
	JobID         string                 `json:"job_id,omitempty"`
	RepoID        string                 `json:"repo_id,omitempty"`
	Status        string                 `json:"status,omitempty"`
	Progress      int                    `json:"progress,omitempty"`
	Message       string                 `json:"message,omitempty"`
	Error         string                 `json:"error,omitempty"`
	ChangedNodes  []string               `json:"changed_nodes,omitempty"`
	ChangedEdges  []string               `json:"changed_edges,omitempty"`
	ResultSummary map[string]interface{} `json:"result_summary,omitempty"`
	Timestamp     time.Time              `json:"timestamp"`
}

// WebSocketClient represents a connected WebSocket client
type WebSocketClient struct {
	conn     *websocket.Conn
	send     chan JobUpdate
	hub      *WebSocketHub
	jobID    string // For job-specific connections
	repoID   string // For repo-specific connections
	clientID string
}

// WebSocketHub manages WebSocket connections and message broadcasting
type WebSocketHub struct {
	clients     map[string]*WebSocketClient // clientID -> client
	jobClients  map[string]map[string]bool  // jobID -> set of clientIDs
	repoClients map[string]map[string]bool  // repoID -> set of clientIDs
	broadcast   chan JobUpdate
	register    chan *WebSocketClient
	unregister  chan *WebSocketClient
	mu          sync.RWMutex
}

// WebSocket upgrader
var upgrader = websocket.Upgrader{
	ReadBufferSize:  1024,
	WriteBufferSize: 1024,
	CheckOrigin: func(r *http.Request) bool {
		// Allow all origins for development - in production, restrict this
		return true
	},
}

// NewWebSocketHub creates a new WebSocket hub
func NewWebSocketHub() *WebSocketHub {
	return &WebSocketHub{
		clients:     make(map[string]*WebSocketClient),
		jobClients:  make(map[string]map[string]bool),
		repoClients: make(map[string]map[string]bool),
		broadcast:   make(chan JobUpdate, 256),
		register:    make(chan *WebSocketClient),
		unregister:  make(chan *WebSocketClient),
	}
}

// Run starts the WebSocket hub
func (h *WebSocketHub) Run() {
	for {
		select {
		case client := <-h.register:
			h.mu.Lock()
			h.clients[client.clientID] = client

			// Register for job-specific updates
			if client.jobID != "" {
				if h.jobClients[client.jobID] == nil {
					h.jobClients[client.jobID] = make(map[string]bool)
				}
				h.jobClients[client.jobID][client.clientID] = true
				log.Printf("🔌 WebSocket client %s registered for job %s", client.clientID, client.jobID)
			}

			// Register for repo-specific updates
			if client.repoID != "" {
				if h.repoClients[client.repoID] == nil {
					h.repoClients[client.repoID] = make(map[string]bool)
				}
				h.repoClients[client.repoID][client.clientID] = true
				log.Printf("🔌 WebSocket client %s registered for repo %s", client.clientID, client.repoID)
			}
			h.mu.Unlock()

		case client := <-h.unregister:
			h.mu.Lock()
			if _, ok := h.clients[client.clientID]; ok {
				delete(h.clients, client.clientID)
				close(client.send)

				// Unregister from job-specific updates
				if client.jobID != "" {
					delete(h.jobClients[client.jobID], client.clientID)
					if len(h.jobClients[client.jobID]) == 0 {
						delete(h.jobClients, client.jobID)
					}
				}

				// Unregister from repo-specific updates
				if client.repoID != "" {
					delete(h.repoClients[client.repoID], client.clientID)
					if len(h.repoClients[client.repoID]) == 0 {
						delete(h.repoClients, client.repoID)
					}
				}

				log.Printf("🔌 WebSocket client %s disconnected", client.clientID)
			}
			h.mu.Unlock()

		case update := <-h.broadcast:
			h.mu.RLock()
			var targetClients []*WebSocketClient

			// Determine which clients should receive this update
			if update.JobID != "" {
				// Send to clients subscribed to this job
				for clientID := range h.jobClients[update.JobID] {
					if client, ok := h.clients[clientID]; ok {
						targetClients = append(targetClients, client)
					}
				}
			}

			if update.RepoID != "" {
				// Send to clients subscribed to this repo
				for clientID := range h.repoClients[update.RepoID] {
					if client, ok := h.clients[clientID]; ok {
						// Avoid duplicates
						isDuplicate := false
						for _, tc := range targetClients {
							if tc.clientID == clientID {
								isDuplicate = true
								break
							}
						}
						if !isDuplicate {
							targetClients = append(targetClients, client)
						}
					}
				}
			}
			h.mu.RUnlock()

			// Send update to target clients
			for _, client := range targetClients {
				select {
				case client.send <- update:
				default:
					// Client send buffer is full, disconnect them
					h.mu.Lock()
					close(client.send)
					delete(h.clients, client.clientID)
					h.mu.Unlock()
				}
			}
		}
	}
}

// BroadcastJobUpdate sends an update to all clients subscribed to a job
func (h *WebSocketHub) BroadcastJobUpdate(update JobUpdate) {
	update.Timestamp = time.Now()
	h.broadcast <- update
}

// readPump handles incoming messages from the WebSocket connection
func (c *WebSocketClient) readPump() {
	defer func() {
		c.hub.unregister <- c
		c.conn.Close()
	}()

	c.conn.SetReadDeadline(time.Now().Add(60 * time.Second))
	c.conn.SetPongHandler(func(string) error {
		c.conn.SetReadDeadline(time.Now().Add(60 * time.Second))
		return nil
	})

	for {
		_, _, err := c.conn.ReadMessage()
		if err != nil {
			if websocket.IsUnexpectedCloseError(err, websocket.CloseGoingAway, websocket.CloseAbnormalClosure) {
				log.Printf("⚠️  WebSocket error: %v", err)
			}
			break
		}
	}
}

// writePump sends messages from the hub to the WebSocket connection
func (c *WebSocketClient) writePump() {
	ticker := time.NewTicker(54 * time.Second)
	defer func() {
		ticker.Stop()
		c.conn.Close()
	}()

	for {
		select {
		case update, ok := <-c.send:
			c.conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
			if !ok {
				// Channel closed, disconnect
				c.conn.WriteMessage(websocket.CloseMessage, []byte{})
				return
			}

			// Send JSON update
			if err := c.conn.WriteJSON(update); err != nil {
				log.Printf("⚠️  Error writing to WebSocket: %v", err)
				return
			}

		case <-ticker.C:
			// Send ping to keep connection alive
			c.conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
			if err := c.conn.WriteMessage(websocket.PingMessage, nil); err != nil {
				return
			}
		}
	}
}

// JobResponse represents the response after creating a job
type JobResponse struct {
	JobID     string    `json:"job_id"`
	RepoID    string    `json:"repo_id"`
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
	wsHub       *WebSocketHub
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

	// Initialize WebSocket Hub
	wsHub = NewWebSocketHub()
	go wsHub.Run()
	log.Println("🔌 WebSocket Hub initialized")

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
	redisURL := strings.TrimSpace(getEnv("REDIS_URL", "localhost:6379"))
	redisPassword := getEnv("REDIS_PASSWORD", "")

	var options *redis.Options
	if strings.HasPrefix(redisURL, "redis://") || strings.HasPrefix(redisURL, "rediss://") {
		parsed, err := redis.ParseURL(redisURL)
		if err != nil {
			log.Fatalf("Failed to parse REDIS_URL: %v", err)
		}
		if parsed.Password == "" && redisPassword != "" {
			parsed.Password = redisPassword
		}
		options = parsed
	} else {
		options = &redis.Options{
			Addr:     redisURL,
			Password: redisPassword,
			DB:       0,
		}
	}

	redisClient = redis.NewClient(options)

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

	if err := ensureCommitHistorySchema(); err != nil {
		log.Printf("⚠️  Failed to ensure commit_history schema: %v", err)
	}
}

// ensureCommitHistorySchema creates commit history storage if migrations were not applied.
func ensureCommitHistorySchema() error {
	_, err := db.Exec(`
		CREATE TABLE IF NOT EXISTS commit_history (
			id SERIAL PRIMARY KEY,
			repo_uuid VARCHAR(255) NOT NULL,
			repo_url TEXT NOT NULL,
			commit_sha VARCHAR(64) NOT NULL,
			author_name VARCHAR(255),
			author_email VARCHAR(255),
			authored_at TIMESTAMP,
			message TEXT,
			changed_files JSONB,
			files_changed_count INTEGER DEFAULT 0,
			created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
			UNIQUE(repo_uuid, commit_sha)
		);
		CREATE INDEX IF NOT EXISTS idx_commit_history_repo_uuid ON commit_history(repo_uuid);
		CREATE INDEX IF NOT EXISTS idx_commit_history_authored_at ON commit_history(authored_at DESC);
	`)
	return err
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
		AllowOriginFunc: func(origin string) bool {
			if origin == "" {
				return true
			}

			normalized := strings.ToLower(strings.TrimSpace(origin))
			switch {
			case strings.HasPrefix(normalized, "http://localhost:"),
				strings.HasPrefix(normalized, "https://localhost:"),
				strings.HasPrefix(normalized, "http://127.0.0.1:"),
				strings.HasPrefix(normalized, "https://127.0.0.1:"),
				strings.HasPrefix(normalized, "vscode-webview://"),
				strings.HasPrefix(normalized, "vscode-file://"):
				return true
			default:
				return false
			}
		},
		AllowMethods:     []string{"GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"},
		AllowHeaders:     []string{"Origin", "Content-Type", "Authorization"},
		ExposeHeaders:    []string{"Content-Length"},
		AllowCredentials: true,
		MaxAge:           12 * time.Hour,
	}))

	// Health check
	router.GET("/health", healthCheck)

	// WebSocket endpoints
	ws := router.Group("/ws")
	{
		ws.GET("/analysis/:job_id", handleJobWebSocket)
		ws.GET("/repo/:repo_id", handleRepoWebSocket)
	}

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
		v1.GET("/commits/:repo_id", listCommitHistory)

		// Webhook management
		v1.GET("/webhooks", listWebhooks)
		v1.POST("/webhooks", createWebhook)
		v1.DELETE("/webhooks/:id", deleteWebhook)
		v1.POST("/webhooks/:id/ping", pingWebhook)
	}

	// Export endpoint
	router.POST("/api/export/:repo_id", exportRepository)

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

// handleJobWebSocket handles WebSocket connections for job-specific updates
func handleJobWebSocket(c *gin.Context) {
	jobID := c.Param("job_id")

	// Validate job ID
	if !validateUUID(jobID) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid job ID"})
		return
	}

	// Upgrade HTTP connection to WebSocket
	conn, err := upgrader.Upgrade(c.Writer, c.Request, nil)
	if err != nil {
		log.Printf("⚠️  Failed to upgrade WebSocket: %v", err)
		return
	}

	// Create client
	clientID := uuid.New().String()
	client := &WebSocketClient{
		conn:     conn,
		send:     make(chan JobUpdate, 256),
		hub:      wsHub,
		jobID:    jobID,
		clientID: clientID,
	}

	// Register client
	wsHub.register <- client

	// Start goroutines for reading and writing
	go client.writePump()
	go client.readPump()

	log.Printf("✅ WebSocket connection established for job %s (client: %s)", jobID, clientID)
}

// handleRepoWebSocket handles WebSocket connections for repository-specific updates
func handleRepoWebSocket(c *gin.Context) {
	repoID := c.Param("repo_id")

	// Validate repo ID
	if !validateUUID(repoID) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid repository ID"})
		return
	}

	// Upgrade HTTP connection to WebSocket
	conn, err := upgrader.Upgrade(c.Writer, c.Request, nil)
	if err != nil {
		log.Printf("⚠️  Failed to upgrade WebSocket: %v", err)
		return
	}

	// Create client
	clientID := uuid.New().String()
	client := &WebSocketClient{
		conn:     conn,
		send:     make(chan JobUpdate, 256),
		hub:      wsHub,
		repoID:   repoID,
		clientID: clientID,
	}

	// Register client
	wsHub.register <- client

	// Start goroutines for reading and writing
	go client.writePump()
	go client.readPump()

	log.Printf("✅ WebSocket connection established for repo %s (client: %s)", repoID, clientID)
}

// generateRepoID generates a deterministic UUID v5 based on the repository URL
func generateRepoID(repoURL string) string {
	// Normalize URL: lowercase, remove .git suffix, remove trailing slash
	normalized := strings.ToLower(strings.TrimSpace(repoURL))
	normalized = strings.TrimSuffix(normalized, ".git")
	normalized = strings.TrimSuffix(normalized, "/")

	// Generate UUID v5 using NameSpaceURL and the normalized URL
	return uuid.NewSHA1(uuid.NameSpaceURL, []byte(normalized)).String()
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

	// Validate repository URL
	if !validateRepoURL(req.RepoURL) {
		validationError(c, "repo_url", "Invalid git repository URL format. Expected HTTPS, SSH, or Git URL.")
		return
	}

	// Set default branch if not provided
	if req.Branch == "" {
		req.Branch = "main"
	}

	// Validate branch name
	if !validateBranchName(req.Branch) {
		validationError(c, "branch", "Invalid branch name format. Use alphanumeric, dashes, underscores, dots, or slashes.")
		return
	}

	// Create job ID
	jobID := uuid.New().String()

	// Generate deterministic Repo ID
	repoID := generateRepoID(req.RepoURL)

	// Create job object
	job := AnalysisJob{
		JobID:     jobID,
		RepoID:    repoID,
		RepoURL:   req.RepoURL,
		Branch:    req.Branch,
		Status:    "QUEUED",
		Options:   req.Options,
		CreatedAt: time.Now().UTC(),
	}

	// Store job in PostgreSQL
	// Note: We currently don't store RepoID in Postgres as it requires schema migration
	// It is passed to Redis for the worker to use
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

	log.Printf("📝 Created analysis job: %s for repo: %s (ID: %s)", jobID, req.RepoURL, repoID)

	// Return response
	c.JSON(http.StatusCreated, JobResponse{
		JobID:     jobID,
		RepoID:    repoID,
		Status:    "QUEUED",
		Message:   "Analysis job created successfully",
		CreatedAt: job.CreatedAt,
	})
}

// getJobStatus retrieves the status of a specific job
func getJobStatus(c *gin.Context) {
	jobID := c.Param("id")
	if !validateUUID(jobID) {
		validationError(c, "id", "Invalid UUID format for job ID.")
		return
	}

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
	if !validateUUID(jobID) {
		validationError(c, "id", "Invalid UUID format for job ID.")
		return
	}

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

	// Resolve repo UUID for WebSocket broadcast and commit storage
	repoID, repoURL := resolveRepoUUID(jobID)

	// Broadcast update via WebSocket
	update := JobUpdate{
		Type:          "progress",
		JobID:         jobID,
		RepoID:        repoID,
		Status:        finalStatus,
		Progress:      0,
		ResultSummary: req.ResultSummary,
		Timestamp:     time.Now(),
	}

	if req.Progress != nil {
		update.Progress = *req.Progress
	}

	if req.Error != nil {
		update.Type = "error"
		update.Error = *req.Error
	} else if finalStatus == "COMPLETED" || finalStatus == "FAILED" {
		update.Type = "status"
	}

	if update.Type != "error" && req.ResultSummary != nil {
		if commits, err := extractCommitHistory(req.ResultSummary); err != nil {
			log.Printf("Failed to parse commit history: %v", err)
		} else if len(commits) > 0 {
			if err := storeCommitHistory(repoID, repoURL, commits); err != nil {
				log.Printf("Failed to store commit history: %v", err)
			}
		}

		if _, ok := req.ResultSummary["graph_patch"]; ok {
			update.Type = "graph_updated"
			if changedNodes, ok := req.ResultSummary["changed_nodes"].([]interface{}); ok {
				for _, node := range changedNodes {
					if id, ok := node.(string); ok {
						update.ChangedNodes = append(update.ChangedNodes, id)
					}
				}
			}
			if changedEdges, ok := req.ResultSummary["changed_edges"].([]interface{}); ok {
				for _, edge := range changedEdges {
					if id, ok := edge.(string); ok {
						update.ChangedEdges = append(update.ChangedEdges, id)
					}
				}
			}
		}
	}

	wsHub.BroadcastJobUpdate(update)

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

// listCommitHistory returns commit history for a repository UUID
func listCommitHistory(c *gin.Context) {
	repoID := c.Param("repo_id")
	if !validateUUID(repoID) {
		validationError(c, "repo_id", "Invalid repository ID.")
		return
	}

	limit := 200
	if raw := c.Query("limit"); raw != "" {
		if parsed, err := strconv.Atoi(raw); err == nil {
			if parsed > 0 && parsed <= 2000 {
				limit = parsed
			}
		}
	}

	commits, err := queryCommitHistoryTable(repoID, limit)
	if err != nil {
		log.Printf("Database error reading commit_history table (repo=%s): %v", repoID, err)

		fallbackCommits, fallbackErr := queryCommitHistoryFromJobSummaries(repoID, limit)
		if fallbackErr != nil {
			log.Printf("Fallback error reading commit history from job summaries (repo=%s): %v", repoID, fallbackErr)
			c.JSON(http.StatusInternalServerError, gin.H{
				"error": "Failed to retrieve commit history",
			})
			return
		}

		log.Printf("ℹ️ Using commit history fallback from analysis_jobs for repo %s (%d commits)", repoID, len(fallbackCommits))
		c.JSON(http.StatusOK, CommitHistoryResponse{
			RepoID:  repoID,
			Commits: fallbackCommits,
		})
		return
	}

	// If table is available but empty, try reading the latest worker payloads as a backup.
	if len(commits) == 0 {
		if fallbackCommits, fallbackErr := queryCommitHistoryFromJobSummaries(repoID, limit); fallbackErr == nil && len(fallbackCommits) > 0 {
			commits = fallbackCommits
		}
	}

	c.JSON(http.StatusOK, CommitHistoryResponse{
		RepoID:  repoID,
		Commits: commits,
	})
}

func queryCommitHistoryTable(repoID string, limit int) ([]CommitHistoryItem, error) {
	rows, err := db.Query(`
		SELECT commit_sha, author_name, author_email, authored_at, message, changed_files, files_changed_count
		FROM commit_history
		WHERE repo_uuid = $1
		ORDER BY authored_at DESC NULLS LAST
		LIMIT $2
	`, repoID, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	commits := make([]CommitHistoryItem, 0, limit)
	for rows.Next() {
		var sha string
		var authorName sql.NullString
		var authorEmail sql.NullString
		var authoredAt sql.NullTime
		var message sql.NullString
		var changedFilesJSON []byte
		var filesChangedCount int

		if err := rows.Scan(&sha, &authorName, &authorEmail, &authoredAt, &message, &changedFilesJSON, &filesChangedCount); err != nil {
			log.Printf("Scan error: %v", err)
			continue
		}

		var changedFiles []string
		if len(changedFilesJSON) > 0 {
			_ = json.Unmarshal(changedFilesJSON, &changedFiles)
		}

		commit := CommitHistoryItem{
			SHA:               sha,
			AuthorName:        authorName.String,
			AuthorEmail:       authorEmail.String,
			Message:           message.String,
			AuthoredAt:        "",
			ChangedFiles:      changedFiles,
			FilesChangedCount: filesChangedCount,
		}
		if authoredAt.Valid {
			commit.AuthoredAt = authoredAt.Time.UTC().Format(time.RFC3339)
		}
		if commit.FilesChangedCount == 0 {
			commit.FilesChangedCount = len(commit.ChangedFiles)
		}
		commits = append(commits, commit)
	}

	if err := rows.Err(); err != nil {
		return nil, err
	}

	return commits, nil
}

func queryCommitHistoryFromJobSummaries(repoID string, limit int) ([]CommitHistoryItem, error) {
	rows, err := db.Query(`
		SELECT repo_url, result_summary
		FROM analysis_jobs
		WHERE result_summary IS NOT NULL
		ORDER BY updated_at DESC NULLS LAST, created_at DESC
		LIMIT 250
	`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	seenSHAs := make(map[string]bool, limit)
	commits := make([]CommitHistoryItem, 0, limit)

	for rows.Next() {
		var repoURL string
		var summaryJSON []byte
		if err := rows.Scan(&repoURL, &summaryJSON); err != nil {
			log.Printf("Scan error reading job summary fallback: %v", err)
			continue
		}

		if generateRepoID(repoURL) != repoID {
			continue
		}

		var summary map[string]interface{}
		if err := json.Unmarshal(summaryJSON, &summary); err != nil {
			log.Printf("JSON decode error in job summary fallback: %v", err)
			continue
		}

		parsedCommits, err := extractCommitHistory(summary)
		if err != nil {
			log.Printf("Commit history parse error in fallback: %v", err)
			continue
		}

		for _, commit := range parsedCommits {
			if commit.SHA == "" || seenSHAs[commit.SHA] {
				continue
			}
			if commit.FilesChangedCount == 0 {
				commit.FilesChangedCount = len(commit.ChangedFiles)
			}
			seenSHAs[commit.SHA] = true
			commits = append(commits, commit)
			if len(commits) >= limit {
				break
			}
		}

		if len(commits) >= limit {
			break
		}
	}

	if err := rows.Err(); err != nil {
		return nil, err
	}

	sort.SliceStable(commits, func(i, j int) bool {
		left := parseCommitTime(commits[i].AuthoredAt)
		right := parseCommitTime(commits[j].AuthoredAt)
		if left.Valid && right.Valid {
			return left.Time.After(right.Time)
		}
		return left.Valid && !right.Valid
	})

	return commits, nil
}

// listWebhooks returns configured webhooks
func listWebhooks(c *gin.Context) {
	rows, err := db.Query(`
		SELECT w.id, w.repo_id, r.url, w.url, w.secret, w.events, w.active, w.created_at, w.updated_at
		FROM webhooks w
		JOIN repositories r ON w.repo_id = r.id
		ORDER BY w.created_at DESC
	`)
	if err != nil {
		log.Printf("Database error: %v", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": "Failed to retrieve webhooks",
		})
		return
	}
	defer rows.Close()

	webhooks := []WebhookConfig{}
	for rows.Next() {
		var hook WebhookConfig
		var secret sql.NullString
		var eventsJSON []byte
		if err := rows.Scan(&hook.ID, &hook.RepoID, &hook.RepoURL, &hook.URL, &secret, &eventsJSON, &hook.Active, &hook.CreatedAt, &hook.UpdatedAt); err != nil {
			log.Printf("Scan error: %v", err)
			continue
		}
		if secret.Valid {
			value := secret.String
			hook.Secret = &value
		}
		if len(eventsJSON) > 0 {
			_ = json.Unmarshal(eventsJSON, &hook.Events)
		}
		webhooks = append(webhooks, hook)
	}

	c.JSON(http.StatusOK, WebhookListResponse{Webhooks: webhooks})
}

// createWebhook stores a webhook configuration
func createWebhook(c *gin.Context) {
	var req WebhookCreateRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{
			"error":   "Invalid request body",
			"details": err.Error(),
		})
		return
	}

	if req.URL == "" {
		validationError(c, "url", "Webhook URL is required")
		return
	}

	if req.RepoID == nil && req.RepoURL == "" {
		validationError(c, "repo_url", "Repository URL is required")
		return
	}

	if req.RepoURL != "" && !validateRepoURL(req.RepoURL) {
		validationError(c, "repo_url", "Invalid git repository URL format")
		return
	}

	repoID := 0
	if req.RepoID != nil {
		repoID = *req.RepoID
	} else {
		id, err := getOrCreateRepository(req.RepoURL)
		if err != nil {
			log.Printf("Repository error: %v", err)
			c.JSON(http.StatusInternalServerError, gin.H{
				"error": "Failed to resolve repository",
			})
			return
		}
		repoID = id
	}

	events := req.Events
	if len(events) == 0 {
		events = []string{"push", "pull_request"}
	}
	var eventsJSON []byte
	eventsJSON, _ = json.Marshal(events)

	var createdAt time.Time
	var updatedAt time.Time
	var id int
	var secret *string
	if req.Secret != "" {
		secret = &req.Secret
	}

	err := db.QueryRow(`
		INSERT INTO webhooks (user_id, repo_id, url, secret, events, active)
		VALUES ($1, $2, $3, $4, $5, true)
		RETURNING id, created_at, updated_at
	`, 1, repoID, req.URL, req.Secret, eventsJSON).Scan(&id, &createdAt, &updatedAt)

	if err != nil {
		log.Printf("Database error: %v", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": "Failed to create webhook",
		})
		return
	}

	repoURL := req.RepoURL
	if repoURL == "" {
		repoURL = lookupRepoURL(repoID)
	}

	c.JSON(http.StatusCreated, WebhookConfig{
		ID:        id,
		RepoID:    repoID,
		RepoURL:   repoURL,
		URL:       req.URL,
		Secret:    secret,
		Events:    events,
		Active:    true,
		CreatedAt: createdAt,
		UpdatedAt: updatedAt,
	})
}

// deleteWebhook deletes a webhook by ID
func deleteWebhook(c *gin.Context) {
	idParam := c.Param("id")
	id, err := strconv.Atoi(idParam)
	if err != nil {
		validationError(c, "id", "Invalid webhook ID")
		return
	}

	result, err := db.Exec("DELETE FROM webhooks WHERE id = $1", id)
	if err != nil {
		log.Printf("Database error: %v", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": "Failed to delete webhook",
		})
		return
	}

	rows, _ := result.RowsAffected()
	if rows == 0 {
		c.JSON(http.StatusNotFound, gin.H{
			"error": "Webhook not found",
		})
		return
	}

	c.Status(http.StatusNoContent)
}

// pingWebhook sends a test payload to the webhook URL
func pingWebhook(c *gin.Context) {
	idParam := c.Param("id")
	id, err := strconv.Atoi(idParam)
	if err != nil {
		validationError(c, "id", "Invalid webhook ID")
		return
	}

	var url string
	var secret sql.NullString
	err = db.QueryRow("SELECT url, secret FROM webhooks WHERE id = $1", id).Scan(&url, &secret)
	if err == sql.ErrNoRows {
		c.JSON(http.StatusNotFound, gin.H{
			"error": "Webhook not found",
		})
		return
	} else if err != nil {
		log.Printf("Database error: %v", err)
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": "Failed to load webhook",
		})
		return
	}

	payload := map[string]interface{}{
		"zen":     "ArchMind webhook test",
		"hook_id": id,
		"sent_at": time.Now().UTC().Format(time.RFC3339),
	}
	body, _ := json.Marshal(payload)

	req, err := http.NewRequest(http.MethodPost, url, bytes.NewBuffer(body))
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{
			"error": "Failed to build ping request",
		})
		return
	}

	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-GitHub-Event", "ping")
	req.Header.Set("X-GitHub-Delivery", uuid.New().String())
	req.Header.Set("User-Agent", "ArchMind-Webhook")
	if secret.Valid {
		signature := signGitHubPayload(body, secret.String)
		if signature != "" {
			req.Header.Set("X-Hub-Signature-256", signature)
		}
	}

	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{
			"error": "Webhook ping failed",
		})
		return
	}
	defer resp.Body.Close()

	if resp.StatusCode >= 300 {
		bodyText, _ := io.ReadAll(resp.Body)
		c.JSON(http.StatusBadGateway, gin.H{
			"error":   "Webhook ping returned error",
			"details": string(bodyText),
			"status":  resp.StatusCode,
		})
		return
	}

	c.Status(http.StatusNoContent)
}

// exportRepository builds export payloads from graph engine data
func exportRepository(c *gin.Context) {
	repoID := c.Param("repo_id")
	if !validateUUID(repoID) {
		validationError(c, "repo_id", "Invalid repository ID")
		return
	}

	var req ExportRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		req.Formats = []string{}
	}

	formats := req.Formats
	if len(formats) == 0 {
		formats = []string{"json", "mermaid", "plantuml"}
	}

	maxNodes := req.MaxNodes
	if maxNodes <= 0 {
		maxNodes = 5000
	}

	graphURL := getEnv("GRAPH_ENGINE_URL", "http://localhost:8000")

	graph, warnings, err := fetchGraphEngineGraph(graphURL, repoID, maxNodes)
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{
			"error":   "Failed to fetch graph",
			"details": err.Error(),
		})
		return
	}

	exports := map[string]interface{}{}
	if containsFormat(formats, "json") {
		exports["json"] = graph
	}
	if containsFormat(formats, "mermaid") {
		exports["mermaid"] = buildMermaid(graph)
	}
	if containsFormat(formats, "plantuml") {
		exports["plantuml"] = buildPlantUML(graph)
	}
	if containsFormat(formats, "markdown") {
		exports["markdown"] = buildMarkdownExport(graph)
	}

	if req.IncludeHeatmap {
		heatmap, err := fetchGraphEngineJSON(graphURL, fmt.Sprintf("/api/graph/%s/contributions", repoID))
		if err != nil {
			warnings = append(warnings, "heatmap_unavailable")
		} else {
			exports["heatmap"] = heatmap
		}
	}

	if req.IncludeLLMSummary {
		insights, err := fetchGraphEngineJSON(graphURL, fmt.Sprintf("/api/analyze/%s/architecture", repoID))
		if err != nil {
			warnings = append(warnings, "llm_summary_unavailable")
		} else {
			exports["llm_summary"] = insights
		}
	}

	c.JSON(http.StatusOK, ExportResponse{
		RepoID:   repoID,
		Exports:  exports,
		Warnings: warnings,
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

func resolveRepoUUID(jobID string) (string, string) {
	var repoURL string
	if err := db.QueryRow("SELECT repo_url FROM analysis_jobs WHERE job_id = $1", jobID).Scan(&repoURL); err != nil {
		return "", ""
	}
	return generateRepoID(repoURL), repoURL
}

func extractCommitHistory(summary map[string]interface{}) ([]CommitHistoryItem, error) {
	raw, ok := summary["commit_history"]
	if !ok {
		return nil, nil
	}

	payload, err := json.Marshal(raw)
	if err != nil {
		return nil, err
	}

	var commits []CommitHistoryItem
	if err := json.Unmarshal(payload, &commits); err != nil {
		return nil, err
	}
	return commits, nil
}

func parseCommitTime(value string) sql.NullTime {
	if value == "" {
		return sql.NullTime{}
	}
	if parsed, err := time.Parse(time.RFC3339Nano, value); err == nil {
		return sql.NullTime{Time: parsed.UTC(), Valid: true}
	}
	if parsed, err := time.Parse(time.RFC3339, value); err == nil {
		return sql.NullTime{Time: parsed.UTC(), Valid: true}
	}
	return sql.NullTime{}
}

func storeCommitHistory(repoID string, repoURL string, commits []CommitHistoryItem) error {
	if repoID == "" || len(commits) == 0 {
		return nil
	}

	tx, err := db.Begin()
	if err != nil {
		return err
	}
	defer func() {
		if err != nil {
			_ = tx.Rollback()
		}
	}()

	stmt, err := tx.Prepare(`
		INSERT INTO commit_history (
			repo_uuid,
			repo_url,
			commit_sha,
			author_name,
			author_email,
			authored_at,
			message,
			changed_files,
			files_changed_count
		) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
		ON CONFLICT (repo_uuid, commit_sha)
		DO UPDATE SET
			author_name = EXCLUDED.author_name,
			author_email = EXCLUDED.author_email,
			authored_at = EXCLUDED.authored_at,
			message = EXCLUDED.message,
			changed_files = EXCLUDED.changed_files,
			files_changed_count = EXCLUDED.files_changed_count
	`)
	if err != nil {
		_ = tx.Rollback()
		return err
	}
	defer stmt.Close()

	for _, commit := range commits {
		filesChangedCount := commit.FilesChangedCount
		if filesChangedCount == 0 {
			filesChangedCount = len(commit.ChangedFiles)
		}

		changedFilesJSON, _ := json.Marshal(commit.ChangedFiles)
		authoredAt := parseCommitTime(commit.AuthoredAt)

		if _, err = stmt.Exec(
			repoID,
			repoURL,
			commit.SHA,
			commit.AuthorName,
			commit.AuthorEmail,
			authoredAt,
			commit.Message,
			changedFilesJSON,
			filesChangedCount,
		); err != nil {
			_ = tx.Rollback()
			return err
		}
	}

	return tx.Commit()
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

	// Step 2: Determine event type and resolve secret
	eventType := c.GetHeader("X-GitHub-Event")
	contentType := c.GetHeader("Content-Type")

	payloadBody := body
	if eventType == "push" || eventType == "pull_request" || eventType == "ping" {
		payloadBody, err = extractGitHubJSONPayload(body, contentType)
		if err != nil {
			log.Printf("❌ Webhook: Failed to decode payload (event=%s, content_type=%s): %v", eventType, contentType, err)
			c.JSON(http.StatusBadRequest, WebhookResponse{
				Status:  "error",
				Message: "Invalid webhook payload format",
			})
			return
		}
	}

	secretOverride := resolveWebhookSecret(eventType, payloadBody)

	// Step 3: Verify the signature (security check)
	signature := c.GetHeader("X-Hub-Signature-256")
	if !verifyGitHubSignature(body, signature, secretOverride) {
		log.Printf("❌ Webhook: Invalid signature from IP: %s", c.ClientIP())
		c.JSON(http.StatusUnauthorized, WebhookResponse{
			Status:  "error",
			Message: "Invalid signature",
		})
		return
	}

	// Step 4: Check the event type
	deliveryID := c.GetHeader("X-GitHub-Delivery")

	log.Printf("📥 Webhook received: event=%s, delivery=%s", eventType, deliveryID)

	// Step 5: Route to appropriate handler based on event type
	switch eventType {
	case "push":
		handlePushEvent(c, payloadBody)
	case "pull_request":
		handlePullRequestEvent(c, payloadBody)
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
func verifyGitHubSignature(payload []byte, signature string, secretOverride string) bool {
	secret := secretOverride
	if secret == "" {
		secret = getEnv("GITHUB_WEBHOOK_SECRET", "")
	}
	if secret == "" {
		log.Println("⚠️ Warning: webhook secret not set, skipping signature verification")
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

// resolveWebhookSecret attempts to resolve a repo-specific webhook secret
func resolveWebhookSecret(eventType string, payload []byte) string {
	var repoURL string

	switch eventType {
	case "push":
		var push GitHubPushPayload
		if err := json.Unmarshal(payload, &push); err == nil {
			repoURL = push.Repository.CloneURL
		}
	case "pull_request":
		var pr GitHubPullRequestPayload
		if err := json.Unmarshal(payload, &pr); err == nil {
			repoURL = pr.Repository.CloneURL
		}
	case "ping":
		var ping struct {
			Repository GitHubRepository `json:"repository"`
		}
		if err := json.Unmarshal(payload, &ping); err == nil {
			repoURL = ping.Repository.CloneURL
		}
	}

	if repoURL == "" {
		return ""
	}

	repoURL = normalizeRepoURL(repoURL)
	var secret sql.NullString
	err := db.QueryRow(`
		SELECT w.secret
		FROM webhooks w
		JOIN repositories r ON w.repo_id = r.id
		WHERE r.url = $1 AND w.active = true
		ORDER BY w.id DESC
		LIMIT 1
	`, repoURL).Scan(&secret)
	if err != nil || !secret.Valid {
		return ""
	}

	return secret.String
}

func extractGitHubJSONPayload(body []byte, contentType string) ([]byte, error) {
	trimmed := bytes.TrimSpace(body)
	if len(trimmed) == 0 {
		return nil, fmt.Errorf("empty request body")
	}

	if json.Valid(trimmed) {
		return trimmed, nil
	}

	mediaType := strings.ToLower(strings.TrimSpace(strings.Split(contentType, ";")[0]))
	if mediaType == "application/x-www-form-urlencoded" || bytes.HasPrefix(trimmed, []byte("payload=")) {
		values, err := url.ParseQuery(string(body))
		if err != nil {
			return nil, fmt.Errorf("failed to parse form payload: %w", err)
		}

		payload := strings.TrimSpace(values.Get("payload"))
		if payload == "" {
			return nil, fmt.Errorf("missing payload field in form body")
		}

		payloadBytes := []byte(payload)
		if !json.Valid(payloadBytes) {
			return nil, fmt.Errorf("decoded payload is not valid JSON")
		}
		return payloadBytes, nil
	}

	return nil, fmt.Errorf("unsupported payload format: %s", mediaType)
}

func signGitHubPayload(payload []byte, secret string) string {
	if secret == "" {
		return ""
	}
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write(payload)
	return "sha256=" + hex.EncodeToString(mac.Sum(nil))
}

func normalizeRepoURL(repoURL string) string {
	normalized := strings.ToLower(strings.TrimSpace(repoURL))
	normalized = strings.TrimSuffix(normalized, ".git")
	normalized = strings.TrimSuffix(normalized, "/")
	return normalized
}

func parseRepoName(repoURL string) string {
	trimmed := normalizeRepoURL(repoURL)
	parts := strings.Split(trimmed, "/")
	if len(parts) == 0 {
		return "repository"
	}
	return parts[len(parts)-1]
}

func getOrCreateRepository(repoURL string) (int, error) {
	normalized := normalizeRepoURL(repoURL)
	var id int
	err := db.QueryRow("SELECT id FROM repositories WHERE url = $1", normalized).Scan(&id)
	if err == nil {
		return id, nil
	}
	if err != sql.ErrNoRows {
		return 0, err
	}

	name := parseRepoName(normalized)
	err = db.QueryRow(`
		INSERT INTO repositories (url, owner_id, name)
		VALUES ($1, $2, $3)
		RETURNING id
	`, normalized, 1, name).Scan(&id)
	if err != nil {
		return 0, err
	}
	return id, nil
}

func lookupRepoURL(repoID int) string {
	var url string
	if err := db.QueryRow("SELECT url FROM repositories WHERE id = $1", repoID).Scan(&url); err != nil {
		return ""
	}
	return url
}

func fetchGraphEngineJSON(baseURL, endpoint string) (map[string]interface{}, error) {
	url := strings.TrimRight(baseURL, "/") + endpoint
	resp, err := http.Get(url)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode >= 300 {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("graph engine error: %s", string(body))
	}
	var payload map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&payload); err != nil {
		return nil, err
	}
	return payload, nil
}

func fetchGraphEngineGraph(baseURL, repoID string, maxNodes int) (*GraphEngineGraphResponse, []string, error) {
	warnings := []string{}
	limit := maxNodes
	if limit <= 0 {
		limit = 5000
	}
	url := fmt.Sprintf("%s/api/graph/%s?limit=%d&offset=0", strings.TrimRight(baseURL, "/"), repoID, limit)
	resp, err := http.Get(url)
	if err != nil {
		return nil, warnings, err
	}
	defer resp.Body.Close()
	if resp.StatusCode >= 300 {
		body, _ := io.ReadAll(resp.Body)
		return nil, warnings, fmt.Errorf("graph engine error: %s", string(body))
	}
	var graph GraphEngineGraphResponse
	if err := json.NewDecoder(resp.Body).Decode(&graph); err != nil {
		return nil, warnings, err
	}
	if graph.HasMore {
		warnings = append(warnings, "graph_truncated")
	}
	return &graph, warnings, nil
}

func containsFormat(formats []string, target string) bool {
	for _, format := range formats {
		if strings.EqualFold(format, target) {
			return true
		}
	}
	return false
}

func sanitizeDiagramID(raw string) string {
	if raw == "" {
		return "node"
	}
	cleaned := regexp.MustCompile(`[^a-zA-Z0-9_]`).ReplaceAllString(raw, "_")
	if cleaned == "" {
		cleaned = "node"
	}
	if cleaned[0] >= '0' && cleaned[0] <= '9' {
		cleaned = "n_" + cleaned
	}
	return cleaned
}

func buildMermaid(graph *GraphEngineGraphResponse) string {
	if graph == nil {
		return "graph TD"
	}
	lines := []string{"graph TD"}
	idMap := map[string]string{}
	used := map[string]int{}
	for _, node := range graph.Nodes {
		base := sanitizeDiagramID(node.ID)
		if count, ok := used[base]; ok {
			count++
			used[base] = count
			base = fmt.Sprintf("%s_%d", base, count)
		} else {
			used[base] = 1
		}
		idMap[node.ID] = base
		label := strings.ReplaceAll(node.Label, "\"", "'")
		lines = append(lines, fmt.Sprintf("%s[\"%s\"]", base, label))
	}
	for _, edge := range graph.Edges {
		source := idMap[edge.Source]
		target := idMap[edge.Target]
		if source == "" || target == "" {
			continue
		}
		lines = append(lines, fmt.Sprintf("%s --> %s", source, target))
	}
	return strings.Join(lines, "\n")
}

func buildPlantUML(graph *GraphEngineGraphResponse) string {
	if graph == nil {
		return "@startuml\n@enduml"
	}
	lines := []string{"@startuml"}
	idMap := map[string]string{}
	used := map[string]int{}
	for _, node := range graph.Nodes {
		base := sanitizeDiagramID(node.ID)
		if count, ok := used[base]; ok {
			count++
			used[base] = count
			base = fmt.Sprintf("%s_%d", base, count)
		} else {
			used[base] = 1
		}
		idMap[node.ID] = base
		label := strings.ReplaceAll(node.Label, "\"", "'")
		lines = append(lines, fmt.Sprintf("object %s as \"%s\"", base, label))
	}
	for _, edge := range graph.Edges {
		source := idMap[edge.Source]
		target := idMap[edge.Target]
		if source == "" || target == "" {
			continue
		}
		lines = append(lines, fmt.Sprintf("%s --> %s", source, target))
	}
	lines = append(lines, "@enduml")
	return strings.Join(lines, "\n")
}

func buildMarkdownExport(graph *GraphEngineGraphResponse) string {
	mermaid := buildMermaid(graph)
	return strings.Join([]string{
		"# Architecture Export",
		"",
		"## Mermaid Graph",
		"```mermaid",
		mermaid,
		"```",
	}, "\n")
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
	removedFiles := collectRemovedFiles(payload.Commits)
	allChanged := append(changedFiles, removedFiles...)
	if !hasAnalyzableFiles(allChanged) {
		log.Printf("ℹ️ Webhook: No analyzable files changed, skipping analysis")
		c.JSON(http.StatusOK, WebhookResponse{
			Status:  "skipped",
			Message: "No analyzable code files were changed",
		})
		return
	}

	// Create and queue analysis job
	jobID, err := createWebhookAnalysisJob(payload.Repository.CloneURL, branch, "push", changedFiles, removedFiles)
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
		nil,
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

// collectRemovedFiles aggregates removed files from commits
func collectRemovedFiles(commits []GitHubCommit) []string {
	fileSet := make(map[string]bool)
	for _, commit := range commits {
		for _, file := range commit.Removed {
			fileSet[file] = true
		}
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
func createWebhookAnalysisJob(repoURL, branch, trigger string, changedFiles []string, removedFiles []string) (string, error) {
	jobID := uuid.New().String()
	repoID := generateRepoID(repoURL)

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
		options["incremental"] = "true"
		options["analysis_mode"] = "incremental"
	}

	if len(removedFiles) > 0 {
		maxFiles := 100
		if len(removedFiles) > maxFiles {
			removedFiles = removedFiles[:maxFiles]
			options["removed_files_truncated"] = "true"
		}
		removedJSON, _ := json.Marshal(removedFiles)
		options["removed_files"] = string(removedJSON)
		options["incremental"] = "true"
		options["analysis_mode"] = "incremental"
	}

	job := AnalysisJob{
		JobID:     jobID,
		RepoID:    repoID,
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

// validateRepoURL validates if the repository URL is a valid Git URL
func validateRepoURL(url string) bool {
	// Regex for standard HTTPS, SSH, and Git URLs
	// Supports:
	// - https://github.com/user/repo
	// - https://github.com/user/repo.git
	// - git@github.com:user/repo.git
	// - ssh://user@server/project.git
	regex := regexp.MustCompile(`^(?:https?://|git@|ssh://)(?:[^@]+@)?[^/]+(?:[:/][^/]+)+(\.git)?$`)
	return regex.MatchString(url)
}

// validateBranchName validates if the branch name is safe and valid
func validateBranchName(branch string) bool {
	// Allow alphanumeric, dashes, underscores, slashes, and dots
	// Disallow dangerous characters like spaces, control chars, ~, ^, :, *, ?, [, \
	if len(branch) == 0 || len(branch) > 255 {
		return false
	}
	// Simplified git branch validation
	regex := regexp.MustCompile(`^[a-zA-Z0-9_\-\./]+$`)

	// Check for ".." which can be used for directory traversal in some contexts
	if strings.Contains(branch, "..") {
		return false
	}

	return regex.MatchString(branch)
}

// validateUUID checks if the string is a valid UUID
func validateUUID(id string) bool {
	_, err := uuid.Parse(id)
	return err == nil
}

// validationError sends a formatted 400 Bad Request response
func validationError(c *gin.Context, field, message string) {
	c.JSON(http.StatusBadRequest, gin.H{
		"error":   "Validation Error",
		"field":   field,
		"message": message,
	})
}
