# Test Script for PATCH /api/v1/jobs/:id Endpoint
# This script tests all required functionality of the job update endpoint

$baseUrl = "http://localhost:8080"
$testResults = @()

# Helper function to make HTTP requests
function Invoke-APIRequest {
    param(
        [string]$Method,
        [string]$Uri,
        [object]$Body
    )
    
    try {
        $params = @{
            Uri = $Uri
            Method = $Method
            ContentType = "application/json"
            UseBasicParsing = $true
        }
        
        if ($Body) {
            $params.Body = ($Body | ConvertTo-Json -Depth 10)
        }
        
        $response = Invoke-WebRequest @params
        return @{
            Success = $true
            StatusCode = $response.StatusCode
            Content = ($response.Content | ConvertFrom-Json)
        }
    }
    catch {
        return @{
            Success = $false
            StatusCode = $_.Exception.Response.StatusCode.value__
            Error = $_.Exception.Message
            Content = $null
        }
    }
}

# Helper function to log test results
function Add-TestResult {
    param(
        [string]$TestName,
        [bool]$Passed,
        [string]$Details
    )
    
    $script:testResults += @{
        Test = $TestName
        Passed = $Passed
        Details = $Details
    }
    
    $status = if ($Passed) { "✅ PASS" } else { "❌ FAIL" }
    Write-Host "$status - $TestName"
    if ($Details) {
        Write-Host "  Details: $Details"
    }
}

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Testing PATCH /api/v1/jobs/:id Endpoint" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Test 0: Health Check
Write-Host "Test 0: Checking API Gateway Health..." -ForegroundColor Yellow
$health = Invoke-APIRequest -Method GET -Uri "$baseUrl/health"
if ($health.Success) {
    Add-TestResult -TestName "API Gateway Health Check" -Passed $true -Details "API is running"
} else {
    Add-TestResult -TestName "API Gateway Health Check" -Passed $false -Details "API is not responding"
    Write-Host "❌ API Gateway is not running. Please start it first." -ForegroundColor Red
    exit 1
}

# Test 1: Create a test job first
Write-Host "`nTest 1: Creating test job..." -ForegroundColor Yellow
$createJob = Invoke-APIRequest -Method POST -Uri "$baseUrl/api/v1/analyze" -Body @{
    repo_url = "https://github.com/test/repo"
    branch = "main"
}

if ($createJob.Success -and $createJob.Content.job_id) {
    $jobId = $createJob.Content.job_id
    Add-TestResult -TestName "Create Test Job" -Passed $true -Details "Job ID: $jobId"
} else {
    Add-TestResult -TestName "Create Test Job" -Passed $false -Details "Failed to create job"
    exit 1
}

# Test 2: Update job status from QUEUED to PROCESSING
Write-Host "`nTest 2: Testing status transition QUEUED → PROCESSING..." -ForegroundColor Yellow
$updateToProcessing = Invoke-APIRequest -Method PATCH -Uri "$baseUrl/api/v1/jobs/$jobId" -Body @{
    status = "PROCESSING"
    progress = 10
}

if ($updateToProcessing.Success -and $updateToProcessing.StatusCode -eq 200) {
    Add-TestResult -TestName "Status Transition: QUEUED → PROCESSING" -Passed $true -Details "Status updated successfully"
} else {
    Add-TestResult -TestName "Status Transition: QUEUED → PROCESSING" -Passed $false -Details "Status code: $($updateToProcessing.StatusCode)"
}

# Test 3: Update progress only
Write-Host "`nTest 3: Testing progress update..." -ForegroundColor Yellow
$updateProgress = Invoke-APIRequest -Method PATCH -Uri "$baseUrl/api/v1/jobs/$jobId" -Body @{
    progress = 50
}

if ($updateProgress.Success -and $updateProgress.StatusCode -eq 200) {
    Add-TestResult -TestName "Progress Update" -Passed $true -Details "Progress updated to 50%"
} else {
    Add-TestResult -TestName "Progress Update" -Passed $false -Details "Status code: $($updateProgress.StatusCode)"
}

# Test 4: Update result_summary
Write-Host "`nTest 4: Testing result_summary update..." -ForegroundColor Yellow
$updateSummary = Invoke-APIRequest -Method PATCH -Uri "$baseUrl/api/v1/jobs/$jobId" -Body @{
    result_summary = @{
        files_analyzed = 42
        issues_found = 5
        total_lines = 10000
    }
}

if ($updateSummary.Success -and $updateSummary.StatusCode -eq 200) {
    Add-TestResult -TestName "Result Summary Update" -Passed $true -Details "Summary updated with JSON data"
} else {
    Add-TestResult -TestName "Result Summary Update" -Passed $false -Details "Status code: $($updateSummary.StatusCode)"
}

# Test 5: Update status to COMPLETED
Write-Host "`nTest 5: Testing status transition PROCESSING → COMPLETED..." -ForegroundColor Yellow
$updateToCompleted = Invoke-APIRequest -Method PATCH -Uri "$baseUrl/api/v1/jobs/$jobId" -Body @{
    status = "COMPLETED"
    progress = 100
}

if ($updateToCompleted.Success -and $updateToCompleted.StatusCode -eq 200) {
    Add-TestResult -TestName "Status Transition: PROCESSING → COMPLETED" -Passed $true -Details "Job marked as completed"
} else {
    Add-TestResult -TestName "Status Transition: PROCESSING → COMPLETED" -Passed $false -Details "Status code: $($updateToCompleted.StatusCode)"
}

