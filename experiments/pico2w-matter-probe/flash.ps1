param(
    [ValidatePattern('^[0-9A-Fa-f]{16}$')]
    [string]$Serial = '3C10A29F7E389333',
    [switch]$SkipBuild
)

# Windows picotool does not support load -f. Reboot to BOOTSEL separately.
# Every USB operation is restricted to this explicit, previously verified serial.
$probeRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
$probeManifest = Join-Path $PSScriptRoot 'Cargo.toml'
$probeTarget = Join-Path $probeRoot '.local\matter-probe\target'
$probeElf = Join-Path $probeTarget 'thumbv8m.main-none-eabihf\release\pico2w-matter-probe'

if (-not $SkipBuild) {
    & cargo build --release --manifest-path $probeManifest --target-dir $probeTarget --target thumbv8m.main-none-eabihf
    if ($LASTEXITCODE -ne 0) { throw 'Firmware build failed; nothing was written.' }
}
if (-not (Test-Path -LiteralPath $probeElf -PathType Leaf)) { throw 'Built firmware not found.' }

$probeInfo = & picotool info -a --ser $Serial 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "Requesting USB BOOTSEL reset for $Serial"
    & picotool reboot -u --ser $Serial -f
    if ($LASTEXITCODE -ne 0) { throw 'The identified device did not accept the USB reset request.' }
    $probeDeadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        Start-Sleep -Milliseconds 500
        $probeInfo = & picotool info -a --ser $Serial 2>&1
        $probeReady = $LASTEXITCODE -eq 0
    } while (-not $probeReady -and [DateTime]::UtcNow -lt $probeDeadline)
    if (-not $probeReady) { throw 'The identified device did not return in BOOTSEL mode.' }
}

$probeText = $probeInfo -join "`n"
if ($probeText -notmatch 'target chip:\s+RP2350' -or $probeText -notmatch 'boot type:\s+bootsel') {
    throw 'Expected an RP2350 in BOOTSEL mode; nothing was written.'
}
Write-Host "Writing and verifying the identified RP2350: $Serial"
& picotool load -u -v -x $probeElf -t elf --ser $Serial
if ($LASTEXITCODE -ne 0) { throw 'Firmware load or verification failed.' }
Write-Host 'USB automatic flash verified; application rebooted.'
