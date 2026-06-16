
param(
    [string]$Version = "1.0.0",
    [string]$OutDir = "$PSScriptRoot\..\dist"
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path "$PSScriptRoot\.."
$ModDir = Join-Path $Root "client-mod"
$PackName = "chadow-client-pack-$Version.zip"
$PackPath = Join-Path $OutDir $PackName

Write-Host "==> Building mod (Gradle)..."
Push-Location $ModDir
try {
    & .\gradlew.bat build --no-daemon
    if ($LASTEXITCODE -ne 0) { throw "Gradle build failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}

$Jar = Get-ChildItem (Join-Path $ModDir "build\libs") -Filter "chadow-games-client-*.jar" |
    Where-Object { $_.Name -notmatch "sources" } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

if (-not $Jar) {
    throw "Mod JAR not found in client-mod\build\libs"
}

Write-Host "==> Mod JAR: $($Jar.FullName)"

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$Staging = Join-Path $env:TEMP "chadow-client-pack-staging"
if (Test-Path $Staging) { Remove-Item $Staging -Recurse -Force }
New-Item -ItemType Directory -Force -Path (Join-Path $Staging "mods") | Out-Null
Copy-Item $Jar.FullName (Join-Path $Staging "mods\$($Jar.Name)") -Force

if (Test-Path $PackPath) { Remove-Item $PackPath -Force }
Compress-Archive -Path (Join-Path $Staging "*") -DestinationPath $PackPath -Force
Remove-Item $Staging -Recurse -Force

$Hash = (Get-FileHash -Path $PackPath -Algorithm SHA256).Hash.ToLower()
$Size = (Get-Item $PackPath).Length

Write-Host ""
Write-Host "Done:"
Write-Host "  Archive: $PackPath"
Write-Host "  SHA256:  $Hash"
Write-Host "  Size:    $Size bytes"
Write-Host ""
Write-Host "Upload the zip and set bootstrap clientPack (version 1.21.11):"
Write-Host "  url:    https://chadow.ru/.../chadow-client-pack-$Version.zip"
Write-Host "  sha256: $Hash"
Write-Host "  size:   $Size"
