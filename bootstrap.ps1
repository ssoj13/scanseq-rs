# ScanSeq build/test bootstrap script
param(
    [Parameter(Position=0)]
    [ValidateSet("build", "ext", "doc", "test", "flame", "profile", "check", "help")]
    [string]$Command = "help"
)

switch ($Command) {
    "build" {
        Write-Host "Building with cargo..." -ForegroundColor Cyan
        cargo build --release
        if ($LASTEXITCODE -eq 0) {
            Write-Host "`nBuild complete!" -ForegroundColor Green
        } else {
            Write-Host "Build failed!" -ForegroundColor Red
            exit 1
        }
    }

    "ext" {
        Write-Host "Building Python extension with maturin..." -ForegroundColor Cyan
        maturin develop --release --features python
        if ($LASTEXITCODE -eq 0) {
            Write-Host "`nBuild complete! Test with:" -ForegroundColor Green
            Write-Host "python -c `"import scanseq; s = scanseq.Scanner(['.']); print(s)`"" -ForegroundColor Yellow
        } else {
            Write-Host "Build failed!" -ForegroundColor Red
            exit 1
        }
    }

    "doc" {
        Write-Host "Building documentation..." -ForegroundColor Cyan
        cargo doc --open
        if ($LASTEXITCODE -eq 0) {
            Write-Host "Documentation built successfully" -ForegroundColor Green
        } else {
            Write-Host "Documentation build failed" -ForegroundColor Red
            exit 1
        }
    }

    "test" {
        Write-Host "Running tests..." -ForegroundColor Cyan
        cargo test
        if ($LASTEXITCODE -eq 0) {
            Write-Host "Tests passed successfully" -ForegroundColor Green
        } else {
            Write-Host "Tests failed" -ForegroundColor Red
            exit 1
        }
    }

    "flame" {
        Write-Host "Generating flamegraph..." -ForegroundColor Cyan
        Write-Host "Tip: debug=true in [profile.release] gives better symbols`n" -ForegroundColor Yellow
        $testPath = "src/**"
        Write-Host "Profiling: scanseq-cli $testPath`n" -ForegroundColor Cyan
        cargo flamegraph --bin scanseq-cli -- $testPath
        if ($LASTEXITCODE -eq 0) {
            Write-Host "`nFlamegraph generated: flamegraph.svg" -ForegroundColor Green
            Write-Host "Open: start flamegraph.svg" -ForegroundColor Cyan
        } else {
            Write-Host "`nFlamegraph generation failed!" -ForegroundColor Red
            exit 1
        }
    }

    "profile" {
        Write-Host "Building release binary..." -ForegroundColor Cyan
        cargo build --release
        if ($LASTEXITCODE -ne 0) {
            Write-Host "Build failed!" -ForegroundColor Red
            exit 1
        }
        Write-Host "`nProfiling current directory..." -ForegroundColor Yellow
        Measure-Command { .\target\release\scanseq-cli.exe "." | Out-Null }
        $testDir = "C:\Programs\Ntutil"
        if (Test-Path $testDir) {
            Write-Host "`nProfiling large directory ($testDir)..." -ForegroundColor Yellow
            Measure-Command { .\target\release\scanseq-cli.exe $testDir | Out-Null }
        }
        Write-Host "`nDone!" -ForegroundColor Green
    }

    "check" {
        $exe = ".\target\release\scanseq-cli.exe"
        $testDir = "C:\temp\test_scanseq"
        Write-Host "`n=== Testing ScanSeq ===" -ForegroundColor Cyan

        Write-Host "`n[Test 1] Basic output" -ForegroundColor Yellow
        & $exe $testDir

        Write-Host "`n[Test 2] JSON output" -ForegroundColor Yellow
        & $exe $testDir --json

        Write-Host "`n[Test 3] With mask *.exr" -ForegroundColor Yellow
        & $exe $testDir --mask "*.exr"

        Write-Host "`n[Test 4] min-len 10" -ForegroundColor Yellow
        & $exe $testDir --min-len 10

        Write-Host "`n[Test 5] Large dataset (first 20 lines)" -ForegroundColor Yellow
        $env:RUST_LOG = "info"
        & $exe "C:\programs\ntutil" --min-len 10 2>&1 | Select-Object -First 20

        Write-Host "`n=== Tests complete ===" -ForegroundColor Cyan
    }

    "help" {
        Write-Host "`nUsage: .\bootstrap.ps1 <command>`n" -ForegroundColor Cyan
        Write-Host "Commands:" -ForegroundColor Yellow
        Write-Host "  build   - Build release binary (cargo build --release)"
        Write-Host "  ext     - Build Python extension (maturin develop)"
        Write-Host "  doc     - Build and open documentation"
        Write-Host "  test    - Run unit tests (cargo test)"
        Write-Host "  flame   - Generate flamegraph profile"
        Write-Host "  profile - Run performance benchmarks"
        Write-Host "  check   - Run integration tests"
        Write-Host ""
    }
}
