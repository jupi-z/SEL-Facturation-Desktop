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
$iscc="${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
if (Test-Path $iscc) {
  & $iscc 'packaging\SEL-Facturation.iss'
  if ($LASTEXITCODE -ne 0) { throw 'Création de l’installateur Inno Setup échouée.' }
  Write-Host 'Installateur cree dans dist\installer\' -ForegroundColor Green
} else { Write-Host 'Inno Setup absent : installez-le pour créer Setup.exe.' -ForegroundColor Yellow }
