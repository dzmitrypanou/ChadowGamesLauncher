

param(
    [string]$Version = "1.21.11",
    [string]$SourceRoot,
    [string]$OutDir = "$PSScriptRoot\..\dist",
    [switch]$SkipModBuild,
    [switch]$Vanilla,
    [switch]$Download
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path "$PSScriptRoot\.."
$ModDir = Join-Path $Root "client-mod"
$ReferenceRoot = Join-Path $Root "reference-client"
$ZipName = if ($Vanilla) { "minecraft-$Version-client-vanilla.zip" } else { "minecraft-$Version-client.zip" }
$ZipPath = Join-Path $OutDir $ZipName
$Staging = Join-Path $env:TEMP "chadow-full-client-staging"

if (-not $PSBoundParameters.ContainsKey("SourceRoot")) {
    $refJar = Join-Path $ReferenceRoot "versions\$Version\$Version.jar"
    if ((Test-Path $refJar) -and (Test-Path (Join-Path $ReferenceRoot "libraries")) -and (Test-Path (Join-Path $ReferenceRoot "assets"))) {
        $SourceRoot = $ReferenceRoot
    } else {
        $SourceRoot = "$env:APPDATA\ChadowGamesLauncher"
    }
}

function Write-Step($msg) { Write-Host "==> $msg" }

function Get-VanillaVersionJsonText([string]$McVersion) {
    $manifest = Invoke-RestMethod "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json"
    $entry = $manifest.versions | Where-Object { $_.id -eq $McVersion } | Select-Object -First 1
    if (-not $entry) { throw "Version $McVersion not found in Mojang manifest" }
    return (Invoke-WebRequest -Uri $entry.url -UseBasicParsing).Content
}

$VersionDir = Join-Path $SourceRoot "versions\$Version"
$JarPath = Join-Path $VersionDir "$Version.jar"
$LibrariesDir = Join-Path $SourceRoot "libraries"
$AssetsDir = Join-Path $SourceRoot "assets"
$HasLocalInstall = (Test-Path $JarPath) -and (Test-Path $LibrariesDir) -and (Test-Path $AssetsDir)

if ($Download -or (-not $Vanilla -and -not $HasLocalInstall)) {
    if ($Vanilla) { throw "-Download is only supported for modded client packs." }
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
    Write-Step "Downloading full client from Mojang and building ZIP..."
    $buildZipManifest = Join-Path $Root "tools\build-client-zip\Cargo.toml"
    & cargo run --release --manifest-path $buildZipManifest
    if ($LASTEXITCODE -ne 0) { throw "build-client-zip failed" }
    exit 0
}

if (-not $Vanilla) {
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
}

$VersionDir = Join-Path $SourceRoot "versions\$Version"
$JarPath = Join-Path $VersionDir "$Version.jar"
$LibrariesDir = Join-Path $SourceRoot "libraries"
$AssetsDir = Join-Path $SourceRoot "assets"

foreach ($path in @($JarPath, $LibrariesDir, $AssetsDir)) {
    if (-not (Test-Path $path)) {
        throw "Missing: $path`nInstall Minecraft $Version via the launcher first, then re-run this script."
    }
}

Write-Step "Staging client files from $SourceRoot ..."
if (Test-Path $Staging) { Remove-Item $Staging -Recurse -Force }
New-Item -ItemType Directory -Force -Path $Staging | Out-Null

$versionStaging = Join-Path $Staging "versions\$Version"
New-Item -ItemType Directory -Force -Path $versionStaging | Out-Null
Copy-Item $JarPath (Join-Path $versionStaging "$Version.jar") -Force

if ($Vanilla) {
    Write-Step "Fetching vanilla version.json from Mojang..."
    $vanillaJson = Get-VanillaVersionJsonText $Version
    [IO.File]::WriteAllText((Join-Path $versionStaging "$Version.json"), $vanillaJson)
} else {
    $JsonPath = Join-Path $VersionDir "$Version.json"
    if (-not (Test-Path $JsonPath)) { throw "Missing: $JsonPath" }
    Copy-Item $JsonPath (Join-Path $versionStaging "$Version.json") -Force
}

Write-Host "    copy libraries ..."
& robocopy $LibrariesDir (Join-Path $Staging "libraries") /E /NFL /NDL /NJH /NJS /nc /ns /np | Out-Null
if ($LASTEXITCODE -ge 8) { throw "robocopy failed for libraries" }

if ($Vanilla) {
    $fabricLibs = Join-Path $Staging "libraries\net\fabricmc"
    if (Test-Path $fabricLibs) {
        Remove-Item $fabricLibs -Recurse -Force
        Write-Host "    removed Fabric libraries from staging"
    }
}

Write-Host "    copy assets ..."
& robocopy $AssetsDir (Join-Path $Staging "assets") /E /NFL /NDL /NJH /NJS /nc /ns /np | Out-Null
if ($LASTEXITCODE -ge 8) { throw "robocopy failed for assets" }

if (-not $Vanilla) {
    $ModsStaging = Join-Path $Staging "mods"
    New-Item -ItemType Directory -Force -Path $ModsStaging | Out-Null
    Copy-Item $ModJar.FullName (Join-Path $ModsStaging $ModJar.Name) -Force
    Write-Host "    added mod: $($ModJar.Name)"
} else {
    Write-Host "    vanilla build: no mods/"
}

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
Write-Host "  Type:   $(if ($Vanilla) { 'vanilla (no mod, no Fabric)' } else { 'with mod' })"
Write-Host ""
Write-Host "Upload at https://chadow.ru/admin/minecraft"
Write-Host "  Version: $Version"
Write-Host "  ZIP:     $ZipName"
