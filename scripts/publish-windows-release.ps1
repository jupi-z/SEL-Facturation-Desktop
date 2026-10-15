param(
  [Parameter(Mandatory=$true)]
  [string]$Tag
)

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot\..

$version = (& .\.venv\Scripts\python.exe -c "from version import APP_VERSION; print(APP_VERSION)").Trim()
$exe = (Resolve-Path "dist\SEL-Facturation.exe").Path
$setup = (Resolve-Path "dist\installer\SEL-Facturation-Setup-$version.exe").Path
$msi = (Resolve-Path 'dist\installer\SEL-Facturation.msi').Path
$manifestPath = Join-Path (Split-Path $setup) 'update.json'
$releaseUrl = "https://github.com/jupi-z/SEL-Facturation-Desktop/releases/download/$Tag/SEL-Facturation-Setup-$version.exe"
$hash = (Get-FileHash $setup -Algorithm SHA256).Hash.ToLowerInvariant()
$manifest = [ordered]@{
  schema = 1
  version = $version
  url = $releaseUrl
  sha256 = $hash
  size = (Get-Item $setup).Length
  published_at = (Get-Date).ToUniversalTime().ToString('o')
  release_notes = 'Mise à jour SEL Facturation Desktop.'
}
[IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json), [Text.UTF8Encoding]::new($false))

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { throw 'GitHub CLI (gh) est requis pour publier la release.' }
gh release create $Tag $exe $setup $msi $manifestPath --target (git rev-parse HEAD) --prerelease --title "SEL Facturation Desktop $version" --notes 'Installateur Windows x64 avec WebView2 hors ligne. Vérifier la signature et le poste cible SEL avant livraison finale.'
if ($LASTEXITCODE -ne 0) { throw 'Publication GitHub Release échouée.' }
Write-Host "Release $Tag publiée avec update.json." -ForegroundColor Green
