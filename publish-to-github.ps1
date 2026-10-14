# Publie les sources du dossier décompressé dans le dépôt privé GitHub existant.
# Aucun token n'est intégré; Git utilise l'authentification GitHub déjà configurée sur le PC.
$ErrorActionPreference='Stop'
$repo='https://github.com/jupi-z/SEL-Facturation-Desktop.git'
$workspace=Join-Path $env:TEMP ('sel-publish-'+[guid]::NewGuid().ToString('N'))
Write-Host 'Clonage du dépôt privé SEL...' -ForegroundColor Cyan
git clone $repo $workspace
if ($LASTEXITCODE -ne 0) { throw 'Impossible de cloner. Vérifiez Git et votre authentification GitHub.' }
try {
  Get-ChildItem -LiteralPath $PSScriptRoot -Force | Where-Object {
    $_.Name -notin @('.git','.venv','__pycache__','.pytest_cache','build','dist')
  } | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination $workspace -Recurse -Force
  }
  Push-Location $workspace
  try {
    git add -A
    if ($LASTEXITCODE -ne 0) { throw 'Impossible de préparer les fichiers Git.' }
    git diff --cached --quiet
    if ($LASTEXITCODE -eq 0) { Write-Host 'Aucun changement à publier.'; return }
    git commit -m 'feat: add SEL Facturation Desktop V1'
    if ($LASTEXITCODE -ne 0) { throw 'Commit refusé. Configurez git user.name et git user.email.' }
    git push origin main
    if ($LASTEXITCODE -ne 0) { throw 'Push refusé. Vérifiez les permissions ou la branche distante.' }
    Write-Host 'Code source publié sur GitHub. Vérifiez ensuite Actions > Build Windows Desktop.' -ForegroundColor Green
  } finally { Pop-Location }
} finally {
  Remove-Item -LiteralPath $workspace -Recurse -Force -ErrorAction SilentlyContinue
}
