param(
  [Parameter(Mandatory=$true, Position=0)][string]$Tool,
  [Parameter(ValueFromRemainingArguments=$true)][string[]]$OriginalCmd
)

$utf8NoBom = [System.Text.UTF8Encoding]::new($false)
$reader = [System.IO.StreamReader]::new([Console]::OpenStandardInput(), $utf8NoBom)
$payload = $reader.ReadToEnd()

$homeDir = if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }
$hookDir = Join-Path $homeDir ".toki\hooks"
New-Item -ItemType Directory -Force -Path $hookDir | Out-Null
$tmp = Join-Path $hookDir (".{0}.json.tmp.{1}" -f $Tool, $PID)
$dest = Join-Path $hookDir ("{0}.json" -f $Tool)

[System.IO.File]::WriteAllText($tmp, $payload, $utf8NoBom)
Move-Item -Force -Path $tmp -Destination $dest

if ($OriginalCmd -and $OriginalCmd.Count -gt 0) {
  $OutputEncoding = $utf8NoBom
  $exe = $OriginalCmd[0]
  $rest = if ($OriginalCmd.Count -gt 1) { $OriginalCmd[1..($OriginalCmd.Count - 1)] } else { @() }
  $payload | & $exe @rest
}
