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
& .\.venv\Scripts\pyinstaller.exe --clean --noconfirm --onefile --noconsole --name SEL-Facturation --icon 'packaging\sel-facturation.ico' --add-data 'templates;templates' --add-data 'static;static' --hidden-import clr --collect-all webview desktop.py
if ($LASTEXITCODE -ne 0) { throw 'Compilation Windows échouée.' }
Write-Host 'Application de bureau creee dans dist\SEL-Facturation.exe' -ForegroundColor Green
$wix = Get-Command wix -ErrorAction SilentlyContinue
if (-not $wix) { throw 'WiX Toolset 4 est requis pour construire le paquet officiel.' }
$version = (Select-String -Path version.py -Pattern 'APP_VERSION\s*=\s*["'']([^"'']+)["'']').Matches.Groups[1].Value
if ([string]::IsNullOrWhiteSpace($version)) { throw 'APP_VERSION introuvable.' }
New-Item -ItemType Directory -Force -Path 'build\webview2','dist\installer' | Out-Null
$webview = (Resolve-Path 'build\webview2').Path + '\MicrosoftEdgeWebView2RuntimeInstallerX64.exe'
if (-not (Test-Path $webview)) {
  Invoke-WebRequest -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $webview
}
& wix build packaging\wix\Product.wxs -d ProductVersion=$version -d ExePath="$(Resolve-Path 'dist\SEL-Facturation.exe').Path" -o 'dist\installer\SEL-Facturation.msi'
if ($LASTEXITCODE -ne 0) { throw 'Compilation MSI WiX échouée.' }
& wix build packaging\bundle\Bundle.wxs -ext WixToolset.Bal.wixext -ext WixToolset.Util.wixext -d ProductVersion=$version -d MsiPath="$(Resolve-Path 'dist\installer\SEL-Facturation.msi').Path" -d WebView2Installer=$webview -o "dist\installer\SEL-Facturation-Setup-$version.exe"
if ($LASTEXITCODE -ne 0) { throw 'Compilation bootstrapper WiX échouée.' }
Write-Host "Installateur officiel créé dans dist\installer\SEL-Facturation-Setup-$version.exe" -ForegroundColor Green
