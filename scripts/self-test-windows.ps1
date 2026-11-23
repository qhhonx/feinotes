# Release self-test of the Windows installer: installs it silently into a
# fresh folder (spaces and CJK in its path, as users' have), runs the app's
# built-in checks (src/selftest.rs) on a temporary library, then uninstalls
# and checks the program is gone. Never touches the real library. Usage:
#   scripts/self-test-windows.ps1 -Installer <quiet-md-…-windows-x64-setup.exe> [-Output <dir>]
# The output dir gets report.json, logs, the install log and, for a failed
# scenario, its screenshot. Exit code 0: every scenario passed.
param(
    [Parameter(Mandatory)] [string]$Installer,
    [string]$Output = (Join-Path ([IO.Path]::GetTempPath()) "quiet-md-self-test-$([guid]::NewGuid().ToString('N'))")
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force $Output | Out-Null
$Output = (Resolve-Path $Output).Path
$Installer = (Resolve-Path $Installer).Path
# "quiet-md 自检 安装" spelt in code points: Windows PowerShell reads a
# script without a BOM in the ANSI code page.
$cjk = -join ([char]0x81EA, [char]0x68C0, ' ', [char]0x5B89, [char]0x88C5)
$place = Join-Path ([IO.Path]::GetTempPath()) "quiet-md $cjk $([guid]::NewGuid().ToString('N').Substring(0, 8))"
$exe = Join-Path $place 'quiet_md.exe'

$install = Start-Process -FilePath $Installer -Wait -PassThru -ArgumentList @(
    '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOICONS', "/DIR=`"$place`"", "/LOG=`"$Output\install.log`"")
if ($install.ExitCode -ne 0) { throw "installer exited with $($install.ExitCode); see $Output\install.log" }
if (!(Test-Path -LiteralPath $exe)) { throw "the installer left no $exe" }
Write-Output "installed into $place"

$status = 1
try {
    $run = Start-Process -FilePath $exe -Wait -PassThru -NoNewWindow `
        -ArgumentList @('--self-test', "`"$Output\run`"", "`"$Output\report.json`"") `
        -RedirectStandardOutput "$Output\self-test.txt" -RedirectStandardError "$Output\self-test-err.txt"
    $status = $run.ExitCode
    Get-Content -LiteralPath "$Output\self-test.txt", "$Output\self-test-err.txt" -Encoding UTF8 -ErrorAction SilentlyContinue
} finally {
    # The uninstaller copies itself away and returns at once: wait for the program to go.
    $uninstaller = Join-Path $place 'unins000.exe'
    if (Test-Path -LiteralPath $uninstaller) {
        Start-Process -FilePath $uninstaller -Wait -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') | Out-Null
        $deadline = (Get-Date).AddSeconds(60)
        while ((Test-Path -LiteralPath $exe) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 250 }
    }
}
if (Test-Path -LiteralPath $exe) { throw "uninstalling left $exe behind" }
Write-Output "uninstalled; self-test output: $Output"
exit $status
