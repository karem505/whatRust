$ErrorActionPreference='Stop'
$taskRoot=Split-Path -Parent $PSScriptRoot
$taskOutput=Join-Path $taskRoot 'output\playwright'
New-Item -ItemType Directory -Path $taskOutput -Force | Out-Null
$taskTemplate=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'ui-audit.template.cjs') -Raw -Encoding UTF8
$taskUrl=([uri](Join-Path $taskRoot 'settings-ui\index.html')).AbsoluteUri
$taskCode=$taskTemplate.Replace('__SETTINGS_URL__',($taskUrl | ConvertTo-Json -Compress))
$taskFile=Join-Path $taskOutput 'ui-audit.cjs'
[IO.File]::WriteAllText($taskFile,$taskCode,[Text.UTF8Encoding]::new($false))
npx.cmd --yes --package @playwright/cli playwright-cli -s=whatrust-ui-audit open about:blank | Out-Null
$taskResult=npx.cmd --yes --package @playwright/cli playwright-cli -s=whatrust-ui-audit run-code --filename $taskFile
$taskResult | Set-Content -LiteralPath (Join-Path $taskOutput 'ui-audit-result.log') -Encoding UTF8
$taskResult | Select-Object -First 3
npx.cmd --yes --package @playwright/cli playwright-cli -s=whatrust-ui-audit close | Out-Null
if (($taskResult -join "`n") -match '### Error') { exit 1 }
