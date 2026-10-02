$ErrorActionPreference = 'Stop'

$fixture = Join-Path ([System.IO.Path]::GetTempPath()) ('kb-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
$buildScript = Join-Path $fixture 'build-metadata.exe'
$repository = Join-Path $fixture 'repo'

New-Item -ItemType Directory -Path $fixture | Out-Null

try {
    rustc (Join-Path $PSScriptRoot '../build.rs') -o $buildScript
    if ($LASTEXITCODE -ne 0) { throw 'Build script compilation failed' }

    jj git init $repository
    if ($LASTEXITCODE -ne 0) { throw 'Fixture initialization failed' }

    Push-Location $repository
    try {
        Set-Content tracked.txt 'tracked content'
        jj commit -m 'Build metadata fixture'
        if ($LASTEXITCODE -ne 0) { throw 'Fixture commit failed' }

        $trackedFile = Get-Item tracked.txt
        $trackedFile.LastWriteTimeUtc = $trackedFile.LastWriteTimeUtc.AddSeconds(-10)
        $index = Join-Path $repository '.git/index'
        $before = (Get-Item -Force $index).LastWriteTimeUtc

        $metadata = & $buildScript
        if ($LASTEXITCODE -ne 0) { throw 'Build metadata query failed' }
        if ($metadata -notcontains 'cargo:rustc-env=GIT_DIRTY=0' -or
            -not ($metadata -match '^cargo:rustc-env=GIT_COMMIT=[0-9a-f]{40}$')) {
            throw 'Build metadata query failed to read the fixture repository'
        }

        $after = (Get-Item -Force $index).LastWriteTimeUtc
        if ($after -ne $before) {
            throw 'Build metadata query rewrote the watched index'
        }

        Write-Output 'Build metadata leaves the watched index unchanged'
    }
    finally {
        Pop-Location
    }
}
finally {
    $resolvedFixture = (Resolve-Path -LiteralPath $fixture).Path
    if ($resolvedFixture -ne $fixture) { throw 'Unexpected fixture cleanup path' }
    Remove-Item -LiteralPath $fixture -Recurse -Force
}
