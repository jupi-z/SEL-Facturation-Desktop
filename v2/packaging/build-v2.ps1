param(
  [string]$Version = ""
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..\.." )).Path
if (-not $Version) { $Version = (Get-Content (Join-Path $PSScriptRoot "version.txt") -Raw).Trim() }
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Version WiX invalide: $Version" }

Push-Location $root
try {
  cargo build --release --manifest-path v2\Cargo.toml -p sel-local
  python -m PyInstaller --clean --noconfirm --onefile --noconsole `
    --name SEL-Facturation-V2 `
    --icon packaging\sel-facturation.ico `
    --add-data "v2\client;v2\client" `
    --add-binary "v2\target\release\sel-local.exe;." `
    desktop_v2.py
  if ($LASTEXITCODE -ne 0) { throw "PyInstaller a échoué." }

  New-Item -ItemType Directory -Force -Path build\webview2, dist\installer | Out-Null
  $webview = (Resolve-Path build\webview2).Path + "\MicrosoftEdgeWebView2RuntimeInstallerX64.exe"
  Invoke-WebRequest -Uri "https://go.microsoft.com/fwlink/p/?LinkId=2124703" -OutFile $webview
  wix build v2\packaging\Product.wxs `
    -d "ProductVersion=$Version" `
    -d "ExePath=$((Resolve-Path dist\SEL-Facturation-V2.exe).Path)" `
    -o dist\installer\SEL-Facturation-V2.msi
  if ($LASTEXITCODE -ne 0) { throw "WiX MSI V2 a échoué." }
  wix build v2\packaging\Bundle.wxs `
    -ext WixToolset.Bal.wixext -ext WixToolset.Util.wixext `
    -d "ProductVersion=$Version" `
    -d "MsiPath=$((Resolve-Path dist\installer\SEL-Facturation-V2.msi).Path)" `
    -d "WebView2Installer=$webview" `
    -d "IconPath=$((Resolve-Path packaging\sel-facturation.ico).Path)" `
    -o "dist\installer\SEL-Facturation-V2-Setup-$Version.exe"
  if ($LASTEXITCODE -ne 0) { throw "WiX bootstrapper V2 a échoué." }

  & dist\SEL-Facturation-V2.exe --sel-v2-packaging-smoke
  if ($LASTEXITCODE -ne 0) { throw "Smoke test de l'EXE V2 échoué." }
  Get-FileHash dist\SEL-Facturation-V2.exe, dist\installer\SEL-Facturation-V2.msi, "dist\installer\SEL-Facturation-V2-Setup-$Version.exe" -Algorithm SHA256 |
    ForEach-Object { "$($_.Hash)  $($_.Path)" } |
    Set-Content dist\installer\SHA256SUMS.txt -Encoding ascii
}
finally { Pop-Location }
