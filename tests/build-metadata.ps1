$ErrorActionPreference = 'Stop'

$fixture = Join-Path ([System.IO.Path]::GetTempPath()) ('kb-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
$repository = Join-Path $fixture 'repo'

New-Item -ItemType Directory -Path $fixture | Out-Null

try {
    $manifestPath = (Resolve-Path (Join-Path $PSScriptRoot '../Cargo.toml')).Path
    $buildOutput = cargo build --manifest-path $manifestPath --lib --message-format=json
    if ($LASTEXITCODE -ne 0) { throw 'Build script compilation failed' }
    $buildScript = $buildOutput | ForEach-Object { $_ | ConvertFrom-Json } |
        Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'build-script-build' -and
            $_.manifest_path -eq $manifestPath } |
        Select-Object -Last 1 -ExpandProperty filenames |
        Where-Object { $_ -like '*.exe' }
    if (-not $buildScript) { throw 'KOSK build script executable not found' }

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
