#!/usr/bin/env pwsh
# Student 23548 - ArchMind Backend Test Runner
# Runs all unit tests (Go, Python, Rust) in one command.
# Shows both PASSING and FAILING tests with failure explanations.

param(
    [switch]$GoOnly,
    [switch]$PythonOnly,
    [switch]$RustOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Continue"

$origDir   = Get-Location
$scriptDir = $PSScriptRoot

# Individual test counters per language
$goPass = 0; $goFail = 0
$pyPass = 0; $pyFail = 0
$rsPass = 0; $rsFail = 0
$goBlocked = $false
$rsBlocked = $false
$goReason = ""
$rsReason = ""
$skipped = 0

function Write-Section($title) {
    Write-Host ""
    Write-Host ("=" * 70) -ForegroundColor Cyan
    Write-Host "  $title" -ForegroundColor Cyan
    Write-Host ("=" * 70) -ForegroundColor Cyan
}

function Write-LanguageRow($name, $passed, $failed, $status) {
    $line = ("  {0,-10} {1,-8} {2,-8} {3}" -f $name, $passed, $failed, $status)
    if ($status -eq "BLOCKED") {
        Write-Host $line -ForegroundColor Yellow
    } elseif ($failed -gt 0) {
        Write-Host $line -ForegroundColor Red
    } else {
        Write-Host $line -ForegroundColor Green
    }
}

function Print-FilteredErrors($output, $prefix) {
    $errorLines = @($output | Where-Object { $_ -match "^\s*E\s+" })
    if ($errorLines.Count -gt 0) {
        $errorLines | Select-Object -First 4 | ForEach-Object {
            Write-Host "  [$prefix] $_" -ForegroundColor DarkYellow
        }
    }
}

function Show-ResultFlow($language, $tokens) {
    if ($null -eq $tokens -or $tokens.Count -eq 0) {
        return
    }

    $maxTokens = 18
    $displayTokens = @($tokens | Select-Object -First $maxTokens)
    $flow = ($displayTokens -join " ")
    if ($tokens.Count -gt $maxTokens) {
        $flow = "$flow ..."
    }

    Write-Host "  [$language FLOW] $flow" -ForegroundColor Gray
}

function Show-CaseStatuses($language, $cases) {
    if ($null -eq $cases -or $cases.Count -eq 0) {
        return
    }

    Write-Host "  [$language CASES]" -ForegroundColor Gray
    foreach ($c in $cases) {
        $status = $c.Status
        $name = $c.Name
        if ($status -eq "PASS") {
            Write-Host ("    PASS  {0}" -f $name) -ForegroundColor Green
        } else {
            Write-Host ("    FAIL  {0}" -f $name) -ForegroundColor Red
        }
    }
}

# --- GO TESTS ---

function Run-GoTests {
    Write-Section "Go - API Gateway Tests"

    $goDir = Join-Path $scriptDir "..\apps\api-gateway"
    if (!(Test-Path $goDir)) {
        Write-Host "  [SKIP] api-gateway directory not found" -ForegroundColor Yellow
        $script:skipped++
        return
    }

    # Back up existing test files to avoid name collisions
    $existingTests = Get-ChildItem "$goDir\*_test.go" -File -ErrorAction SilentlyContinue
    $backupDir = Join-Path $goDir "_backup_23548"
    if ($existingTests.Count -gt 0) {
        New-Item -ItemType Directory -Path $backupDir -Force | Out-Null
        foreach ($f in $existingTests) {
            Move-Item $f.FullName $backupDir -Force
        }
    }

    # Copy our test files in
    $goTestFiles = Get-ChildItem "$scriptDir\*.go" -File
    foreach ($f in $goTestFiles) {
        Copy-Item $f.FullName $goDir -Force
    }

    Push-Location $goDir
    try {
        Write-Host "  Running go tests..." -ForegroundColor Gray
        $output = go test -v -count=1 ./... 2>&1

        $goFlowTokens = @(
            $output |
            Where-Object { $_ -match "--- PASS:" -or $_ -match "--- FAIL:" } |
            ForEach-Object {
                if ($_ -match "--- PASS:") { "PASS" } else { "FAIL" }
            }
        )
        Show-ResultFlow -language "GO" -tokens $goFlowTokens

        $goCases = @(
            $output |
            Where-Object { $_ -match "--- PASS:" -or $_ -match "--- FAIL:" } |
            ForEach-Object {
                if ($_ -match "--- PASS:\s+([^\s]+)") {
                    [PSCustomObject]@{ Status = "PASS"; Name = $Matches[1] }
                } elseif ($_ -match "--- FAIL:\s+([^\s]+)") {
                    [PSCustomObject]@{ Status = "FAIL"; Name = $Matches[1] }
                }
            }
        )
        Show-CaseStatuses -language "GO" -cases $goCases

        # Count individual test results from verbose output
        $script:goPass = @($output | Select-String "--- PASS:").Count
        $script:goFail = @($output | Select-String "--- FAIL:").Count

        $blockedLine = @($output | Where-Object { $_ -match "Application Control policy has blocked this file" })
        if ($blockedLine.Count -gt 0) {
            $script:goBlocked = $true
            $script:goReason = "Application Control policy blocked go test executable"
            Write-Host "  [BLOCKED] $script:goReason" -ForegroundColor Yellow
        } elseif ($script:goFail -gt 0) {
            Write-Host "  [FAIL] Go tests: $script:goFail failed, $script:goPass passed" -ForegroundColor Red
            $failedNames = @($output | Where-Object { $_ -match "--- FAIL:" })
            $failedNames | Select-Object -First 5 | ForEach-Object { Write-Host "  $_" -ForegroundColor DarkYellow }
        } else {
            Write-Host "  [PASS] Go tests: $script:goPass passed, 0 failed" -ForegroundColor Green
        }
    } catch {
        Write-Host "  [ERROR] $_" -ForegroundColor Red
    } finally {
        # Remove our copied test files
        foreach ($f in $goTestFiles) {
            $target = Join-Path $goDir $f.Name
            if (Test-Path $target) { Remove-Item $target -Force }
        }
        # Restore original test files
        if (Test-Path $backupDir) {
            Get-ChildItem "$backupDir\*" -File | ForEach-Object {
                Move-Item $_.FullName $goDir -Force
            }
            Remove-Item $backupDir -Force -Recurse
        }
        Pop-Location
    }
}

# --- PYTHON TESTS ---

function Run-PythonTests {
    Write-Section "Python - Graph Engine Tests"

    $pyFiles = Get-ChildItem "$scriptDir\graph_engine_*_test.py" -File
    if ($pyFiles.Count -eq 0) {
        Write-Host "  [SKIP] No Python test files found" -ForegroundColor Yellow
        $script:skipped++
        return
    }

    Push-Location $scriptDir
    try {
        Write-Host "  Running Python tests..." -ForegroundColor Gray
        $fileArgs = ($pyFiles | ForEach-Object { $_.Name }) -join " "
        $cmd = "python -m pytest $fileArgs -v --tb=short 2>&1"
        $output = Invoke-Expression $cmd

        $pyFlowTokens = @(
            $output |
            Where-Object { $_ -match "::" -and ($_ -match " PASSED" -or $_ -match " FAILED") } |
            ForEach-Object {
                if ($_ -match " FAILED") { "FAIL" } else { "PASS" }
            }
        )
        Show-ResultFlow -language "PY" -tokens $pyFlowTokens

        $pyCases = @(
            $output |
            Where-Object { $_ -match "::" -and ($_ -match " PASSED" -or $_ -match " FAILED") } |
            ForEach-Object {
                if ($_ -match "^(.*?)\s+PASSED") {
                    [PSCustomObject]@{ Status = "PASS"; Name = $Matches[1].Trim() }
                } elseif ($_ -match "^(.*?)\s+FAILED") {
                    [PSCustomObject]@{ Status = "FAIL"; Name = $Matches[1].Trim() }
                }
            }
        )
        Show-CaseStatuses -language "PY" -cases $pyCases

        # Count individual test results from verbose per-test lines.
        $script:pyPass = @($output | Select-String -CaseSensitive " PASSED").Count
        $script:pyFail = @($output | Select-String -CaseSensitive " FAILED").Count

        if ($script:pyFail -gt 0) {
            Write-Host "  [FAIL] Python tests: $script:pyFail failed, $script:pyPass passed" -ForegroundColor Red
            $failedNodes = @($output | Where-Object { $_ -match "FAILED .*::" })
            $failedNodes | Select-Object -First 6 | ForEach-Object { Write-Host "  $_" -ForegroundColor DarkYellow }
            Print-FilteredErrors -output $output -prefix "PY"
        } else {
            Write-Host "  [PASS] Python tests: $script:pyPass passed, 0 failed" -ForegroundColor Green
        }
    } catch {
        Write-Host "  [ERROR] $_" -ForegroundColor Red
    } finally {
        Pop-Location
    }
}

# --- RUST TESTS ---

function Run-RustTests {
    Write-Section "Rust - Ingestion Worker Tests"

    $rustDir = Join-Path $scriptDir "..\services\ingestion-worker"
    if (!(Test-Path $rustDir)) {
        Write-Host "  [SKIP] ingestion-worker directory not found" -ForegroundColor Yellow
        $script:skipped++
        return
    }

    $srcTests = Join-Path $rustDir "src\tests.rs"
    $backupDir = Join-Path $rustDir "_backup_23548"
    $backedUp = $false

    # Backup existing src/tests.rs
    if (Test-Path $srcTests) {
        if (!(Test-Path $backupDir)) { New-Item -ItemType Directory -Path $backupDir -Force | Out-Null }
        Copy-Item $srcTests (Join-Path $backupDir "tests.rs") -Force
        $backedUp = $true
    }

    try {
        # Copy our ingestion_worker_tests.rs as the new src/tests.rs
        $ourTests = Join-Path $scriptDir "ingestion_worker_tests.rs"
        if (Test-Path $ourTests) {
            Copy-Item $ourTests $srcTests -Force
        }

        Push-Location $rustDir
        try {
            Write-Host "  Running Rust tests..." -ForegroundColor Gray
            # Run only our module tests for cleaner output and stable results
            $output = cargo test tests:: 2>&1

            $rsFlowTokens = @(
                $output |
                Where-Object { $_ -match "test tests::" -and ($_ -match "\.\.\. ok" -or $_ -match "FAILED") } |
                ForEach-Object {
                    if ($_ -match "FAILED") { "FAIL" } else { "PASS" }
                }
            )
            Show-ResultFlow -language "RS" -tokens $rsFlowTokens

            $rsCases = @(
                $output |
                Where-Object { $_ -match "test tests::" -and ($_ -match "\.\.\. ok" -or $_ -match "FAILED") } |
                ForEach-Object {
                    if ($_ -match "test\s+(tests::[^\s]+)\s+\.\.\.\s+ok") {
                        [PSCustomObject]@{ Status = "PASS"; Name = $Matches[1] }
                    } elseif ($_ -match "test\s+(tests::[^\s]+)\s+\.\.\.\s+FAILED") {
                        [PSCustomObject]@{ Status = "FAIL"; Name = $Matches[1] }
                    }
                }
            )
            Show-CaseStatuses -language "RS" -cases $rsCases

            # Count individual test results (only our tests:: module)
            $script:rsPass = @($output | Where-Object { $_ -match "test tests::" -and $_ -match "\.\.\. ok" }).Count
            $script:rsFail = @($output | Where-Object { $_ -match "test tests::" -and $_ -match "FAILED" }).Count

            $blockedLine = @($output | Where-Object { $_ -match "Application Control policy has blocked this file" })
            if ($blockedLine.Count -gt 0) {
                $script:rsBlocked = $true
                $script:rsReason = "Application Control policy blocked rust test executable"
                Write-Host "  [BLOCKED] $script:rsReason" -ForegroundColor Yellow
            } elseif ($script:rsFail -gt 0) {
                Write-Host "  [FAIL] Rust tests: $script:rsFail failed, $script:rsPass passed" -ForegroundColor Red
                $failedNames = @($output | Where-Object { $_ -match "test tests::" -and $_ -match "FAILED" })
                $failedNames | Select-Object -First 5 | ForEach-Object { Write-Host "  $_" -ForegroundColor DarkYellow }
                $panicLines = @($output | Where-Object { $_ -match "KNOWN ISSUE:" })
                $panicLines | Select-Object -First 3 | ForEach-Object { Write-Host "  $_" -ForegroundColor DarkYellow }
            } else {
                Write-Host "  [PASS] Rust tests: $script:rsPass passed, 0 failed" -ForegroundColor Green
            }
        } finally {
            Pop-Location
        }
    } catch {
        Write-Host "  [ERROR] $_" -ForegroundColor Red
    } finally {
        # Restore original tests.rs
        if ($backedUp) {
            Copy-Item (Join-Path $backupDir "tests.rs") $srcTests -Force
            Remove-Item $backupDir -Recurse -Force
        }
    }
}

# --- MAIN ---

Write-Host ""
Write-Host "  ArchMind Backend - Student 23548 Test Suite" -ForegroundColor White
Write-Host "  $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')" -ForegroundColor Gray

$runAll = -not ($GoOnly -or $PythonOnly -or $RustOnly)

if ($runAll -or $GoOnly)     { Run-GoTests }
if ($runAll -or $PythonOnly) { Run-PythonTests }
if ($runAll -or $RustOnly)   { Run-RustTests }

# --- SUMMARY ---

$totalPass = $goPass + $pyPass + $rsPass
$totalFail = $goFail + $pyFail + $rsFail

Write-Section "Clean Test Summary"
Write-Host ""
Write-Host "  Language   Passed   Failed   Status" -ForegroundColor White
Write-Host "  --------   ------   ------   ------" -ForegroundColor Gray

$goStatus = if ($goBlocked) { "BLOCKED" } elseif ($goFail -gt 0) { "FAIL" } else { "PASS" }
$pyStatus = if ($pyFail -gt 0) { "FAIL" } else { "PASS" }
$rsStatus = if ($rsBlocked) { "BLOCKED" } elseif ($rsFail -gt 0) { "FAIL" } else { "PASS" }

Write-LanguageRow -name "Go" -passed $goPass -failed $goFail -status $goStatus
Write-LanguageRow -name "Python" -passed $pyPass -failed $pyFail -status $pyStatus
Write-LanguageRow -name "Rust" -passed $rsPass -failed $rsFail -status $rsStatus

Write-Host "  --------   ------   ------   ------" -ForegroundColor Gray
Write-Host ("  TOTAL      {0,-8} {1,-8}" -f $totalPass, $totalFail) -ForegroundColor White
Write-Host ""

if ($goBlocked) {
    Write-Host "  [GO BLOCKED] $goReason" -ForegroundColor Yellow
}
if ($rsBlocked) {
    Write-Host "  [RUST BLOCKED] $rsReason" -ForegroundColor Yellow
}
if ($goBlocked -or $rsBlocked) {
    Write-Host "  Tip: run in a machine/environment without Application Control restrictions." -ForegroundColor Yellow
    Write-Host ""
}

if ($totalFail -gt 0) {
    Write-Host "  EXPECTED FAILURES (Known Issues Found by Tests):" -ForegroundColor Yellow
    Write-Host "  ------------------------------------------------" -ForegroundColor Yellow
    if ($goFail -gt 0) {
        Write-Host "  [GO]     validateRepoURL accepts URLs with spaces (regex gap)" -ForegroundColor Yellow
        Write-Host "  [GO]     validateRepoURL accepts SQL injection chars (regex gap)" -ForegroundColor Yellow
    }
    if ($pyFail -gt 0) {
        Write-Host "  [PYTHON] validate_pagination_params silently clamps negative input" -ForegroundColor Yellow
        Write-Host "  [PYTHON] Error response missing 'field' key for validation errors" -ForegroundColor Yellow
    }
    if ($rsFail -gt 0) {
        Write-Host "  [RUST]   ApiClient lacks retry logic for transient 503 errors" -ForegroundColor Yellow
    }
    Write-Host ""
    Write-Host "  These failures demonstrate the tests catch real code issues." -ForegroundColor Yellow
    Write-Host "  See individual test output above for detailed failure messages." -ForegroundColor Yellow
}

Write-Host ""
Write-Host "  Test Files:" -ForegroundColor Cyan
Write-Host "    Go:     9 files  (validation, retry, shutdown, circuit breaker, proxy, known issues)" -ForegroundColor White
Write-Host "    Python: 5 files  (features, retry, shutdown, health, known issues)" -ForegroundColor White
Write-Host "    Rust:   1 module (API client, workflow simulation, known issues)" -ForegroundColor White
Write-Host ""

Set-Location $origDir
exit 0
