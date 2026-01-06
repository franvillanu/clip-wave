param(
  [Parameter(Mandatory = $true)]
  [string]$Role,

  [Parameter(Mandatory = $true)]
  [string]$TaskFile
)

$ErrorActionPreference = "Stop"

$roleFile = Join-Path ".sdd\roles" ("$Role.md")

if (-not (Test-Path -LiteralPath $roleFile)) {
  throw "Role file not found: $roleFile"
}

if (-not (Test-Path -LiteralPath $TaskFile)) {
  throw "Task file not found: $TaskFile"
}

Write-Output "=== ROLE ==="
Get-Content -LiteralPath $roleFile
Write-Output ""
Write-Output "=== TASK ==="
Get-Content -LiteralPath $TaskFile
Write-Output ""
Write-Output "=== INSTRUCTION ==="
Write-Output "Follow the role instructions and produce output in the role's required format."