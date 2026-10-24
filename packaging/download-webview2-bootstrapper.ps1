param(
  [Parameter(Mandatory = $true)]
  [string] $Destination
)

$ErrorActionPreference = 'Stop'
$downloadUrl = 'https://go.microsoft.com/fwlink/p/?LinkId=2124703'
$destinationPath = [IO.Path]::GetFullPath($Destination)
$destinationDirectory = Split-Path -Parent $destinationPath
$partialPath = "$destinationPath.download"
$minimumBytes = 512KB
$maximumBytes = 10MB
$expectedSigner = 'CN=Microsoft Corporation'

New-Item -ItemType Directory -Force -Path $destinationDirectory | Out-Null

for ($attempt = 1; $attempt -le 3; $attempt++) {
  try {
    Remove-Item -LiteralPath $partialPath -Force -ErrorAction SilentlyContinue
    Invoke-WebRequest -Uri $downloadUrl -OutFile $partialPath -TimeoutSec 90 -UseBasicParsing

    $payload = Get-Item -LiteralPath $partialPath
    if ($payload.Length -lt $minimumBytes -or $payload.Length -gt $maximumBytes) {
      throw "Taille inattendue du bootstrapper WebView2 ($($payload.Length) octets)."
    }

    $signature = Get-AuthenticodeSignature -LiteralPath $partialPath
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notlike "$expectedSigner,*") {
    throw 'WebView2 bootstrapper has an invalid Authenticode signature or an unexpected publisher.'
    }

    Move-Item -LiteralPath $partialPath -Destination $destinationPath -Force
    Write-Host "Verified Microsoft WebView2 bootstrapper: $destinationPath ($($payload.Length) bytes)."
    return
  }
  catch {
    Remove-Item -LiteralPath $partialPath -Force -ErrorAction SilentlyContinue
    if ($attempt -eq 3) {
      throw "Could not download or validate the WebView2 bootstrapper after $attempt attempts. Check HTTPS access to Microsoft servers. Details: $($_.Exception.Message)"
    }
    Start-Sleep -Seconds (2 * $attempt)
  }
}
