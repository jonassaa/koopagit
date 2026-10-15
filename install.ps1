<#
.SYNOPSIS
KoopaGit setup wizard for Windows (macOS and Linux: install.sh).

.DESCRIPTION
Checks every prerequisite, installs the missing ones (Git, the Visual Studio
C++ Build Tools, Rust), builds KoopaGit from source, installs it, optionally
registers the experimental Explorer extension, and verifies the result.
Choose the recommended setup to accept every default with one key press, or
customize the install location and each option. Run it again to update.

Building runs as you; only installing into Program Files and registering the
Explorer extension ask for administrator rights (one UAC prompt).

Outside a checkout it clones the sources into $env:KOOPAGIT_SRC (default:
%LOCALAPPDATA%\KoopaGit\src), so it also works as
    irm https://raw.githubusercontent.com/jonassaa/koopagit/main/install.ps1 | iex

.EXAMPLE
powershell -ExecutionPolicy Bypass -File .\install.ps1
.EXAMPLE
.\install.ps1 -Check
.EXAMPLE
.\install.ps1 -Uninstall
#>
[CmdletBinding()]
param(
    # Only report what is installed and what is missing.
    [switch]$Check,
    # Remove KoopaGit.
    [switch]$Uninstall,
    # Use the recommended setup without asking.
    [switch]$Yes,
    # Register the (experimental) Explorer extension without asking.
    [switch]$Explorer,
    # Install location.
    [string]$Dest = "$env:ProgramFiles\KoopaGit",
    # Internal: the step run in the elevated process (install | uninstall).
    [string]$ElevatedStep = '',
    [string]$Source = '',
    [string]$Log = ''
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$MinRust = [version]'1.92'
$RepoUrl = 'https://github.com/jonassaa/koopagit.git'
$MenuClsid = '{5e1a8f00-6b4c-4d1f-9a2e-7c0d3b1e4a00}'
$Binaries = 'koopagit-proc.exe', 'koopagit-merge.exe', 'koopagit_explorer.dll'
$Crates = 'koopagit-proc', 'koopagit-merge', 'koopagit-explorer'
$ProgramFilesDest = "$env:ProgramFiles\KoopaGit"
$UserDest = "$env:LOCALAPPDATA\Programs\KoopaGit"
$DestGiven = $PSBoundParameters.ContainsKey('Dest')

# ---------------------------------------------------------------- terminal

# The file stays ASCII (Windows PowerShell 5.1 misreads UTF-8 without a BOM),
# so the glyphs are built from code points.
$G = @{
    Ok = [string][char]0x221A; Fail = 'x'; Warn = '!'
    Now = [string][char]0x25CF; Todo = [string][char]0x25CB
    Line = [string][char]0x2501; Full = [string][char]0x2588; Empty = [string][char]0x2591
}
try { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch { }

# The full-screen wizard (screens, spinners, arrow-key menus) needs a real
# console; otherwise everything is printed line by line.
$Fancy = (-not $Check) -and (-not $ElevatedStep) -and ($Host.Name -eq 'ConsoleHost') -and
    -not [Console]::IsOutputRedirected -and -not [Console]::IsInputRedirected
$Steps = @('Check', 'Setup', 'Prepare', 'Build', 'Install', 'Finish')
$LogDir = Join-Path $env:LOCALAPPDATA 'KoopaGit'
$LogFile = Join-Path $LogDir 'install.log'

function Get-Width { try { [Math]::Max(40, [Console]::WindowWidth) } catch { 80 } }

# Write one status line in place, padded so it overwrites what was there.
function Write-InPlace([string]$text, [string]$color = 'Gray') {
    $w = (Get-Width) - 1
    if ($text.Length -gt $w) { $text = $text.Substring(0, $w) }
    Write-Host ("`r" + $text.PadRight($w)) -NoNewline -ForegroundColor $color
}

function Format-Time([TimeSpan]$t) {
    if ($t.TotalSeconds -ge 60) { '{0}m {1:00}s' -f [int][Math]::Floor($t.TotalMinutes), $t.Seconds }
    else { '{0}s' -f [int]$t.TotalSeconds }
}

function Show-Screen([int]$n, [string]$title) {
    $count = $script:Steps.Count
    if (-not $Fancy) { Write-Host ''; Write-Host "==> [$n/$count] $title" -ForegroundColor Cyan; return }
    Clear-Host
    Write-Host ''
    Write-Host '  KoopaGit Setup' -NoNewline -ForegroundColor White
    Write-Host "  $script:PlatformLabel" -ForegroundColor DarkGray
    Write-Host ''
    $w = [Math]::Min((Get-Width) - 6, 64)
    $filled = [int][Math]::Floor($w * $n / $count)
    Write-Host '  ' -NoNewline
    Write-Host ($G.Line * $filled) -NoNewline -ForegroundColor Cyan
    Write-Host ($G.Line * ($w - $filled)) -ForegroundColor DarkGray
    Write-Host '  ' -NoNewline
    for ($i = 1; $i -le $count; $i++) {
        $name = $script:Steps[$i - 1]
        if ($i -lt $n) { Write-Host "$($G.Ok) $name   " -NoNewline -ForegroundColor Green }
        elseif ($i -eq $n) { Write-Host "$($G.Now) $name   " -NoNewline -ForegroundColor Cyan }
        else { Write-Host "$($G.Todo) $name   " -NoNewline -ForegroundColor DarkGray }
    }
    Write-Host ''
    Write-Host ''
    Write-Host "  Step $n of $count - $title" -ForegroundColor White
    Write-Host ''
}

# ---------------------------------------------------------------- output

function Ok($m) { Write-Host "  $($G.Ok) $m" -ForegroundColor Green }
function Warn($m) { Write-Host "  $($G.Warn) $m" -ForegroundColor Yellow }
function Bad($m) { Write-Host "  $($G.Fail) $m" -ForegroundColor Red }
function Info($m) { Write-Host "    $m" }
function Note($m) { Write-Host "    $m" -ForegroundColor DarkGray }
function Title($m) { Write-Host ''; Write-Host "  $m" -ForegroundColor White }
# Throw rather than exit: under `irm | iex`, exit would close the user's window.
function Fail($m) { throw $m }

# In the recommended setup every yes/no question takes its default.
$script:Auto = $false

function Confirm-Choice([string]$Question, [bool]$Default = $true) {
    if ($Yes -or $script:Auto) { return $Default }
    $hint = if ($Default) { '[Y/n]' } else { '[y/N]' }
    while ($true) {
        $r = Read-Host "  ? $Question $hint"
        if ($r -eq '') { return $Default }
        if ($r -match '^[yY]') { return $true }
        if ($r -match '^[nN]') { return $false }
    }
}

function Ask([string]$Question, [string]$Default) {
    if ($Yes) { return $Default }
    $r = Read-Host "  ? $Question [$Default]"
    if ($r) { return $r } else { return $Default }
}

# Arrow keys (or the item's number) move, Enter picks. Items are
# "Label|Description"; returns the chosen item's number.
function Select-Menu([int]$Default, [string[]]$Items) {
    if ($Yes) { return $Default }
    $labels = $Items | ForEach-Object { ($_ -split '\|', 2)[0] }
    $pad = ($labels | Measure-Object -Property Length -Maximum).Maximum
    if (-not $Fancy) {
        for ($i = 0; $i -lt $Items.Count; $i++) {
            $p = $Items[$i] -split '\|', 2
            Write-Host ("    {0}) {1}  {2}" -f ($i + 1), $p[0].PadRight($pad), $p[1])
        }
        while ($true) {
            $r = Read-Host "  ? Choice [$Default]"
            if ($r -eq '') { return $Default }
            if ($r -match '^\d+$' -and [int]$r -ge 1 -and [int]$r -le $Items.Count) { return [int]$r }
        }
    }
    $sel = $Default
    $width = (Get-Width) - 1
    $draw = {
        for ($i = 0; $i -lt $Items.Count; $i++) {
            $p = $Items[$i] -split '\|', 2
            $desc = if ($p.Count -gt 1) { $p[1] } else { '' }
            $line = '    ' + $p[0].PadRight($pad) + '  ' + $desc
            if ($i + 1 -eq $sel) {
                Write-Host '  > ' -NoNewline -ForegroundColor Cyan
                Write-Host $p[0].PadRight($pad) -NoNewline -ForegroundColor White
                Write-Host ('  ' + $desc).PadRight([Math]::Max(0, $width - 4 - $pad))
            } else {
                Write-Host ('    ' + $p[0].PadRight($pad)) -NoNewline
                Write-Host ('  ' + $desc).PadRight([Math]::Max(0, $width - 4 - $pad)) -ForegroundColor DarkGray
            }
        }
    }
    [Console]::CursorVisible = $false
    try {
        & $draw
        # Measured after drawing, in case drawing scrolled the window.
        $top = [Console]::CursorTop - $Items.Count
        while ($true) {
            $k = [Console]::ReadKey($true)
            if ($k.Key -eq 'Enter') { return $sel }
            if ($k.Key -eq 'UpArrow' -or $k.KeyChar -eq 'k') { $sel = if ($sel -le 1) { $Items.Count } else { $sel - 1 } }
            elseif ($k.Key -eq 'DownArrow' -or $k.KeyChar -eq 'j') { $sel = if ($sel -ge $Items.Count) { 1 } else { $sel + 1 } }
            elseif ("$($k.KeyChar)" -match '^[1-9]$' -and [int]"$($k.KeyChar)" -le $Items.Count) { $sel = [int]"$($k.KeyChar)" }
            [Console]::SetCursorPosition(0, $top)
            & $draw
        }
    } finally { [Console]::CursorVisible = $true }
}

# ---------------------------------------------------------------- tasks

# Quote arguments for Start-Process, which takes a single command line.
function Join-Arguments([string[]]$Arguments) {
    ($Arguments | ForEach-Object {
            if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ }
        }) -join ' '
}

# Run a native command, show it, and fail on a non-zero exit code.
function Invoke-Native([string]$Exe, [string[]]$Arguments) {
    Write-Host "    > $Exe $($Arguments -join ' ')" -ForegroundColor DarkGray
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { Fail "$Exe exited with code $LASTEXITCODE" }
}

# Read a file another process is still writing.
function Read-Shared([string]$path) {
    if (-not (Test-Path $path)) { return '' }
    $fs = [IO.File]::Open($path, 'Open', 'Read', 'ReadWrite, Delete')
    try { (New-Object IO.StreamReader($fs)).ReadToEnd() } finally { $fs.Dispose() }
}

function Show-LogTail {
    Write-Host ''
    Write-Host '  End of the log:' -ForegroundColor DarkGray
    Get-Content $LogFile -Tail 25 | ForEach-Object { Write-Host "    $_" }
}

# Run a native command behind a spinner (or, with -Build, a progress bar fed by
# cargo's "Compiling" lines), its output going to the log. For the bar, the
# dependencies ($Totals[0] crates) fill the first 60%; KoopaGit's own crates
# ($Totals[1]), which take most of the time, share the last 40%.
function Invoke-Task([string]$Label, [string]$Exe, [string[]]$Arguments, [int[]]$Totals = @(0, 0), [switch]$Build) {
    Add-Content $LogFile "`r`n> $Exe $($Arguments -join ' ')"
    if (-not $Fancy) {
        Info "$Label..."
        Invoke-Native $Exe $Arguments
        Ok $Label
        return
    }
    $path = (Get-Command $Exe -CommandType Application | Select-Object -First 1).Source
    $out = "$LogFile.out"
    $err = "$LogFile.err"
    $p = Start-Process -FilePath $path -ArgumentList (Join-Arguments $Arguments) -NoNewWindow -PassThru `
        -RedirectStandardOutput $out -RedirectStandardError $err
    $null = $p.Handle # keeps ExitCode available after exit
    if ($Build) { Write-Host "  $Label" }
    $code = Wait-Task $Label $p $err $Totals ([bool]$Build)
    foreach ($f in $out, $err) {
        if (Test-Path $f) { Add-Content $LogFile (Get-Content $f -Raw); Remove-Item $f }
    }
    if ($code -ne 0) { Show-LogTail; Fail "$Label failed (exit code $code)" }
}

function Wait-Task([string]$Label, $Proc, [string]$ErrFile, [int[]]$Totals, [bool]$Build) {
    $frames = '|', '/', '-', '\'
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $n = 0
    $barW = [Math]::Max(10, [Math]::Min((Get-Width) - 6, 64) - 32)
    $pct = 0
    $crate = ''
    [Console]::CursorVisible = $false
    try {
        while (-not $Proc.HasExited) {
            if ($Build) {
                if ($n % 4 -eq 0) {
                    $lines = @((Read-Shared $ErrFile) -split "`n" | Where-Object { $_ -match '^\s*Compiling ' })
                    $ws = @($lines | Where-Object { $_.Contains("($script:Src\") }).Count
                    $deps = $lines.Count - $ws
                    $crate = if ($lines.Count) { ($lines[-1].Trim() -split '\s+')[1] } else { '' }
                    $pct = if ($Totals[0] -gt 0) { [Math]::Min(60, [int][Math]::Floor($deps * 60 / $Totals[0])) } else { 0 }
                    if ($Totals[1] -gt 0) { $pct += [int][Math]::Floor($ws * 40 / $Totals[1]) }
                    $pct = [Math]::Min(99, $pct)
                }
                $filled = [int]($barW * $pct / 100)
                Write-InPlace ('  ' + $frames[$n % 4] + ' ' + ($G.Full * $filled) + ($G.Empty * ($barW - $filled)) +
                    (' {0,3}%  {1,-18} {2}' -f $pct, $crate, (Format-Time $sw.Elapsed))) Cyan
                Start-Sleep -Milliseconds 125
            } else {
                Write-InPlace ("  {0} {1}  {2}" -f $frames[$n % 4], $Label, (Format-Time $sw.Elapsed)) Cyan
                Start-Sleep -Milliseconds 120
            }
            $n++
        }
        $Proc.WaitForExit()
    } finally { [Console]::CursorVisible = $true }
    if ($Build) { Write-InPlace ('    ' + ($G.Full * $barW) + ' 100%') Cyan; Write-Host '' }
    if ($Proc.ExitCode -eq 0) {
        Write-InPlace ("  {0} {1}  {2}" -f $G.Ok, $Label, (Format-Time $sw.Elapsed)) Green
    } else {
        Write-InPlace ("  {0} {1}" -f $G.Fail, $Label) Red
    }
    Write-Host ''
    return $Proc.ExitCode
}