# Test 6: Try invalid transition (COMPLETED → PROCESSING)
Write-Host "`nTest 6: Testing invalid status transition (COMPLETED → PROCESSING)..." -ForegroundColor Yellow
$invalidTransition = Invoke-APIRequest -Method PATCH -Uri "$baseUrl/api/v1/jobs/$jobId" -Body @{
    status = "PROCESSING"
}

if (-not $invalidTransition.Success -and $invalidTransition.StatusCode -eq 400) {
    Add-TestResult -TestName "Invalid Status Transition Blocked" -Passed $true -Details "Correctly rejected invalid transition"
} else {
    Add-TestResult -TestName "Invalid Status Transition Blocked" -Passed $false -Details "Should have returned 400, got: $($invalidTransition.StatusCode)"
}

# Test 7: Create another job for FAILED status test
Write-Host "`nTest 7: Testing status transition to FAILED..." -ForegroundColor Yellow
$createJob2 = Invoke-APIRequest -Method POST -Uri "$baseUrl/api/v1/analyze" -Body @{
    repo_url = "https://github.com/test/repo2"
    branch = "develop"
}

if ($createJob2.Success) {
    $jobId2 = $createJob2.Content.job_id
    
    # Update to PROCESSING first
    Invoke-APIRequest -Method PATCH -Uri "$baseUrl/api/v1/jobs/$jobId2" -Body @{
        status = "PROCESSING"
    } | Out-Null
    
    # Update to FAILED with error message
    $updateToFailed = Invoke-APIRequest -Method PATCH -Uri "$baseUrl/api/v1/jobs/$jobId2" -Body @{
        status = "FAILED"
        error = "Repository clone failed: timeout"
    }
    
    if ($updateToFailed.Success -and $updateToFailed.StatusCode -eq 200) {
        Add-TestResult -TestName "Status Transition: PROCESSING → FAILED" -Passed $true -Details "Job marked as failed with error message"
    } else {
        Add-TestResult -TestName "Status Transition: PROCESSING → FAILED" -Passed $false -Details "Status code: $($updateToFailed.StatusCode)"
    }
}

# Test 8: Invalid progress value
Write-Host "`nTest 8: Testing invalid progress value (>100)..." -ForegroundColor Yellow
$createJob3 = Invoke-APIRequest -Method POST -Uri "$baseUrl/api/v1/analyze" -Body @{
    repo_url = "https://github.com/test/repo3"
    branch = "main"
}

if ($createJob3.Success) {
    $jobId3 = $createJob3.Content.job_id
    $invalidProgress = Invoke-APIRequest -Method PATCH -Uri "$baseUrl/api/v1/jobs/$jobId3" -Body @{
        progress = 150
    }
    
    if (-not $invalidProgress.Success -and $invalidProgress.StatusCode -eq 400) {
        Add-TestResult -TestName "Invalid Progress Value Rejected" -Passed $true -Details "Correctly rejected progress > 100"
    } else {
        Add-TestResult -TestName "Invalid Progress Value Rejected" -Passed $false -Details "Should have returned 400, got: $($invalidProgress.StatusCode)"
    }
}

# Test 9: Non-existent job
Write-Host "`nTest 9: Testing update to non-existent job..." -ForegroundColor Yellow
$nonExistentJob = Invoke-APIRequest -Method PATCH -Uri "$baseUrl/api/v1/jobs/non-existent-id-12345" -Body @{
    status = "PROCESSING"
}

if (-not $nonExistentJob.Success -and $nonExistentJob.StatusCode -eq 404) {
    Add-TestResult -TestName "Non-existent Job Returns 404" -Passed $true -Details "Correctly returned 404 for missing job"
} else {
    Add-TestResult -TestName "Non-existent Job Returns 404" -Passed $false -Details "Should have returned 404, got: $($nonExistentJob.StatusCode)"
}

# Test 10: Multiple fields update
Write-Host "`nTest 10: Testing multiple fields update..." -ForegroundColor Yellow
$createJob4 = Invoke-APIRequest -Method POST -Uri "$baseUrl/api/v1/analyze" -Body @{
    repo_url = "https://github.com/test/repo4"
    branch = "main"
}

if ($createJob4.Success) {
    $jobId4 = $createJob4.Content.job_id
    $multiUpdate = Invoke-APIRequest -Method PATCH -Uri "$baseUrl/api/v1/jobs/$jobId4" -Body @{
        status = "PROCESSING"
        progress = 25
        result_summary = @{
            current_file = "main.go"
            files_processed = 10
        }
    }
    
    if ($multiUpdate.Success -and $multiUpdate.StatusCode -eq 200) {
        Add-TestResult -TestName "Multiple Fields Update" -Passed $true -Details "Updated status, progress, and result_summary together"
    } else {
        Add-TestResult -TestName "Multiple Fields Update" -Passed $false -Details "Status code: $($multiUpdate.StatusCode)"
    }
}

# Print Summary
Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "Test Summary" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

$totalTests = $testResults.Count
$passedTests = ($testResults | Where-Object { $_.Passed }).Count
$failedTests = $totalTests - $passedTests

Write-Host "`nTotal Tests: $totalTests" -ForegroundColor White
Write-Host "Passed: $passedTests" -ForegroundColor Green
Write-Host "Failed: $failedTests" -ForegroundColor $(if ($failedTests -eq 0) { "Green" } else { "Red" })

Write-Host "`nDetailed Results:" -ForegroundColor White
foreach ($result in $testResults) {
    $status = if ($result.Passed) { "✅" } else { "❌" }
    Write-Host "$status $($result.Test)"
}

if ($failedTests -eq 0) {
    Write-Host "`n🎉 All tests passed!" -ForegroundColor Green
} else {
    Write-Host "`n⚠️  Some tests failed. Please review the details above." -ForegroundColor Yellow
}
