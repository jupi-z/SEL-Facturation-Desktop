param(
  [string]$ComposeFile = (Join-Path $PSScriptRoot "compose.production.example.yml")
)

$ErrorActionPreference = "Stop"
$required = @(
  "POSTGRES_DB", "POSTGRES_ADMIN_USER", "POSTGRES_ADMIN_PASSWORD",
  "POSTGRES_APP_USER", "POSTGRES_APP_PASSWORD", "SEL_OFFLINE_GRANT_PUBLIC_KEY",
  "SEL_ORGANIZATION_ID", "SEL_ORGANIZATION_NAME"
)
$missing = $required | Where-Object { [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($_)) }
if ($missing) { throw "Configuration de production incomplète: $($missing -join ', ')" }
if (-not (Test-Path -LiteralPath $ComposeFile)) { throw "Compose introuvable: $ComposeFile" }
docker compose -f $ComposeFile config --quiet
if ($LASTEXITCODE -ne 0) { throw "Configuration Compose invalide." }
Write-Output "Configuration de production syntaxiquement valide; aucun déploiement n'a été exécuté."
