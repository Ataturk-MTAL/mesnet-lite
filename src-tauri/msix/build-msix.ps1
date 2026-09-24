# MESNET.Lite için Microsoft Store MSIX paketi üretir.
#
# Önkoşul: `pnpm tauri build --no-bundle` ile derlenmiş
# src-tauri/target/release/mesnet-lite.exe (arayüz içine gömülü).
#
# Kullanım:
#   pwsh src-tauri/msix/build-msix.ps1 -IdentityName "..." -Publisher "CN=..." `
#     -PublisherDisplayName "..." -OutFile dist-msix/MESNET.Lite_0.1.0_x64.msix
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $IdentityName,
    [Parameter(Mandatory)] [string] $Publisher,
    [Parameter(Mandatory)] [string] $PublisherDisplayName,
    [Parameter(Mandatory)] [string] $OutFile
)

$ErrorActionPreference = 'Stop'
$tauriDir = Resolve-Path (Join-Path $PSScriptRoot '..')

# Sürüm tek kaynaktan: tauri.conf.json. MSIX dört parçalı sürüm ister (x.y.z.0).
$conf = Get-Content (Join-Path $tauriDir 'tauri.conf.json') -Raw | ConvertFrom-Json
if ($conf.version -notmatch '^\d+\.\d+\.\d+$') {
    throw "tauri.conf.json sürümü x.y.z biçiminde değil: $($conf.version)"
}
$version = "$($conf.version).0"

$exe = Join-Path $tauriDir 'target/release/mesnet-lite.exe'
if (-not (Test-Path $exe)) { throw "Derlenmiş program yok: $exe (önce 'pnpm tauri build --no-bundle')" }

# Paket düzeni: kökte program ve bildirim, Assets/ altında Store simgeleri.
$layout = Join-Path ([System.IO.Path]::GetTempPath()) "mesnet-msix-$([guid]::NewGuid())"
$assets = Join-Path $layout 'Assets'
New-Item -ItemType Directory -Path $assets -Force | Out-Null
Copy-Item $exe $layout
foreach ($icon in 'StoreLogo', 'Square44x44Logo', 'Square71x71Logo', 'Square150x150Logo', 'Square310x310Logo') {
    Copy-Item (Join-Path $tauriDir "icons/$icon.png") $assets
}

$manifest = (Get-Content (Join-Path $PSScriptRoot 'AppxManifest.xml.in') -Raw -Encoding UTF8).
    Replace('@IDENTITY_NAME@', $IdentityName).
    Replace('@PUBLISHER@', [System.Security.SecurityElement]::Escape($Publisher)).
    Replace('@PUBLISHER_DISPLAY_NAME@', [System.Security.SecurityElement]::Escape($PublisherDisplayName)).
    Replace('@VERSION@', $version)
if ($manifest -match '@[A-Z_]+@') { throw "Doldurulmamış yer tutucu kaldı: $($Matches[0])" }
[System.IO.File]::WriteAllText((Join-Path $layout 'AppxManifest.xml'), $manifest, [System.Text.UTF8Encoding]::new($false))

# MakeAppx, Windows SDK ile gelir; en yeni x64 sürümü seçilir.
$makeappx = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin\*\x64\makeappx.exe' |
    Sort-Object { [version]($_.Directory.Parent.Name) } -Descending | Select-Object -First 1
if (-not $makeappx) { throw 'makeappx.exe bulunamadı (Windows SDK gerekli).' }

$outDir = Split-Path $OutFile -Parent
if ($outDir) { New-Item -ItemType Directory -Path $outDir -Force | Out-Null }
& $makeappx.FullName pack /d $layout /p $OutFile /o /h SHA256
if ($LASTEXITCODE -ne 0) { throw "makeappx başarısız (çıkış kodu $LASTEXITCODE)" }

Remove-Item $layout -Recurse -Force
Write-Host "MSIX hazır: $OutFile (sürüm $version)"
