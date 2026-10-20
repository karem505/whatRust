$ErrorActionPreference='Stop'
$taskRoot=Split-Path -Parent $PSScriptRoot
$taskOutput=Join-Path $taskRoot 'output\playwright'
New-Item -ItemType Directory -Path $taskOutput -Force | Out-Null
$taskSource=Get-Content -LiteralPath (Join-Path $taskRoot 'src-tauri\resources\integration.js') -Raw -Encoding UTF8
$taskTemplate=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'adapter-check.template.cjs') -Raw -Encoding UTF8
$taskCode=$taskTemplate.Replace('__ADAPTER_SOURCE__',($taskSource | ConvertTo-Json -Compress))
$taskFile=Join-Path $taskOutput 'adapter-check.cjs'
[IO.File]::WriteAllText($taskFile,$taskCode,[Text.UTF8Encoding]::new($false))
npx.cmd --yes --package @playwright/cli playwright-cli -s=whatrust-adapter open about:blank | Out-Null
$taskResult=npx.cmd --yes --package @playwright/cli playwright-cli -s=whatrust-adapter run-code --filename $taskFile
$taskResult | Set-Content -LiteralPath (Join-Path $taskOutput 'adapter-result.log') -Encoding UTF8
$taskResult | Select-Object -First 3
npx.cmd --yes --package @playwright/cli playwright-cli -s=whatrust-adapter close | Out-Null
if (($taskResult -join "`n") -match '### Error') { exit 1 }
