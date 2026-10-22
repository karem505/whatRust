# whatRust one-line installer — Windows.
#
#   irm https://raw.githubusercontent.com/karem505/whatRust/master/install.ps1 | iex
#
# Downloads the latest published GitHub release installer (NSIS .exe, falling back
# to the .msi) and runs it.
$ErrorActionPreference = 'Stop'
$repo = 'karem505/whatRust'
$app  = 'whatRust'

Write-Host "Installing $app (latest)..." -ForegroundColor Green

try {
  $rel = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" `
    -Headers @{ 'User-Agent' = 'whatrust-installer' }
} catch {
  throw "Couldn't reach the GitHub release. Is $repo public with a published release? ($_)"
}

$asset = $rel.assets | Where-Object { $_.name -match '-setup\.exe$' } | Select-Object -First 1
if (-not $asset) { $asset = $rel.assets | Where-Object { $_.name -match '\.msi$' } | Select-Object -First 1 }
if (-not $asset) { throw "No .exe/.msi installer in the latest release (the build may still be running)." }

$out = Join-Path $env:TEMP $asset.name
Write-Host "Downloading $($asset.name)..."
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $out -UseBasicParsing

# GitHub publishes a SHA-256 digest for each release asset; refuse a mismatch.
$expected = if ($asset.digest -match '^sha256:(?<h>[0-9a-fA-F]{64})$') { $Matches.h } else { $null }
$actual = (Get-FileHash -Algorithm SHA256 -Path $out).Hash
if ($expected) {
  if ($actual -ne $expected) {
    Remove-Item -Force $out
    throw "Checksum mismatch for $($asset.name): expected $expected, got $actual. Aborting."
  }
  Write-Host "SHA-256 verified: $actual"
} else {
  Write-Warning "GitHub published no digest for this asset; SHA-256 is $actual. Compare it with the release notes before trusting the installer."
}

Write-Host "Launching installer..."
if ($asset.name -match '\.msi$') {
  Start-Process msiexec.exe -ArgumentList "/i `"$out`"" -Wait
} else {
  Start-Process -FilePath $out -Wait
}
Write-Host "Done. Launch '$app' from the Start menu." -ForegroundColor Green
