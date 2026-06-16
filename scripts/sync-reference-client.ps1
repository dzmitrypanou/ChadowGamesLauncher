

param(
    [string]$SourceRoot = "$env:APPDATA\ChadowGamesLauncher",
    [string]$Version = "1.21.11",
    [switch]$SkipModBuild
)

$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path "$PSScriptRoot\.."
$DestRoot = Join-Path $RepoRoot "reference-client"
$ModDir = Join-Path $RepoRoot "client-mod"

function Write-Step($msg) { Write-Host "==> $msg" }

if (-not (Test-Path $SourceRoot)) {
    throw "Source not found: $SourceRoot"
}

$jarPath = Join-Path $SourceRoot "versions\$Version\$Version.jar"
if (-not (Test-Path $jarPath)) {
    throw "Missing client jar: $jarPath"
}

if (-not $SkipModBuild) {
    Write-Step "Building mod..."
    Push-Location $ModDir
    try {
        & .\gradlew.bat build --no-daemon -q
        if ($LASTEXITCODE -ne 0) { throw "Gradle build failed" }
    } finally {
        Pop-Location
    }
}

Write-Step "Syncing client payload to $DestRoot ..."
New-Item -ItemType Directory -Force -Path $DestRoot | Out-Null

foreach ($name in @("versions", "libraries", "assets", "natives")) {
    $src = Join-Path $SourceRoot $name
    if (-not (Test-Path $src)) {
        throw "Missing source folder: $src"
    }
    $dst = Join-Path $DestRoot $name
    if (Test-Path $dst) { Remove-Item $dst -Recurse -Force }
    Write-Host "    $name ..."
    & robocopy $src $dst /E /NFL /NDL /NJH /NJS /nc /ns /np | Out-Null
    if ($LASTEXITCODE -ge 8) { throw "robocopy failed for $name" }
}

$modsDir = Join-Path $DestRoot "mods"
New-Item -ItemType Directory -Force -Path $modsDir | Out-Null
$modJar = Get-ChildItem (Join-Path $ModDir "build\libs") -Filter "chadow-games-client-*.jar" |
    Where-Object { $_.Name -notmatch "sources" } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if ($modJar) {
    Copy-Item $modJar.FullName (Join-Path $modsDir $modJar.Name) -Force
    Write-Host "    mods/$($modJar.Name)"
}

$size = (Get-ChildItem $DestRoot -Recurse -File | Measure-Object Length -Sum).Sum
$manifest = @{
    version = $Version
    syncedAt = (Get-Date).ToUniversalTime().ToString("o")
    source = $SourceRoot
    sizeBytes = $size
} | ConvertTo-Json -Depth 3
Set-Content -Path (Join-Path $DestRoot "manifest.json") -Value $manifest -Encoding UTF8

Write-Host ""
Write-Host "Done!"
Write-Host "  Target: $DestRoot"
Write-Host "  Size:   $([math]::Round($size / 1MB, 1)) MB"
