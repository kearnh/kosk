param([string]$Executable = (Join-Path $PSScriptRoot '../target/release/kosk.exe'))

$ErrorActionPreference = 'Stop'
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
$manifest = Get-Content (Join-Path $PSScriptRoot '../Cargo.toml') -Raw
$version = [regex]::Match($manifest, '(?m)^version = "([^"]+)"').Groups[1].Value
$details = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($executablePath)
$expected = @{
    FileDescription = 'Controller-operated on-screen keyboard'
    ProductName = 'KOSK'
    FileVersion = $version
    ProductVersion = $version
    InternalName = 'kosk'
    OriginalFilename = 'kosk.exe'
    LegalCopyright = 'Copyright (c) 2026 KOSK contributors'
}

foreach ($field in $expected.Keys) {
    if ($details.$field -cne $expected[$field]) {
        throw "$field expected '$($expected[$field])', got '$($details.$field)'"
    }
}

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ExecutableResources {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr LoadLibraryEx(string path, IntPtr file, uint flags);
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern IntPtr FindResource(IntPtr module, IntPtr name, IntPtr type);
    [DllImport("kernel32.dll")]
    public static extern bool FreeLibrary(IntPtr module);
}
'@

# LOAD_LIBRARY_AS_DATAFILE; RT_GROUP_ICON.
$loadAsDataFile = 0x00000002
$groupIconResource = 14
$module = [ExecutableResources]::LoadLibraryEx($executablePath, [IntPtr]::Zero, $loadAsDataFile)
if ($module -eq [IntPtr]::Zero) { throw 'Could not load executable resources' }

try {
    $icon = [ExecutableResources]::FindResource($module, [IntPtr]1, [IntPtr]$groupIconResource)
    if ($icon -eq [IntPtr]::Zero) { throw 'Executable has no application icon' }
}
finally {
    [void][ExecutableResources]::FreeLibrary($module)
}

Write-Output 'Executable icon and file details verified'
