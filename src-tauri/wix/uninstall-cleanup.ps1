param(
  [Parameter(Mandatory = $true)]
  [string]$Target,

  [Parameter(Mandatory = $false)]
  [string]$LogPath
)

$ErrorActionPreference = 'Stop'

function Resolve-LogPath([string]$p) {
  if ($p -and $p.Trim()) { return $p }
  $fallback = Join-Path $env:TEMP 'ClipWave.uninstall-cleanup.log'
  return $fallback
}

function Ensure-DirForFile([string]$p) {
  try {
    $dir = Split-Path -Parent $p
    if ($dir -and -not (Test-Path -LiteralPath $dir)) {
      New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
  } catch {}
}

function Write-Log([string]$p, [string]$m) {
  try {
    Ensure-DirForFile $p
    $ts = [DateTime]::UtcNow.ToString('o')
    Add-Content -LiteralPath $p -Value ("[$ts][$env:USERNAME] $m") -Encoding UTF8
  } catch {
    # last resort: ignore logging failures
  }
}

function Try-RemoveTree([string]$log, [string]$path) {
  Write-Log $log ("Target=" + $path)

  if (-not (Test-Path -LiteralPath $path)) {
    Write-Log $log "Not found."
    return $true
  }

  try {
    Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction Stop
    Write-Log $log "Removed."
    return $true
  } catch {
    Write-Log $log ("Remove-Item failed: " + $_.Exception.Message)
  }

  try {
    Add-Type -Namespace Win32 -Name Native -MemberDefinition @'
[DllImport("kernel32.dll", SetLastError=true, CharSet=CharSet.Unicode)]
public static extern bool MoveFileEx(string existingFileName, string newFileName, int flags);
'@ | Out-Null
  } catch {
    Write-Log $log ("Add-Type MoveFileEx failed: " + $_.Exception.Message)
  }

  try {
    $renamed = $path + '.__delete__' + [DateTime]::UtcNow.Ticks
    Move-Item -LiteralPath $path -Destination $renamed -ErrorAction Stop
    Write-Log $log ("Renamed to " + $renamed)
    try {
      Remove-Item -LiteralPath $renamed -Recurse -Force -ErrorAction SilentlyContinue
    } catch {}
    if (Test-Path -LiteralPath $renamed) {
      try { [Win32.Native]::MoveFileEx($renamed, $null, 4) | Out-Null } catch {}
      Write-Log $log "Scheduled renamed dir for deletion on reboot."
    }
  } catch {
    Write-Log $log ("Rename/remove failed: " + $_.Exception.Message)
    try { [Win32.Native]::MoveFileEx($path, $null, 4) | Out-Null } catch {}
    Write-Log $log "Scheduled target for deletion on reboot."
  }

  if (Test-Path -LiteralPath $path) {
    Write-Log $log "Still exists after cleanup."
    return $false
  }

  Write-Log $log "Gone after cleanup."
  return $true
}

$log = Resolve-LogPath $LogPath
Write-Log $log "--- ClipWave uninstall cleanup ---"
Write-Log $log ("Temp=" + $env:TEMP)
Write-Log $log ("LogPath=" + $log)
Write-Log $log ("UserName=" + $env:USERNAME)
Write-Log $log ("PID=" + $PID)
try { Write-Log $log ("Cwd=" + (Get-Location).Path) } catch {}

if (-not ($Target -and $Target.Trim())) {
  try {
    $fallbackTarget = Join-Path $env:LOCALAPPDATA 'Clip Wave'
    Write-Log $log ("Target not provided; fallback=" + $fallbackTarget)
    $Target = $fallbackTarget
  } catch {}
}

$ok = Try-RemoveTree $log $Target

if (-not $ok) {
  try {
    $fallbackTarget2 = Join-Path $env:LOCALAPPDATA 'Clip Wave'
    if ($fallbackTarget2 -and ($fallbackTarget2 -ne $Target)) {
      Write-Log $log ("Retry fallback=" + $fallbackTarget2)
      $null = Try-RemoveTree $log $fallbackTarget2
    }
  } catch {}
}

exit 0
