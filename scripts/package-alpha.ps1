param(
    [string]$BigramSource,
    [string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repository = Split-Path $PSScriptRoot
$platform = 'x86_64-pc-windows-msvc'
$themeFiles = @('old-steam-controller.toml', 'portal.toml', 'factorio.toml',
    'cyberpunk-2077.toml', 'hollow-knight.toml', 'stardew-valley.toml')

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed ($LASTEXITCODE)" }
}

function Copy-PackageFile {
    param([string]$Source, [string]$RelativePath)
    $destination = Join-Path $staging $RelativePath
    New-Item -ItemType Directory -Path (Split-Path $destination) -Force | Out-Null
    Copy-Item -LiteralPath $Source -Destination $destination
}

Push-Location $repository
$previousCommit = $env:KOSK_BUILD_COMMIT
$previousDirty = $env:KOSK_BUILD_DIRTY
$previousRustFlags = $env:RUSTFLAGS
try {
    $changes = Invoke-Checked jj @('diff', '--summary')
    if ($changes) { throw 'Commit working-copy changes before packaging' }
    $sourceCommit = (Invoke-Checked jj @('log', '-r', '@-', '--no-graph', '-T', 'commit_id')).Trim()
    $env:KOSK_BUILD_COMMIT = $sourceCommit
    $env:KOSK_BUILD_DIRTY = '0'
    $env:RUSTFLAGS = '-C target-feature=+crt-static'

    $metadata = (Invoke-Checked cargo @('metadata', '--locked', '--offline', '--format-version', '1',
        '--filter-platform', $platform)) | ConvertFrom-Json
    $package = $metadata.packages | Where-Object name -eq 'kosk'
    $version = $package.version
    $packageName = "kosk-$version-windows-x64"
    if (-not $BigramSource) { $BigramSource = Join-Path $repository 'data/completion/en/count_2w.txt' }
    $BigramSource = (Resolve-Path -LiteralPath $BigramSource).Path
    if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repository 'target/dist' }
    $OutputDirectory = [System.IO.Path]::GetFullPath($OutputDirectory)
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    $staging = Join-Path $repository "target/package/$packageName-$([guid]::NewGuid().ToString('N'))/$packageName"
    New-Item -ItemType Directory -Path $staging -Force | Out-Null

    Invoke-Checked cargo @('build', '--release', '--locked', '--offline', '--target', $platform,
        '--bin', 'kosk', '--bin', 'completion_build', '--bin', 'completion_dev')
    $binaries = Join-Path $metadata.target_directory "$platform/release"
    Copy-PackageFile (Join-Path $binaries 'kosk.exe') 'kosk.exe'

    $model = Join-Path $staging 'resources/completion/en'
    $unigrams = Join-Path $repository 'data/completion/en/unigrams.tsv'
    Invoke-Checked (Join-Path $binaries 'completion_build.exe') @('--unigrams', $unigrams,
        '--bigrams', $BigramSource, '--out', $model)
    $pairBytes = (Get-Item -LiteralPath (Join-Path $model 'bigrams.bin')).Length
    $countRecordBytes = 12
    if ($pairBytes -eq 0 -or $pairBytes % $countRecordBytes -ne 0) { throw 'Invalid or empty contextual bigram table' }
    $trigrams = Join-Path $model 'trigrams.bin'
    if ((Get-Item -LiteralPath $trigrams).Length -eq 0) { Remove-Item -LiteralPath $trigrams }

    Copy-PackageFile $unigrams 'resources/completion/en/unigrams.tsv'
    Copy-PackageFile (Join-Path $repository 'data/completion/en/README.md') 'resources/completion/en/README.md'
    foreach ($theme in $themeFiles) {
        Copy-PackageFile (Join-Path $repository "themes/$theme") "resources/themes/$theme"
    }

    $predictions = Invoke-Checked (Join-Path $binaries 'completion_dev.exe') @('--backend', 'ngram',
        '--model-dir', $model, '--text', 'going ', '--cursor', '6')
    if (-not ($predictions -match '\bto\b')) { throw 'Contextual completion verification failed' }

    $documents = @('README.md', 'LICENSE', 'MAPPINGS.md', 'old_sc.toml', 'old_sc_symbols.toml',
        'qwerty.toml', 'mappings.toml', 'docs/themes.md', 'docs/show-hide.md',
        'assets/controller-glyphs/README.md', 'data/completion/en/README.md',
        'assets/licenses/CC-BY-SA-4.0.txt')
    $tracked = Invoke-Checked jj @('file', 'list', '-r', '@-')
    $documents += $tracked | ForEach-Object { $_.Replace('\', '/') } | Where-Object { $_ -like 'docs/developer/*.md' }
    foreach ($document in $documents) {
        Copy-PackageFile (Join-Path $repository $document) $document
    }

    $resourceRoot = Join-Path $staging 'resources'
    $assetRows = @(Get-ChildItem -LiteralPath $resourceRoot -File -Recurse | ForEach-Object {
        $relative = [System.IO.Path]::GetRelativePath($resourceRoot, $_.FullName).Replace('\', '/')
        [pscustomobject]@{ path = $relative; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
    } | Sort-Object path)
    $identity = ($assetRows | ForEach-Object { "$($_.path) $($_.sha256)" }) -join "`n"
    $bundleId = [Convert]::ToHexString([System.Security.Cryptography.SHA256]::HashData(
        [System.Text.Encoding]::UTF8.GetBytes($identity))).ToLowerInvariant()
    $assetNames = ($assetRows | ForEach-Object { '"' + $_.path + '"' }) -join ', '
    $bundle = "format_version = 1`nid = `"$bundleId`"`nfiles = [$assetNames]`n"
    Set-Content -LiteralPath (Join-Path $resourceRoot 'bundle.toml') -Value $bundle -Encoding utf8NoBOM

    $packagesById = @{}
    $nodesById = @{}
    foreach ($dependency in $metadata.packages) { $packagesById[$dependency.id] = $dependency }
    foreach ($node in $metadata.resolve.nodes) { $nodesById[$node.id] = $node }
    $pending = [System.Collections.Generic.Stack[string]]::new()
    $pending.Push($package.id)
    $included = [System.Collections.Generic.HashSet[string]]::new()
    while ($pending.Count) {
        $id = $pending.Pop()
        if (-not $included.Add($id)) { continue }
        foreach ($dependency in $nodesById[$id].deps) { $pending.Push($dependency.pkg) }
    }

    $notices = [System.Text.StringBuilder]::new()
    $upstreamLicenses = Get-Content -Raw -LiteralPath (Join-Path $repository 'assets/licenses/upstream-sources.json') | ConvertFrom-Json
    [void]$notices.AppendLine('KOSK third-party notices')
    [void]$notices.AppendLine((Get-Content -Raw -LiteralPath (Join-Path $repository 'data/completion/en/README.md')))
    [void]$notices.AppendLine((Get-Content -Raw -LiteralPath (Join-Path $repository 'assets/controller-glyphs/README.md')))
    $dependencies = @($included | ForEach-Object { $packagesById[$_] } | Sort-Object name, version)
    foreach ($dependency in $dependencies) {
        [void]$notices.AppendLine("`n===== $($dependency.name) $($dependency.version) =====")
        $directory = Split-Path $dependency.manifest_path
        $license = $dependency.license
        if (-not $license -and $directory.StartsWith($repository, [StringComparison]::OrdinalIgnoreCase)) { $license = 'MIT (KOSK workspace)' }
        if (-not $license) { throw "Missing license metadata: $($dependency.name)" }
        [void]$notices.AppendLine("License: $license")
        [void]$notices.AppendLine("Repository: $($dependency.repository)")
        $licenseFiles = if ($directory.StartsWith($repository, [StringComparison]::OrdinalIgnoreCase)) {
            @(Get-Item -LiteralPath (Join-Path $repository 'LICENSE'))
        } else {
            @(Get-ChildItem -LiteralPath $directory -File -Recurse | Where-Object {
                $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|OFL)([._-]|$)' -and $_.Length -lt 200000
            })
        }
        if (-not $licenseFiles) {
            $upstream = $upstreamLicenses | Where-Object { $_.packages -contains "$($dependency.name) $($dependency.version)" }
            if (-not $upstream) { throw "Missing license text: $($dependency.name)" }
            [void]$notices.AppendLine("Upstream revision: $($upstream.revision)")
            $licenseFiles = @($upstream.files | ForEach-Object {
                Get-Item -LiteralPath (Join-Path $repository "assets/licenses/$($_.path)")
            })
        }
        foreach ($licenseFile in $licenseFiles) {
            [void]$notices.AppendLine("--- $($licenseFile.Name) ---")
            [void]$notices.AppendLine((Get-Content -Raw -LiteralPath $licenseFile.FullName))
        }
    }
    Set-Content -LiteralPath (Join-Path $staging 'THIRD-PARTY-NOTICES.txt') -Value $notices.ToString() -Encoding utf8NoBOM

    $manifest = [ordered]@{
        version = $version
        platform = $platform
        source_commit = $sourceCommit
        unsigned = $true
        rustc = (Invoke-Checked rustc @('--version')).Trim()
        build_command = 'cargo build --release --locked --offline --target x86_64-pc-windows-msvc --bin kosk --bin completion_build --bin completion_dev'
        rustflags = $env:RUSTFLAGS
        bundle_id = $bundleId
        model_inputs = @(
            @{ name = 'unigrams.tsv'; sha256 = (Get-FileHash -LiteralPath $unigrams -Algorithm SHA256).Hash.ToLowerInvariant() },
            @{ name = [System.IO.Path]::GetFileName($BigramSource); sha256 = (Get-FileHash -LiteralPath $BigramSource -Algorithm SHA256).Hash.ToLowerInvariant() }
        )
        contextual_bigrams = $pairBytes / $countRecordBytes
        bundled_assets = $assetRows
        pending_publication_checks = @('Steam glyph redistribution terms', 'Norvig bigram redistribution terms', 'Clean Windows machine verification')
    }
    $manifest.files = @(Get-ChildItem -LiteralPath $staging -File -Recurse | ForEach-Object {
        @{ path = [System.IO.Path]::GetRelativePath($staging, $_.FullName).Replace('\', '/');
           sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
    } | Sort-Object { $_.path })
    Set-Content -LiteralPath (Join-Path $staging 'BUILD-MANIFEST.json') -Value ($manifest | ConvertTo-Json -Depth 8) -Encoding utf8NoBOM

    $archive = Join-Path $OutputDirectory "$packageName.zip"
    Compress-Archive -LiteralPath $staging -DestinationPath $archive -CompressionLevel Optimal -Force
    $checksum = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -LiteralPath "$archive.sha256" -Value "$checksum  $packageName.zip" -Encoding ascii
    [pscustomobject]@{ Zip = $archive; Bytes = (Get-Item -LiteralPath $archive).Length; SHA256 = $checksum; SourceCommit = $sourceCommit }
}
finally {
    $env:KOSK_BUILD_COMMIT = $previousCommit
    $env:KOSK_BUILD_DIRTY = $previousDirty
    $env:RUSTFLAGS = $previousRustFlags
    Pop-Location
}
