param(
  [switch]$BuildMsi
)

$ErrorActionPreference='Stop'
Set-Location $PSScriptRoot
py -3.12 -m venv .venv
if ($LASTEXITCODE -ne 0) { throw 'Python 3.12 est requis pour compiler sur Windows.' }
& .\.venv\Scripts\python.exe -m pip install --upgrade pip
if ($LASTEXITCODE -ne 0) { throw 'Installation des outils Python impossible.' }
& .\.venv\Scripts\python.exe -m pip install -r requirements-build.txt pytest
if ($LASTEXITCODE -ne 0) { throw 'Dépendances Python non installées.' }
& .\.venv\Scripts\python.exe -m pytest -q
if ($LASTEXITCODE -ne 0) { throw 'Tests en échec : compilation annulée.' }
& .\.venv\Scripts\pyinstaller.exe --clean --noconfirm --onefile --noconsole --name SEL-Facturation --add-data 'templates;templates' --add-data 'static;static' --hidden-import clr --collect-all webview desktop.py
if ($LASTEXITCODE -ne 0) { throw 'Compilation Windows échouée.' }
Write-Host 'Application de bureau creee dans dist\SEL-Facturation.exe' -ForegroundColor Green

if ($BuildMsi) {
  $wix = Get-Command wix -ErrorAction SilentlyContinue
  if (-not $wix) { throw 'WiX Toolset 4 est requis avec -BuildMsi.' }
  $productVersion = '0.3.0'
  $installerDir = Join-Path $PWD 'dist\installer'
  New-Item -ItemType Directory -Force -Path $installerDir | Out-Null
  $webViewDir = Join-Path $PWD 'build\webview2'
  New-Item -ItemType Directory -Force -Path $webViewDir | Out-Null
  $webViewInstaller = Join-Path $webViewDir 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe'
  if (-not (Test-Path $webViewInstaller)) {
    Invoke-WebRequest -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $webViewInstaller
  }
  & wix build 'packaging\wix\Product.wxs' -d ProductVersion=$productVersion -d ExePath=(Join-Path $PWD 'dist\SEL-Facturation.exe') -o (Join-Path $installerDir 'SEL-Facturation.msi')
  if ($LASTEXITCODE -ne 0) { throw 'Compilation MSI WiX échouée.' }
  & wix build 'packaging\bundle\Bundle.wxs' -ext WixToolset.Bal.wixext -d ProductVersion=$productVersion -d MsiPath=(Join-Path $PWD 'dist\installer\SEL-Facturation.msi') -d WebView2Installer=$webViewInstaller -o (Join-Path $installerDir 'SEL-Facturation-Setup-0.3.0.exe')
  if ($LASTEXITCODE -ne 0) { throw 'Compilation bootstrapper WiX échouée.' }
  Write-Host 'MSI et bootstrapper créés dans dist\installer\' -ForegroundColor Green
}

$iscc="${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
if (Test-Path $iscc) {
  & $iscc 'packaging\SEL-Facturation.iss'
  if ($LASTEXITCODE -ne 0) { throw 'Création de l’installateur Inno Setup échouée.' }
  Write-Host 'Installateur cree dans dist\installer\' -ForegroundColor Green
} else { Write-Host 'Inno Setup absent : installez-le pour créer Setup.exe.' -ForegroundColor Yellow }
