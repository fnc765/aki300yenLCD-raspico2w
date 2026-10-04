param([switch]$Provision, [switch]$Tbyb)
$ErrorActionPreference = 'Stop'
$tickerRepo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$tickerPrivateDir = Join-Path $tickerRepo '.local\ticker-provision'
$tickerTargetDir = Join-Path $tickerRepo '.local\ticker-target'
$tickerOldProvision = $env:TICKER_PROVISION_DIR
Push-Location $tickerRepo
try {
    if ($Provision) {
        & python -X utf8 scripts/prepare-matter-config.py --out $tickerPrivateDir
        if ($LASTEXITCODE -ne 0) { throw 'Private configuration preparation failed.' }
        $env:TICKER_PROVISION_DIR = $tickerPrivateDir
    } else {
        $env:TICKER_PROVISION_DIR = $null
    }
    $tickerBuildArgs = @('build', '--release', '--locked', '--bin', 'ticker', '--target-dir', $tickerTargetDir)
    if ($Tbyb) { $tickerBuildArgs += @('--features', 'tbyb') }
    & cargo @tickerBuildArgs
    if ($LASTEXITCODE -ne 0) { throw 'Ticker build failed; nothing was flashed.' }
    Write-Host "Local ELF: $tickerTargetDir\thumbv8m.main-none-eabihf\release\ticker"
    if ($Provision) { Write-Host 'PRIVATE firmware: contains Wi-Fi/controller identity. Never upload this ELF/UF2.' }
} finally {
    $env:TICKER_PROVISION_DIR = $tickerOldProvision
    Pop-Location
}
