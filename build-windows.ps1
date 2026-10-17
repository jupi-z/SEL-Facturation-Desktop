$ErrorActionPreference='Stop'
Set-Location $PSScriptRoot
py -3.12 -m venv .venv
if ($LASTEXITCODE -ne 0) { throw 'Python 3.12 est requis pour compiler sur Windows.' }
& .\.venv\Scripts\python.exe -m pip install --upgrade pip
if ($LASTEXITCODE -ne 0) { throw 'Installation des outils Python impossible.' }
& .\.venv\Scripts\python.exe -m pip install -r requirements-build.txt pytest
if ($LASTEXITCODE -ne 0) { throw 'Dépendances Python non installées.' }
& .\.venv\Scripts\python.exe packaging\prepare-icon.py
if ($LASTEXITCODE -ne 0) { throw 'Génération de l’icône Windows échouée.' }
& .\.venv\Scripts\python.exe -m pytest -q
if ($LASTEXITCODE -ne 0) { throw 'Tests en échec : compilation annulée.' }
& .\.venv\Scripts\pyinstaller.exe --clean --noconfirm --onefile --noconsole --name SEL-Facturation --icon 'packaging\sel-facturation.ico' --add-data 'templates;templates' --add-data 'static;static' --hidden-import clr --collect-all webview desktop.py
if ($LASTEXITCODE -ne 0) { throw 'Compilation Windows échouée.' }
Write-Host 'Application de bureau creee dans dist\SEL-Facturation.exe' -ForegroundColor Green
& .\.venv\Scripts\pyinstaller.exe --clean --noconfirm --onefile --noconsole --paths '.' --name SEL-LegacyCleanup packaging\legacy_cleanup.py
if ($LASTEXITCODE -ne 0) { throw 'Compilation du migrateur d installation héritée échouée.' }
Write-Host 'Migrateur d installation héritée créé dans dist\SEL-LegacyCleanup.exe' -ForegroundColor Green
$wix = Get-Command wix -ErrorAction SilentlyContinue
if (-not $wix) { throw 'WiX Toolset 4 est requis pour construire le paquet officiel.' }
$version = (Select-String -Path version.py -Pattern 'APP_VERSION\s*=\s*["'']([^"'']+)["'']').Matches.Groups[1].Value
if ([string]::IsNullOrWhiteSpace($version)) { throw 'APP_VERSION introuvable.' }
New-Item -ItemType Directory -Force -Path 'build\webview2','dist\installer' | Out-Null
$webview = (Resolve-Path 'build\webview2').Path + '\MicrosoftEdgeWebView2RuntimeInstallerX64.exe'
if (-not (Test-Path $webview)) {
  Invoke-WebRequest -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $webview
}
$exePath = (Resolve-Path 'dist\SEL-Facturation.exe').Path
$legacyCleanupPath = (Resolve-Path 'dist\SEL-LegacyCleanup.exe').Path
& wix build packaging\wix\Product.wxs -d "ProductVersion=$version" -d "ExePath=$exePath" -o 'dist\installer\SEL-Facturation.msi'
if ($LASTEXITCODE -ne 0) { throw 'Compilation MSI WiX échouée.' }
$msiPath = (Resolve-Path 'dist\installer\SEL-Facturation.msi').Path
& wix build packaging\bundle\Bundle.wxs -ext WixToolset.Bal.wixext -ext WixToolset.Util.wixext -d "ProductVersion=$version" -d "MsiPath=$msiPath" -d "WebView2Installer=$webview" -d "LegacyCleanupPath=$legacyCleanupPath" -d "IconPath=$((Resolve-Path 'packaging\sel-facturation.ico').Path)" -o "dist\installer\SEL-Facturation-Setup-${version}-core.exe"
if ($LASTEXITCODE -ne 0) { throw 'Compilation bootstrapper WiX échouée.' }
$corePath = (Resolve-Path "dist\installer\SEL-Facturation-Setup-${version}-core.exe").Path
& .\.venv\Scripts\pyinstaller.exe --clean --noconfirm --onefile --noconsole --paths '.' --name "SEL-Facturation-Setup-$version" --icon 'packaging\sel-facturation.ico' --add-binary "$corePath;." --add-data 'version.py;.' packaging\preflight_launcher.py
if ($LASTEXITCODE -ne 0) { throw 'Compilation du pré-vérificateur d installation échouée.' }
Move-Item -LiteralPath "dist\SEL-Facturation-Setup-${version}.exe" -Destination "dist\installer\SEL-Facturation-Setup-${version}.exe" -Force
Write-Host "Installateur officiel créé dans dist\installer\SEL-Facturation-Setup-$version.exe" -ForegroundColor Green
