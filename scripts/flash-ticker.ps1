param(
    [Parameter(Mandatory)][ValidatePattern('^[0-9A-Fa-f]{16}$')][string]$Serial,
    [Parameter(Mandatory)][string]$Elf,
    [ValidateRange(0, 1)][int]$Partition = 0
)
$ErrorActionPreference = 'Stop'
$tickerRepo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$tickerElfPath = (Resolve-Path -LiteralPath $Elf).Path
$tickerDeviceInfo = & picotool info -a --ser $Serial 2>&1
if ($LASTEXITCODE -ne 0) {
    & picotool reboot -u --ser $Serial -f
    if ($LASTEXITCODE -ne 0) { throw 'Identified device did not accept automatic USB BOOTSEL reset.' }
    $tickerDeadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        Start-Sleep -Milliseconds 500
        $tickerDeviceInfo = & picotool info -a --ser $Serial 2>&1
        $tickerReady = $LASTEXITCODE -eq 0
    } while (-not $tickerReady -and [DateTime]::UtcNow -lt $tickerDeadline)
    if (-not $tickerReady) { throw 'Identified Pico did not enter BOOTSEL.' }
}
$tickerInfoText = $tickerDeviceInfo -join "`n"
if ($tickerInfoText -notmatch 'target chip:\s+RP2350' -or $tickerInfoText -notmatch 'boot type:\s+bootsel' -or $tickerInfoText -notmatch 'flash size:\s+4096K') {
    throw 'Expected a 4 MB RP2350 in BOOTSEL; nothing was written.'
}
$tickerBackupDir = Join-Path $tickerRepo '.local\ticker-device'
New-Item -ItemType Directory -Path $tickerBackupDir -Force | Out-Null
$tickerBackupPath = Join-Path $tickerBackupDir "before-power-$Serial.bin"
if (-not (Test-Path -LiteralPath $tickerBackupPath)) {
    & picotool save -a $tickerBackupPath -t bin --ser $Serial
    if ($LASTEXITCODE -ne 0) { throw 'Backup failed; nothing was flashed.' }
}
# Explicit A/B partition preserves the other image and the data partition.
& picotool load -u -v -x -p $Partition $tickerElfPath -t elf --ser $Serial
if ($LASTEXITCODE -ne 0) { throw 'Ticker flash or verification failed.' }
Write-Host "Ticker verified on RP2350 $Serial, partition $Partition. Backup: $tickerBackupPath"
