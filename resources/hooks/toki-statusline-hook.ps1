param(
  [Parameter(Mandatory=$true, Position=0)][string]$Tool,
  [Parameter(ValueFromRemainingArguments=$true)][string[]]$OriginalCmd
)

$payload = [Console]::In.ReadToEnd()
$hookDir = Join-Path $HOME ".toki\hooks"
New-Item -ItemType Directory -Force -Path $hookDir | Out-Null
$tmp = Join-Path $hookDir (".{0}.json.tmp.{1}" -f $Tool, $PID)
$dest = Join-Path $hookDir ("{0}.json" -f $Tool)

[System.IO.File]::WriteAllText($tmp, $payload, [System.Text.UTF8Encoding]::new($false))
Move-Item -Force -Path $tmp -Destination $dest

if ($OriginalCmd -and $OriginalCmd.Count -gt 0) {
  $exe = $OriginalCmd[0]
  $rest = if ($OriginalCmd.Count -gt 1) { $OriginalCmd[1..($OriginalCmd.Count - 1)] } else { @() }
  $payload | & $exe @rest
}