# ---------------------------------------------------------------- helpers

function Test-Admin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    ([Security.Principal.WindowsPrincipal]$id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Test-Command($name) { [bool](Get-Command $name -ErrorAction SilentlyContinue) }

$CargoBin = if ($env:CARGO_HOME) { "$env:CARGO_HOME\bin" } else { "$env:USERPROFILE\.cargo\bin" }

# Pick up programs installed by winget or rustup during this session.
function Update-SessionPath {
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
        [Environment]::GetEnvironmentVariable('Path', 'User')
    if ((Test-Path $CargoBin) -and ($env:Path -notlike "*$CargoBin*")) { $env:Path = "$CargoBin;$env:Path" }
}

function Test-Checkout($dir) {
    $dir -and (Test-Path (Join-Path $dir 'Cargo.toml')) -and (Test-Path (Join-Path $dir 'crates\koopagit-proc'))
}

function Test-Writable($dir) {
    try {
        New-Item -ItemType Directory -Force $dir | Out-Null
        $probe = Join-Path $dir '.koopagit-write-test'
        Set-Content $probe '' -ErrorAction Stop
        Remove-Item $probe
        return $true
    } catch { return $false }
}

function Restart-Explorer {
    Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
    # Windows normally restarts the shell itself.
    if (-not (Get-Process explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }
}

function Test-ExplorerRegistered {
    Test-Path "Registry::HKEY_CLASSES_ROOT\CLSID\$MenuClsid\InprocServer32"
}

function Get-InstalledDest {
    foreach ($d in $ProgramFilesDest, $UserDest) {
        if (Test-Path (Join-Path $d 'koopagit-proc.exe')) { return $d }
    }
    return $null
}

# Shorten the user's own folders for display.
function Format-Path([string]$p) {
    if ($p.StartsWith($env:LOCALAPPDATA, 'OrdinalIgnoreCase')) { return '%LOCALAPPDATA%' + $p.Substring($env:LOCALAPPDATA.Length) }
    if ($p.StartsWith($env:USERPROFILE, 'OrdinalIgnoreCase')) { return '~' + $p.Substring($env:USERPROFILE.Length) }
    return $p
}

$Arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
$VcComponent = if ($Arch -eq 'ARM64') { 'Microsoft.VisualStudio.Component.VC.Tools.ARM64' }
               else { 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64' }

function Get-VcTools {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) { return $null }
    $p = & $vswhere -latest -products * -requires $VcComponent -property installationPath
    if ($p) { return $p } else { return $null }
}

function Get-RustVersion {
    if (-not (Test-Command rustc) -or -not (Test-Command cargo)) { return $null }
    if ((rustc --version) -match 'rustc (\d+\.\d+\.\d+)') { return [version]$Matches[1] }
    return $null
}

function Get-RustHost {
    $line = rustc -vV | Where-Object { $_ -like 'host:*' }
    return ($line -replace '^host:\s*', '')
}

# ---------------------------------------------------------------- elevated steps
# These run inside the elevated process (or inline when already elevated).

# Replace files in place. A DLL that Explorer has loaded can't be deleted, but
# it can be renamed out of the way.
function Copy-Replacing($from, $to) {
    if (Test-Path $to) {
        try { Remove-Item $to -Force }
        catch { Rename-Item $to ("{0}.old-{1}" -f (Split-Path $to -Leaf), [DateTime]::Now.Ticks) }
    }
    Copy-Item $from $to
}

function Install-Files($srcRoot, $dest, [bool]$register) {
    $bin = Join-Path $srcRoot 'target\release'
    New-Item -ItemType Directory -Force $dest | Out-Null
    foreach ($f in $Binaries) { Copy-Replacing (Join-Path $bin $f) (Join-Path $dest $f) }
    Get-ChildItem $dest -Filter '*.old-*' | Remove-Item -Force -ErrorAction SilentlyContinue
    # `koopagit` command, like the macOS and Linux installs.
    Set-Content (Join-Path $dest 'koopagit.cmd') '@"%~dp0koopagit-proc.exe" %*' -Encoding ASCII
    Ok "Copied the program files to $dest"

    if ($register) {
        # register.reg assumes C:\Program Files\KoopaGit; point it at $dest.
        $reg = Get-Content (Join-Path $srcRoot 'crates\koopagit-explorer\register.reg') -Raw
        $reg = $reg.Replace('C:\\Program Files\\KoopaGit', $dest.Replace('\', '\\'))
        $tmp = Join-Path $env:TEMP 'koopagit-register.reg'
        Set-Content $tmp $reg -Encoding Unicode
        Invoke-Native reg.exe @('import', $tmp)
        Remove-Item $tmp
        Ok 'Registered the Explorer extension'
        Restart-Explorer
        Ok 'Restarted Explorer'
    }
}

function Uninstall-Files($srcRoot, $dest) {
    if (Test-ExplorerRegistered) {
        Invoke-Native reg.exe @('import', (Join-Path $srcRoot 'crates\koopagit-explorer\unregister.reg'))
        Ok 'Unregistered the Explorer extension'
        Restart-Explorer
    }
    if (Test-Path $dest) {
        try {
            Remove-Item -Recurse -Force $dest
            Ok "Removed $dest"
        } catch {
            Warn "Could not remove $dest (Explorer may still have the DLL loaded)."
            Info 'Restart Explorer or sign out, then delete the folder.'
        }
    }
}

# Run a step elevated: inline if we already are, otherwise in a new elevated
# PowerShell (one UAC prompt) whose output is copied back here.
function Invoke-Elevated([string]$stepName, [bool]$register) {
    if (Test-Admin) {
        if ($stepName -eq 'install') { Install-Files $script:Src $script:Dest $register }
        else { Uninstall-Files $script:Src $script:Dest }
        return
    }
    $logFile = Join-Path $env:TEMP "koopagit-$stepName-$PID.log"
    $exe = (Get-Process -Id $PID).Path
    $argLine = "-NoProfile -ExecutionPolicy Bypass -File `"$script:Src\install.ps1`" " +
        "-ElevatedStep $stepName -Source `"$script:Src`" -Dest `"$script:Dest`" -Log `"$logFile`""
    if ($register) { $argLine += ' -Explorer' }
    Note 'Windows asks for administrator rights (UAC) for this step.'
    try { $p = Start-Process $exe -ArgumentList $argLine -Verb RunAs -WindowStyle Hidden -PassThru }
    catch { Fail 'administrator rights were refused' }
    $null = $p.Handle
    $label = if ($stepName -eq 'install') { "Installing into $script:Dest" } else { "Removing $script:Dest" }
    if ($Fancy) { $code = Wait-Task $label $p '' 0 $false } else { $p.WaitForExit(); $code = $p.ExitCode }
    if (Test-Path $logFile) {
        Get-Content $logFile | Where-Object { $_ -match '^\s+\S ' } | ForEach-Object { Write-Host $_ }
        Remove-Item $logFile -ErrorAction SilentlyContinue
    }
    if ($code -ne 0) { Fail "the elevated $stepName step failed (exit code $code)" }
}

if ($ElevatedStep) {
    if ($Log) { Start-Transcript -Path $Log -Force | Out-Null }
    $code = 0
    try {
        if ($ElevatedStep -eq 'install') { Install-Files $Source $Dest ([bool]$Explorer) }
        elseif ($ElevatedStep -eq 'uninstall') { Uninstall-Files $Source $Dest }
    } catch {
        Bad $_.Exception.Message
        $code = 1
    }
    if ($Log) { Stop-Transcript | Out-Null }
    exit $code
}

# ---------------------------------------------------------------- checks

$script:Missing = @()
$script:Passed = 0
function Need($what) { $script:Missing += $what }
function Is-Missing($what) { $script:Missing -contains $what }
function Pass($m) { Ok $m; $script:Passed++ }

$DefaultSrc = if ($env:KOOPAGIT_SRC) { $env:KOOPAGIT_SRC } else { Join-Path $env:LOCALAPPDATA 'KoopaGit\src' }
if (Test-Checkout $PSScriptRoot) { $script:Src = $PSScriptRoot }
elseif (Test-Checkout (Get-Location).Path) { $script:Src = (Get-Location).Path }
else { $script:Src = $DefaultSrc }

$os = [Environment]::OSVersion.Version
$script:PlatformLabel = "Windows $($os.Major).$($os.Minor) build $($os.Build) ($Arch)"

function Invoke-Checks {
    $script:Missing = @()
    $script:Passed = 0
    Update-SessionPath

    if ($os.Major -ge 10) { Pass $script:PlatformLabel }
    else { Bad "Windows $os (KoopaGit needs Windows 10 or 11)"; Need 'windows' }

    if (Test-Command winget) { Pass 'winget' }
    else { Warn 'winget not found: missing tools must be installed by hand'; Need 'winget' }

    if (Test-Command git) { Pass (git --version) }
    else { Bad 'Git not installed'; Need 'git' }

    if (-not (Is-Missing 'git')) {
        $name = git config --global user.name
        $email = git config --global user.email
        if ($name -and $email) { Pass "git identity: $name <$email>" }
        else { Warn 'git user.name / user.email not set (needed to commit)'; Need 'identity' }
    }

    $vc = Get-VcTools
    if ($vc) { Pass 'Visual Studio C++ build tools' }
    else { Bad 'Visual Studio C++ build tools not installed (needed by Rust)'; Need 'vctools' }

    $rust = Get-RustVersion
    if (-not $rust) { Bad "Rust not installed (needs $MinRust or newer)"; Need 'rust' }
    elseif ($rust -lt $MinRust) { Bad "Rust $rust is too old (needs $MinRust or newer)"; Need 'rust_old' }
    else {
        $hostTriple = Get-RustHost
        if ($hostTriple -like '*-msvc') { Pass "Rust $rust ($hostTriple)" }
        else { Bad "Rust $rust uses the $hostTriple toolchain; KoopaGit needs MSVC"; Need 'rust_msvc' }
    }

    # The release build (Slint's generated code especially) needs a lot of
    # memory per rustc process; cap the parallel jobs at one per 3 GiB.
    $memGiB = [int][math]::Floor((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB)
    $cpus = [Environment]::ProcessorCount
    if ($env:CARGO_BUILD_JOBS) { Pass "$memGiB GiB RAM, $cpus CPUs (CARGO_BUILD_JOBS=$env:CARGO_BUILD_JOBS)" }
    else {
        $jobs = [math]::Max(1, [math]::Floor($memGiB / 3))
        if ($jobs -lt $cpus) {
            $env:CARGO_BUILD_JOBS = "$jobs"
            Pass "$memGiB GiB RAM, $cpus CPUs: building with $jobs parallel jobs to avoid running out of memory"
        } else { Pass "$memGiB GiB RAM, $cpus CPUs" }
        if ($memGiB -lt 4) { Warn 'Less than 4 GiB RAM: the build may still run out of memory' }
    }

    if (Test-Checkout $script:Src) {
        $rev = if (Test-Path "$script:Src\.git") { git -C $script:Src log -1 --format='%h %s' } else { 'not a git checkout' }
        Pass "Source: $(Format-Path $script:Src) ($rev)"
    } else { Warn "Source: not downloaded yet ($(Format-Path $script:Src))"; Need 'source' }

    $installed = Get-InstalledDest
    if ($installed) {
        $upd = if (-not $Check) { ' (will be updated)' } else { '' }
        Ok "Installed: $(& (Join-Path $installed 'koopagit-proc.exe') --version) in $installed$upd"
    }
    if (Test-ExplorerRegistered) { Ok 'Explorer extension registered' }
}

# ---------------------------------------------------------------- setup

# The recommended setup; Customize changes these.
function Set-Recommended {
    $installed = Get-InstalledDest
    $script:Dest = if ($DestGiven) { $Dest } elseif ($installed) { $installed } else { $ProgramFilesDest }
    $script:Register = [bool]$Explorer -or (Test-ExplorerRegistered)
    $script:AddPath = -not (Test-OnUserPath $script:Dest)
}

# What the prepare step will install, in words.
function Get-PrereqPlan {
    $p = @()
    if (Is-Missing 'git') { $p += 'Git for Windows (winget)' }
    if (Is-Missing 'vctools') { $p += 'Visual Studio C++ Build Tools (winget, a few GB)' }
    if (Is-Missing 'rust') { $p += 'Rust (rustup)' }
    if ((Is-Missing 'rust_old') -or (Is-Missing 'rust_msvc')) { $p += 'current Rust MSVC toolchain (rustup)' }
    if (Is-Missing 'source') { $p += "KoopaGit's source" }
    return ($p -join ', ')
}

function Write-PlanRow([string]$name, [string]$value) {
    Write-Host ('    ' + $name.PadRight(14)) -NoNewline -ForegroundColor DarkGray
    Write-Host " $value"
}

function Show-Plan {
    $pre = Get-PrereqPlan
    Write-PlanRow 'Prerequisites' $(if ($pre) { $pre } else { 'all present' })
    Write-PlanRow 'Install to' $script:Dest
    Write-PlanRow 'Command' "koopagit (koopagit.cmd in the install folder)"
    Write-PlanRow 'PATH' $(if ($script:AddPath) { 'add the install folder to your PATH' } elseif (Test-OnUserPath $script:Dest) { 'already set' } else { 'leave alone' })
    Write-PlanRow 'Explorer' $(if ($script:Register) { 'register the extension (experimental; Explorer restarts)' } else { 'not registered (experimental)' })
    if (-not (Test-Admin) -and ($script:Register -or $script:Dest -like "$env:ProgramFiles*")) {
        Write-PlanRow 'Admin' 'one UAC prompt, for installing and registering'
    }
    Write-PlanRow 'Source' $(if (Test-Checkout $script:Src) { Format-Path $script:Src } else { "download to $(Format-Path $script:Src)" })
}

function Invoke-Customize {
    Show-Screen 2 'Customize'

    Title 'Install location'
    Note 'The folder KoopaGit''s programs are installed into.'
    $items = @("$ProgramFilesDest|for everyone on this PC (asks for admin rights)",
        "$UserDest|only for you, no admin rights needed",
        'Other folder...|type a path')
    $sel = if ($script:Dest -eq $ProgramFilesDest) { 1 } elseif ($script:Dest -eq $UserDest) { 2 } else { 3 }
    switch (Select-Menu $sel $items) {
        1 { $script:Dest = $ProgramFilesDest }
        2 { $script:Dest = $UserDest }
        3 { $script:Dest = Ask 'Folder' $script:Dest }
    }

    Title 'Explorer context menu'
    Note 'Experimental: the extension has never been loaded into Explorer. If it'
    Note 'misbehaves, run this installer with -Uninstall (Explorer restarts).'
    $sel = if ($script:Register) { 2 } else { 1 }
    $script:Register = (Select-Menu $sel @("Don't register it|recommended for now",
            'Register it|adds the KoopaGit menu; Explorer restarts')) -eq 2

    $script:AddPath = $false
    if (-not (Test-OnUserPath $script:Dest)) {
        Title 'PATH'
        Note 'Adding the install folder to your PATH makes the koopagit command work in any terminal.'
        $script:AddPath = (Select-Menu 1 @('Add it|recommended', 'Leave PATH alone|')) -eq 1
    }
}

function Request-Identity {
    if ((git config --global user.name) -and (git config --global user.email)) { return }
    if ($Yes) { return }
    Title 'Your git identity'
    Note "Like TortoiseGit's first-start wizard: commits need a name and an email."
    Note 'Leave empty to skip.'
    $n = Ask 'Name' "$(git config --global user.name)"
    if ($n) { git config --global user.name $n }
    $e = Ask 'Email' "$(git config --global user.email)"
    if ($e) { git config --global user.email $e }
}

# ---------------------------------------------------------------- prepare

function Install-Rustup {
    $url = if ($Arch -eq 'ARM64') { 'https://win.rustup.rs/aarch64' } else { 'https://win.rustup.rs/x86_64' }
    $dir = Join-Path $env:TEMP "koopagit-rustup-$PID"
    New-Item -ItemType Directory -Force $dir | Out-Null
    $exe = Join-Path $dir 'rustup-init.exe'
    Note "Downloading $url"
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest $url -OutFile $exe -UseBasicParsing
    Invoke-Task 'Installing Rust (rustup)' $exe @('-y', '--profile', 'minimal', '--default-toolchain', 'stable')
    Remove-Item -Recurse -Force $dir
    Update-SessionPath
}

function Install-WithWinget([string]$label, [string]$id, [string[]]$extra = @()) {
    if (Is-Missing 'winget') { Fail "winget is not available; install $id by hand and run this installer again" }
    Invoke-Task $label winget (@('install', '--id', $id, '-e', '--source', 'winget',
            '--accept-package-agreements', '--accept-source-agreements') + $extra)
    Update-SessionPath
}

function Invoke-Prepare {
    if (Is-Missing 'windows') { Fail 'KoopaGit needs Windows 10 or 11' }
    if (-not (Get-PrereqPlan)) { Ok 'Everything needed is already installed' }

    if (Is-Missing 'git') {
        if (-not (Confirm-Choice 'Install Git for Windows with winget?')) { Fail 'Git is required (https://gitforwindows.org/)' }
        Install-WithWinget 'Installing Git for Windows' 'Git.Git'
        if (-not (Test-Command git)) { Fail 'git is still not on PATH; open a new PowerShell and run this installer again' }
    }

    if (Is-Missing 'vctools') {
        Note 'Rust needs the MSVC linker and the Windows SDK. The download is a few GB.'
        if (-not (Confirm-Choice 'Install the Visual Studio 2022 Build Tools with winget?')) {
            Fail 'the Visual Studio C++ Build Tools are required'
        }
        Install-WithWinget 'Installing the Visual Studio C++ Build Tools' 'Microsoft.VisualStudio.2022.BuildTools' @('--override',
            '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
        if (-not (Get-VcTools)) { Fail 'the C++ build tools are still missing; add "Desktop development with C++" in the Visual Studio Installer' }
    }

    if ((Is-Missing 'rust') -or (((Is-Missing 'rust_old') -or (Is-Missing 'rust_msvc')) -and -not (Test-Command rustup))) {
        Note 'Rust is installed per user with rustup (https://rustup.rs) into ~\.cargo.'
        if (-not (Confirm-Choice 'Install Rust with rustup?')) { Fail "Rust $MinRust or newer is required" }
        Install-Rustup
    } elseif ((Is-Missing 'rust_old') -or (Is-Missing 'rust_msvc')) {
        if (-not (Confirm-Choice 'Switch to the latest stable MSVC toolchain (rustup default stable-msvc)?')) {
            Fail "Rust $MinRust or newer (MSVC) is required"
        }
        Invoke-Task 'Updating Rust' rustup @('update', 'stable-msvc')
        Invoke-Native rustup @('default', 'stable-msvc')
    }
    $rust = Get-RustVersion
    if (-not $rust -or $rust -lt $MinRust) { Fail "Rust $MinRust or newer is still not available; open a new PowerShell and retry" }

    if (Is-Missing 'source') {
        if (-not (Confirm-Choice "Download KoopaGit's source into $(Format-Path $script:Src)?")) { Fail 'no source checkout' }
        New-Item -ItemType Directory -Force (Split-Path $script:Src) | Out-Null
        Invoke-Task 'Downloading the source' git @('clone', $RepoUrl, $script:Src)
    } elseif ($script:Src -eq $DefaultSrc -and (Test-Path "$script:Src\.git")) {
        # Only update the checkout this script cloned, never a developer's own.
        Invoke-Task 'Updating the source (git pull)' git @('-C', $script:Src, 'pull', '--ff-only')
    }
}

# ---------------------------------------------------------------- user PATH

function Get-UserPath { [Environment]::GetEnvironmentVariable('Path', 'User') }
function Test-OnUserPath($dir) { [bool]((Get-UserPath) -split ';' | Where-Object { $_.TrimEnd('\') -ieq $dir.TrimEnd('\') }) }

function Add-UserPath($dir) {
    if (Test-OnUserPath $dir) { return }
    $p = Get-UserPath
    $new = if ($p) { "$($p.TrimEnd(';'));$dir" } else { $dir }
    [Environment]::SetEnvironmentVariable('Path', $new, 'User')
    $env:Path = "$env:Path;$dir"
    Ok "Added $dir to your PATH (new terminals pick it up)"
}

function Remove-UserPath($dir) {
    if (-not (Test-OnUserPath $dir)) { return }
    $new = ((Get-UserPath) -split ';' | Where-Object { $_ -and $_.TrimEnd('\') -ine $dir.TrimEnd('\') }) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $new, 'User')
    Ok "Removed $dir from your PATH"
}

# ---------------------------------------------------------------- main

function Invoke-Install {
    New-Item -ItemType Directory -Force $LogDir | Out-Null
    Set-Content $LogFile ''

    # 1. Check
    Show-Screen 1 'Checking your system'
    Invoke-Checks
    Set-Recommended

    # 2. Setup
    Show-Screen 2 'Choose a setup'
    $pre = Get-PrereqPlan
    if ($pre) { Warn "$script:Passed checks passed; still needed: $pre" } else { Ok "$script:Passed checks passed" }
    Title 'Recommended setup'
    Show-Plan
    Write-Host ''
    switch (Select-Menu 1 @('Install|use the recommended setup above',
            'Customize...|choose the install location and each option', 'Cancel|')) {
        1 { $script:Auto = $true }
        2 {
            Invoke-Customize
            Show-Screen 2 'Review'
            Show-Plan
            Write-Host ''
            if ((Select-Menu 1 @('Install|with these settings', 'Cancel|')) -ne 1) { return }
        }
        default { return }
    }
    Request-Identity

    # 3. Prepare
    Show-Screen 3 'Preparing'
    Invoke-Prepare

    # 4. Build
    Show-Screen 4 'Building KoopaGit'
    Note 'The first build takes a few minutes; later updates are much faster.'
    Write-Host ''
    $pkgs = @()
    foreach ($c in $Crates) { $pkgs += '-p', $c }
    $totals = @(0, 0)
    try {
        $tree = @(cargo tree --locked -e normal,build --prefix none --manifest-path "$script:Src\Cargo.toml" @pkgs 2>$null |
                ForEach-Object { $_ -replace ' \(\*\)$', '' -replace ' \(proc-macro\)$', '' } | Sort-Object -Unique)
        $ws = @($tree | Where-Object { $_.Contains("($script:Src\") }).Count
        $totals = @(($tree.Count - $ws), $ws)
    } catch { }
    Invoke-Task 'Compiling' cargo (@('build', '--release', '--locked', '--manifest-path', "$script:Src\Cargo.toml") + $pkgs) -Totals $totals -Build

    # 5. Install
    Show-Screen 5 'Installing'
    $needAdmin = $script:Register -or -not (Test-Writable $script:Dest)
    if ($needAdmin) { Invoke-Elevated 'install' $script:Register } else { Install-Files $script:Src $script:Dest $script:Register }
    if ($script:AddPath) { Add-UserPath $script:Dest }

    # 6. Finish
    Show-Screen 6 'Checking the installation'
    $failed = $false
    $proc = Join-Path $script:Dest 'koopagit-proc.exe'
    $v = & $proc --version
    if ($LASTEXITCODE -eq 0) { Ok "$v runs" } else { Bad "$proc does not run"; $failed = $true }
    if (Test-Path (Join-Path $script:Dest 'koopagit.cmd')) { Ok "koopagit command: $script:Dest\koopagit.cmd" }
    if ($script:Register) {
        if (Test-ExplorerRegistered) { Ok 'Explorer extension registered' }
        else { Bad 'Explorer extension is not registered'; $failed = $true }
    }
    if ($failed) { Fail 'the installation finished with problems (see above); run with -Check for details' }

    Write-Host ''
    Write-Host '  KoopaGit is installed.' -ForegroundColor Green
    Write-Host ''
    if ($script:Register) { Info 'Right-click inside a repository in Explorer (Windows 11: Show more options).' }
    if ($script:AddPath) { Info 'Open a new terminal to pick up the PATH change.' }
    Info 'From a terminal:  koopagit log   |   koopagit commit   |   koopagit help'
    Info 'Update by running this installer again; remove with -Uninstall.'
    Note "Log: $LogFile"
    Write-Host ''
}

function Invoke-Uninstall {
    $script:Steps = @('Confirm', 'Remove', 'Finish')
    New-Item -ItemType Directory -Force $LogDir | Out-Null
    Show-Screen 1 'Uninstall KoopaGit'
    $installed = Get-InstalledDest
    $script:Dest = if ($DestGiven) { $Dest } elseif ($installed) { $installed } else { $ProgramFilesDest }
    if (-not (Test-Path $script:Dest) -and -not (Test-ExplorerRegistered)) {
        Info "KoopaGit is not installed in $script:Dest."
        return
    }
    Info "$script:Dest"
    if (Test-ExplorerRegistered) { Info 'The Explorer extension' }
    Write-Host ''
    if ((Select-Menu 1 @('Remove KoopaGit|', 'Cancel|')) -ne 1) { return }
    if (-not (Test-Checkout $script:Src)) { Fail 'run the uninstaller from a KoopaGit checkout (it needs unregister.reg)' }
    $settings = Join-Path $env:APPDATA 'koopagit'
    $dropSettings = $false
    if (Test-Path $settings) {
        Title "Your settings  ($settings)"
        $dropSettings = (Select-Menu 1 @('Keep them|in case you reinstall', 'Delete them|')) -eq 2
    }

    Show-Screen 2 'Removing KoopaGit'
    Invoke-Elevated 'uninstall' $false
    Remove-UserPath $script:Dest
    if ($dropSettings) { Remove-Item -Recurse -Force $settings; Ok "Removed $settings" }

    Show-Screen 3 'Done'
    Write-Host '  KoopaGit is removed.' -ForegroundColor Green
    Write-Host ''
}

$exitCode = 0
try {
    if ($Check) {
        Write-Host "KoopaGit setup ($script:PlatformLabel): checking" -ForegroundColor White
        Write-Host ''
        Invoke-Checks
        # The git identity and the checkout are optional or fetched later.
        $blocking = $script:Missing | Where-Object { $_ -notin 'identity', 'source', 'winget' }
        Write-Host ''
        if ($blocking) {
            Write-Host "Missing: $($blocking -join ' '). Run .\install.ps1 to install them."
            $exitCode = 1
        } else { Write-Host 'Everything needed to build and install KoopaGit is present.' }
    } elseif ($Uninstall) { Invoke-Uninstall }
    else { Invoke-Install }
} catch {
    Write-Host ''
    Write-Host "  error: $($_.Exception.Message)" -ForegroundColor Red
    if (Test-Path $LogFile) { Write-Host "  Full log: $LogFile" }
    $exitCode = 1
}
# Exit only when run as a file; under `irm | iex` exit would close the window.
if ($PSCommandPath) { exit $exitCode }
