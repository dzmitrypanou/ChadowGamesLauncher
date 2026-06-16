
param(
    [string]$LogoPath = "$PSScriptRoot\..\src\assets\logo.png",
    [string]$OutDir = "$PSScriptRoot\..\src-tauri\icons"
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

function Save-Bmp {
    param(
        [System.Drawing.Bitmap]$Bitmap,
        [string]$Path
    )

    $dir = Split-Path $Path -Parent
    if (-not (Test-Path $dir)) {
        New-Item -ItemType Directory -Path $dir | Out-Null
    }

    $Bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Bmp)
    $Bitmap.Dispose()
}

function Draw-InstallerHeader {
    param([System.Drawing.Graphics]$Graphics, [int]$Width, [int]$Height, [System.Drawing.Image]$Logo)

    $bg = [System.Drawing.Color]::FromArgb(255, 10, 16, 28)
    $Graphics.Clear($bg)

    $accent = [System.Drawing.Color]::FromArgb(255, 54, 224, 255)
    $brush = New-Object System.Drawing.SolidBrush $accent
    $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(80, 100, 181, 246)), 1
    $Graphics.DrawRectangle($pen, 0, 0, $Width - 1, $Height - 1)
    $pen.Dispose()

    $logoSize = 36
    $Graphics.DrawImage($Logo, 10, [int](($Height - $logoSize) / 2), $logoSize, $logoSize)

    $font = New-Object System.Drawing.Font('Segoe UI', 10, [System.Drawing.FontStyle]::Bold)
    $textBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 240, 244, 250))
    $Graphics.DrawString('Chadow Games Launcher', $font, $textBrush, 54, [int](($Height - $font.Height) / 2))
    $font.Dispose()
    $textBrush.Dispose()
    $brush.Dispose()
}

function Draw-InstallerSidebar {
    param([System.Drawing.Graphics]$Graphics, [int]$Width, [int]$Height, [System.Drawing.Image]$Logo)

    $top = [System.Drawing.Color]::FromArgb(255, 8, 12, 22)
    $bottom = [System.Drawing.Color]::FromArgb(255, 14, 24, 42)
    $rect = New-Object System.Drawing.Rectangle 0, 0, $Width, $Height
    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush $rect, $top, $bottom, 90
    $Graphics.FillRectangle($brush, $rect)
    $brush.Dispose()

    $logoSize = 96
    $x = [int](($Width - $logoSize) / 2)
    $y = [int](($Height - $logoSize) / 2) - 24
    $Graphics.DrawImage($Logo, $x, $y, $logoSize, $logoSize)

    $font = New-Object System.Drawing.Font('Segoe UI', 9, [System.Drawing.FontStyle]::Bold)
    $textBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(220, 100, 181, 246))
    $format = New-Object System.Drawing.StringFormat
    $format.Alignment = [System.Drawing.StringAlignment]::Center
    $Graphics.DrawString('Chadow Games', $font, $textBrush, [single]($Width / 2), [single]($y + $logoSize + 14), $format)
    $font.Dispose()
    $textBrush.Dispose()
}

if (-not (Test-Path $LogoPath)) {
    throw "Logo not found: $LogoPath"
}

$logo = [System.Drawing.Image]::FromFile((Resolve-Path $LogoPath))

$header = New-Object System.Drawing.Bitmap 150, 57
$headerG = [System.Drawing.Graphics]::FromImage($header)
Draw-InstallerHeader -Graphics $headerG -Width 150 -Height 57 -Logo $logo
$headerG.Dispose()
Save-Bmp -Bitmap $header -Path (Join-Path $OutDir 'installer-header.bmp')

$sidebar = New-Object System.Drawing.Bitmap 164, 314
$sidebarG = [System.Drawing.Graphics]::FromImage($sidebar)
Draw-InstallerSidebar -Graphics $sidebarG -Width 164 -Height 314 -Logo $logo
$sidebarG.Dispose()
Save-Bmp -Bitmap $sidebar -Path (Join-Path $OutDir 'installer-sidebar.bmp')

$logo.Dispose()
Write-Host "Generated installer bitmaps in $OutDir"
