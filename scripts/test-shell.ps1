$ErrorActionPreference='Stop'
$taskRoot=Split-Path -Parent $PSScriptRoot
$taskOutput=Join-Path $taskRoot 'output\playwright'
New-Item -ItemType Directory -Path $taskOutput -Force | Out-Null
$taskTemplate=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'shell-check.template.cjs') -Raw -Encoding UTF8
$taskUrl=([uri](Join-Path $taskRoot 'settings-ui\shell.html')).AbsoluteUri
$taskCode=$taskTemplate.Replace('__SHELL_URL__',($taskUrl | ConvertTo-Json -Compress))
$taskFile=Join-Path $taskOutput 'shell-check.cjs'
[IO.File]::WriteAllText($taskFile,$taskCode,[Text.UTF8Encoding]::new($false))
npx.cmd --yes --package @playwright/cli playwright-cli -s=whatrust-shell open about:blank | Out-Null
$taskResult=npx.cmd --yes --package @playwright/cli playwright-cli -s=whatrust-shell run-code --filename $taskFile
$taskResult | Set-Content -LiteralPath (Join-Path $taskOutput 'shell-result.log') -Encoding UTF8
$taskResult | Select-Object -First 3
npx.cmd --yes --package @playwright/cli playwright-cli -s=whatrust-shell close | Out-Null
if (($taskResult -join "`n") -match '### Error') { exit 1 }
