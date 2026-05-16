# Runs `cargo fmt` after the agent edits a Rust source file.
# Receives afterFileEdit JSON on stdin (see Cursor hooks docs).

$ErrorActionPreference = 'SilentlyContinue'

$stdin = [Console]::In.ReadToEnd()
if ([string]::IsNullOrWhiteSpace($stdin)) {
    exit 0
}

try {
    $payload = $stdin | ConvertFrom-Json
} catch {
    exit 0
}

$filePath = [string]$payload.file_path
if (-not $filePath -or -not $filePath.EndsWith('.rs', [System.StringComparison]::OrdinalIgnoreCase)) {
    exit 0
}

$root = (Get-Location).Path
if ($payload.workspace_roots -and @($payload.workspace_roots).Count -gt 0) {
    $root = [string]$payload.workspace_roots[0]
}

Push-Location $root
try {
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargo) {
        exit 0
    }
    & cargo fmt 2>&1 | Out-Null
} finally {
    Pop-Location
}

exit 0
