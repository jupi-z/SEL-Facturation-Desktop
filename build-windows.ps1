$ErrorActionPreference='Stop'
Set-Location $PSScriptRoot
py -3.12 -m venv .venv
if ($LASTEXITCODE -ne 0) { throw 'Python 3.12 est requis pour compiler sur Windows.' }
& .\.venv\Scripts\python.exe -m pip install --upgrade pip
if ($LASTEXITCODE -ne 0) { throw 'Installation des outils Python impossible.' }
& .\.venv\Scripts\python.exe -m pip install -r requirements-build.txt pytest
if ($LASTEXITCODE -ne 0) { throw 'Dépendances Python non installées.' }
& .\.venv\Scripts\python.exe packaging\prepare-icon.py
if ($LASTEXITCODE -ne 0) { throw 'Generation de l icone Windows echouee.' }
& .\.venv\Scripts\python.exe -m pytest -q
if ($LASTEXITCODE -ne 0) { throw 'Tests en échec : compilation annulée.' }
& .\.venv\Scripts\pyinstaller.exe --clean --noconfirm --onefile --noconsole --name SEL-Facturation --icon 'packaging\sel-facturation.ico' --add-data 'templates;templates' --add-data 'static;static' --hidden-import clr --collect-all webview --collect-all reportlab desktop.py
if ($LASTEXITCODE -ne 0) { throw 'Compilation Windows échouée.' }
& .\dist\SEL-Facturation.exe --sel-packaging-smoke
if ($LASTEXITCODE -ne 0) { throw 'Le contrôle des imports dans l EXE compilé a échoué.' }
Write-Host 'Application de bureau creee dans dist\SEL-Facturation.exe' -ForegroundColor Green
& .\.venv\Scripts\pyinstaller.exe --clean --noconfirm --onefile --noconsole --paths '.' --name SEL-LegacyCleanup packaging\legacy_cleanup.py
if ($LASTEXITCODE -ne 0) { throw 'Compilation du migrateur d installation héritée échouée.' }
Write-Host 'Migrateur d installation héritée créé dans dist\SEL-LegacyCleanup.exe' -ForegroundColor Green
$wix = Get-Command wix -ErrorAction SilentlyContinue
if (-not $wix) { throw 'WiX Toolset 4 est requis pour construire le paquet officiel.' }
$version = (Select-String -Path version.py -Pattern 'APP_VERSION\s*=\s*["'']([^"'']+)["'']').Matches.Groups[1].Value
if ([string]::IsNullOrWhiteSpace($version)) { throw 'APP_VERSION introuvable.' }
New-Item -ItemType Directory -Force -Path 'build\webview2','dist\installer' | Out-Null
$webview = (Resolve-Path 'build\webview2').Path + '\MicrosoftEdgeWebview2Setup.exe'
& .\packaging\download-webview2-bootstrapper.ps1 -Destination $webview
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
