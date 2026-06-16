# Build full Minecraft client ZIP for chadow.ru/admin/minecraft upload.
# Structure: versions/{version}/, libraries/, assets/, mods/
param(
    [string]$Version = "1.21.11",
    [string]$SourceRoot = "$env:APPDATA\ChadowGamesLauncher",
    [string]$OutDir = "$PSScriptRoot\..\dist",
    [switch]$SkipModBuild
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path "$PSScriptRoot\.."
$ModDir = Join-Path $Root "client-mod"
$ZipName = "minecraft-$Version-client.zip"
$ZipPath = Join-Path $OutDir $ZipName
$Staging = Join-Path $env:TEMP "chadow-full-client-staging"

function Write-Step($msg) { Write-Host "==> $msg" }

if (-not $SkipModBuild) {
    Write-Step "Building Fabric mod..."
    Push-Location $ModDir
    try {
        & .\gradlew.bat build --no-daemon -q
        if ($LASTEXITCODE -ne 0) { throw "Gradle build failed" }
    } finally {
        Pop-Location
    }
}

$ModJar = Get-ChildItem (Join-Path $ModDir "build\libs") -Filter "chadow-games-client-*.jar" |
    Where-Object { $_.Name -notmatch "sources" } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $ModJar) { throw "Mod JAR not found. Run without -SkipModBuild first." }

$VersionDir = Join-Path $SourceRoot "versions\$Version"
$JarPath = Join-Path $VersionDir "$Version.jar"
$JsonPath = Join-Path $VersionDir "$Version.json"
$LibrariesDir = Join-Path $SourceRoot "libraries"
$AssetsDir = Join-Path $SourceRoot "assets"

foreach ($path in @($JarPath, $JsonPath, $LibrariesDir, $AssetsDir)) {
    if (-not (Test-Path $path)) {
        throw "Missing: $path`nInstall Minecraft $Version via the launcher first, then re-run this script."
    }
}

Write-Step "Staging client files from $SourceRoot ..."
if (Test-Path $Staging) { Remove-Item $Staging -Recurse -Force }
New-Item -ItemType Directory -Force -Path $Staging | Out-Null

$targets = @(
    @{ Src = Join-Path $SourceRoot "versions\$Version"; Dst = Join-Path $Staging "versions\$Version" },
    @{ Src = $LibrariesDir; Dst = Join-Path $Staging "libraries" },
    @{ Src = $AssetsDir; Dst = Join-Path $Staging "assets" }
)

foreach ($t in $targets) {
    Write-Host "    copy $($t.Src) ..."
    & robocopy $t.Src $t.Dst /E /NFL /NDL /NJH /NJS /nc /ns /np | Out-Null
    if ($LASTEXITCODE -ge 8) { throw "robocopy failed for $($t.Src)" }
}

$ModsStaging = Join-Path $Staging "mods"
New-Item -ItemType Directory -Force -Path $ModsStaging | Out-Null
Copy-Item $ModJar.FullName (Join-Path $ModsStaging $ModJar.Name) -Force
Write-Host "    added mod: $($ModJar.Name)"

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
if (Test-Path $ZipPath) { Remove-Item $ZipPath -Force }

Write-Step "Creating ZIP (this may take several minutes)..."
Push-Location $Staging
try {
    if (Get-Command tar -ErrorAction SilentlyContinue) {
        & tar -acf $ZipPath *
    } else {
        Compress-Archive -Path * -DestinationPath $ZipPath -CompressionLevel Optimal
    }
} finally {
    Pop-Location
}

Remove-Item $Staging -Recurse -Force -ErrorAction SilentlyContinue

$Hash = (Get-FileHash -Path $ZipPath -Algorithm SHA256).Hash.ToLower()
$Size = (Get-Item $ZipPath).Length
$SizeMb = [math]::Round($Size / 1MB, 1)

Write-Host ""
Write-Host "Done!"
Write-Host "  File:   $ZipPath"
Write-Host "  Size:   $SizeMb MB ($Size bytes)"
Write-Host "  SHA256: $Hash"
Write-Host ""
Write-Host "Upload at https://chadow.ru/admin/minecraft"
Write-Host "  Version: $Version"
Write-Host "  ZIP:     $ZipName"
Write-Host ""
Write-Host "After upload the launcher will download this archive automatically via bootstrap API."
