<#
.SYNOPSIS
KoopaGit installer for Windows (macOS and Linux: install.sh).

.DESCRIPTION
An interactive wizard: checks every prerequisite, offers to install the missing
ones (Git, the Visual Studio C++ Build Tools, Rust), builds KoopaGit from
source, installs it, optionally registers the experimental Explorer extension,
and verifies the result. Run it again to update.

Building runs as you; only copying into Program Files and registering the
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
    # Accept every default without asking.
    [switch]$Yes,
    # Register the (experimental) Explorer extension without asking.
    [switch]$Explorer,
    # Install folder.
    [string]$Dest = "$env:ProgramFiles\KoopaGit",
    # Internal: the step run in the elevated process (install | uninstall).
    [string]$ElevatedStep = '',
    [string]$Source = '',
    [string]$Log = ''
)

$ErrorActionPreference = 'Stop'
$MinRust = [version]'1.92'
$RepoUrl = 'https://github.com/jonassaa/koopagit.git'
$MenuClsid = '{5e1a8f00-6b4c-4d1f-9a2e-7c0d3b1e4a00}'
$Binaries = 'koopagit-proc.exe', 'koopagit-merge.exe', 'koopagit_explorer.dll'

# ---------------------------------------------------------------- output

function Step($m) { Write-Host ''; Write-Host "==> $m" -ForegroundColor Cyan }
function Ok($m) { Write-Host "  + $m" -ForegroundColor Green }
function Warn($m) { Write-Host "  ! $m" -ForegroundColor Yellow }
function Bad($m) { Write-Host "  x $m" -ForegroundColor Red }
function Info($m) { Write-Host "    $m" }
# Throw rather than exit: under `irm | iex`, exit would close the user's window.
function Fail($m) { throw $m }

function Confirm-Choice([string]$Question, [bool]$Default = $true) {
    if ($Yes) { return $Default }
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

# Run a native command, show it, and fail on a non-zero exit code.
function Invoke-Native([string]$Exe, [string[]]$Arguments) {
    Write-Host "    > $Exe $($Arguments -join ' ')" -ForegroundColor DarkGray
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { Fail "$Exe exited with code $LASTEXITCODE" }
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
        $probe = Join-Path $dir ".koopagit-write-test"
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
    Ok "Copied the binaries to $dest"

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
        if ($stepName -eq 'install') { Install-Files $script:Src $Dest $register }
        else { Uninstall-Files $script:Src $Dest }
        return
    }
    $logFile = Join-Path $env:TEMP "koopagit-$stepName-$PID.log"
    $exe = (Get-Process -Id $PID).Path
    $argLine = "-NoProfile -ExecutionPolicy Bypass -File `"$script:Src\install.ps1`" " +
        "-ElevatedStep $stepName -Source `"$script:Src`" -Dest `"$Dest`" -Log `"$logFile`""
    if ($register) { $argLine += ' -Explorer' }
    Info 'Asking for administrator rights (UAC)...'
    try { $p = Start-Process $exe -ArgumentList $argLine -Verb RunAs -Wait -PassThru }
    catch { Fail 'administrator rights were refused' }
    if (Test-Path $logFile) {
        Get-Content $logFile | Where-Object { $_ -match '^\s+[+!x>] |^\s{4}' } | ForEach-Object { Write-Host $_ }
        Remove-Item $logFile -ErrorAction SilentlyContinue
    }
    if ($p.ExitCode -ne 0) { Fail "the elevated $stepName step failed (exit code $($p.ExitCode))" }
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
function Need($what) { $script:Missing += $what }
function Is-Missing($what) { $script:Missing -contains $what }

if (Test-Checkout $PSScriptRoot) { $script:Src = $PSScriptRoot }
elseif (Test-Checkout (Get-Location).Path) { $script:Src = (Get-Location).Path }
elseif ($env:KOOPAGIT_SRC) { $script:Src = $env:KOOPAGIT_SRC }
else { $script:Src = Join-Path $env:LOCALAPPDATA 'KoopaGit\src' }

function Invoke-Checks {
    $script:Missing = @()
    Update-SessionPath
    Step 'Checking prerequisites'

    $os = [Environment]::OSVersion.Version
    if ($os.Major -ge 10) { Ok "Windows $($os.Major).$($os.Minor) build $($os.Build) ($Arch)" }
    else { Bad "Windows $os (KoopaGit needs Windows 10 or 11)"; Need 'windows' }

    if (Test-Command winget) { Ok 'winget' }
    else { Warn 'winget not found: missing tools must be installed by hand'; Need 'winget' }

    if (Test-Command git) { Ok (git --version) }
    else { Bad 'Git not installed'; Need 'git' }

    if (-not (Is-Missing 'git')) {
        $name = git config --global user.name
        $email = git config --global user.email
        if ($name -and $email) { Ok "git identity: $name <$email>" }
        else { Warn 'git user.name / user.email not set (needed to commit)'; Need 'identity' }
    }

    $vc = Get-VcTools
    if ($vc) { Ok "Visual Studio C++ build tools ($vc)" }
    else { Bad 'Visual Studio C++ build tools not installed (needed by Rust)'; Need 'vctools' }

    $rust = Get-RustVersion
    if (-not $rust) { Bad "Rust not installed (needs $MinRust or newer)"; Need 'rust' }
    elseif ($rust -lt $MinRust) { Bad "Rust $rust is too old (needs $MinRust or newer)"; Need 'rust_old' }
    else {
        $hostTriple = Get-RustHost
        if ($hostTriple -like '*-msvc') { Ok "Rust $rust ($hostTriple)" }
        else { Bad "Rust $rust uses the $hostTriple toolchain; KoopaGit needs MSVC"; Need 'rust_msvc' }
    }

    # The release build (Slint's generated code especially) needs a lot of
    # memory per rustc process; cap the parallel jobs at one per 3 GiB.
    $memGiB = [int][math]::Floor((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB)
    $cpus = [Environment]::ProcessorCount
    if ($env:CARGO_BUILD_JOBS) { Ok "$memGiB GiB RAM, $cpus CPUs (CARGO_BUILD_JOBS=$env:CARGO_BUILD_JOBS)" }
    else {
        $jobs = [math]::Max(1, [math]::Floor($memGiB / 3))
        if ($jobs -lt $cpus) {
            $env:CARGO_BUILD_JOBS = "$jobs"
            Ok "$memGiB GiB RAM, $cpus CPUs: building with $jobs parallel jobs to avoid running out of memory"
        } else { Ok "$memGiB GiB RAM, $cpus CPUs" }
        if ($memGiB -lt 4) { Warn 'Less than 4 GiB RAM: the build may still run out of memory' }
    }

    if (Test-Checkout $script:Src) {
        $rev = if (Test-Path "$script:Src\.git") { git -C $script:Src log -1 --format='%h %s' } else { 'not a git checkout' }
        Ok "Source: $script:Src ($rev)"
    } else { Warn "Source: not found, will clone $RepoUrl into $script:Src"; Need 'source' }

    $installed = Join-Path $Dest 'koopagit-proc.exe'
    if (Test-Path $installed) { Ok "Installed: $(& $installed --version) in $Dest" }
    if (Test-ExplorerRegistered) { Ok 'Explorer extension registered' }
}

# ---------------------------------------------------------------- fixes

function Install-WithWinget([string]$id, [string[]]$extra = @()) {
    if (Is-Missing 'winget') { Fail "winget is not available; install $id by hand and run this script again" }
    Invoke-Native winget (@('install', '--id', $id, '-e', '--source', 'winget',
            '--accept-package-agreements', '--accept-source-agreements') + $extra)
    Update-SessionPath
}

function Install-Rustup {
    $url = if ($Arch -eq 'ARM64') { 'https://win.rustup.rs/aarch64' } else { 'https://win.rustup.rs/x86_64' }
    $dir = Join-Path $env:TEMP "koopagit-rustup-$PID"
    New-Item -ItemType Directory -Force $dir | Out-Null
    $exe = Join-Path $dir 'rustup-init.exe'
    Info "Downloading $url"
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest $url -OutFile $exe -UseBasicParsing
    Invoke-Native $exe @('-y', '--profile', 'minimal', '--default-toolchain', 'stable')
    Remove-Item -Recurse -Force $dir
    Update-SessionPath
}

function Repair-Prerequisites {
    if (Is-Missing 'windows') { Fail 'KoopaGit needs Windows 10 or 11' }

    if (Is-Missing 'git') {
        Step 'Installing Git for Windows'
        if (Confirm-Choice 'Install Git for Windows with winget?') { Install-WithWinget 'Git.Git' }
        else { Fail 'Git is required (https://gitforwindows.org/)' }
        if (-not (Test-Command git)) { Fail 'git is still not on PATH; open a new PowerShell and run this script again' }
    }

    if (Is-Missing 'vctools') {
        Step 'Installing the Visual Studio C++ Build Tools'
        Info 'Rust needs the MSVC linker and the Windows SDK (workload "Desktop development'
        Info 'with C++"). The download is a few GB and can take a while.'
        if (Confirm-Choice 'Install the Visual Studio 2022 Build Tools with winget?') {
            Install-WithWinget 'Microsoft.VisualStudio.2022.BuildTools' @('--override',
                '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
            if (-not (Get-VcTools)) { Fail 'the C++ build tools are still missing; add "Desktop development with C++" in the Visual Studio Installer' }
        } else { Fail 'the Visual Studio C++ Build Tools are required' }
    }

    if (Is-Missing 'rust') {
        Step 'Installing Rust'
        Info 'Rust is installed per user with rustup (https://rustup.rs) into ~\.cargo.'
        if (Confirm-Choice 'Install Rust with rustup?') { Install-Rustup }
        else { Fail "Rust $MinRust or newer is required" }
    } elseif ((Is-Missing 'rust_old') -or (Is-Missing 'rust_msvc')) {
        Step 'Updating Rust'
        if (-not (Test-Command rustup)) {
            if (Confirm-Choice 'This Rust is not managed by rustup. Install rustup?') { Install-Rustup }
            else { Fail "Rust $MinRust or newer (MSVC) is required" }
        } elseif (Confirm-Choice 'Switch to the latest stable MSVC toolchain (rustup default stable-msvc)?') {
            Invoke-Native rustup @('update', 'stable-msvc')
            Invoke-Native rustup @('default', 'stable-msvc')
        } else { Fail "Rust $MinRust or newer (MSVC) is required" }
    }
    $rust = Get-RustVersion
    if (-not $rust -or $rust -lt $MinRust) { Fail "Rust $MinRust or newer is still not available; open a new PowerShell and retry" }

    # Checked again here: git may have only just been installed.
    if (-not (git config --global user.name) -or -not (git config --global user.email)) {
        Step 'Git identity'
        Info "Like TortoiseGit's first-start wizard: commits need a name and an email."
        if (-not $Yes -and (Confirm-Choice 'Set your global git name and email now?')) {
            $current = git config --global user.name
            $n = Ask 'Name' $(if ($current) { $current } else { $env:USERNAME })
            git config --global user.name $n
            $e = Ask 'Email' "$(git config --global user.email)"
            if ($e) { git config --global user.email $e }
            Ok 'Set git identity'
        }
    }

    $defaultSrc = if ($env:KOOPAGIT_SRC) { $env:KOOPAGIT_SRC } else { Join-Path $env:LOCALAPPDATA 'KoopaGit\src' }
    if (Is-Missing 'source') {
        Step 'Getting the source'
        if (Confirm-Choice "Clone $RepoUrl into $($script:Src)?") {
            New-Item -ItemType Directory -Force (Split-Path $script:Src) | Out-Null
            Invoke-Native git @('clone', $RepoUrl, $script:Src)
        } else { Fail 'no source checkout' }
    } elseif ($script:Src -eq $defaultSrc -and (Test-Path "$script:Src\.git")) {
        # Only update the checkout this script cloned, never a developer's own.
        if (Confirm-Choice "Update the source in $($script:Src) (git pull)?") {
            Invoke-Native git @('-C', $script:Src, 'pull', '--ff-only')
        }
    }
}

# ---------------------------------------------------------------- user PATH

function Get-UserPath { [Environment]::GetEnvironmentVariable('Path', 'User') }
function Test-OnUserPath($dir) { (Get-UserPath) -split ';' | Where-Object { $_.TrimEnd('\') -ieq $dir.TrimEnd('\') } }

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
    Invoke-Checks
    if ($script:Missing.Count -gt 0) { Repair-Prerequisites }

    Step 'Options'
    $script:Dest = Ask 'Install KoopaGit into' $Dest
    $register = [bool]$Explorer
    if (-not $register) {
        Warn 'The Explorer context menu is experimental: it has never been loaded into Explorer.'
        Info 'If it misbehaves, run this script with -Uninstall (Explorer restarts).'
        $register = Confirm-Choice 'Register the Explorer extension?' $false
    }
    $addPath = $false
    if (-not (Test-OnUserPath $Dest)) { $addPath = Confirm-Choice "Add $Dest to your PATH (for the koopagit command)?" }
    $needAdmin = $register -or -not (Test-Writable $Dest)

    Step 'Ready to install'
    Info "Folder:   $Dest"
    Info "Explorer: $(if ($register) { 'register the extension (Explorer restarts)' } else { 'not registered' })"
    if ($needAdmin) { Info 'Admin:    one UAC prompt for copying and registering' }
    if (-not (Confirm-Choice 'Continue?')) { return }

    Step 'Building (the first build takes a few minutes)'
    Invoke-Native cargo @('build', '--release', '--locked', '--manifest-path', "$script:Src\Cargo.toml",
        '-p', 'koopagit-proc', '-p', 'koopagit-merge', '-p', 'koopagit-explorer')

    Step "Installing into $Dest"
    if ($needAdmin) { Invoke-Elevated 'install' $register } else { Install-Files $script:Src $Dest $register }
    if ($addPath) { Add-UserPath $Dest }

    Step 'Verifying'
    $failed = $false
    $proc = Join-Path $Dest 'koopagit-proc.exe'
    $v = & $proc --version
    if ($LASTEXITCODE -eq 0) { Ok "$v ($proc)" } else { Bad "$proc does not run"; $failed = $true }
    if (Test-Path (Join-Path $Dest 'koopagit.cmd')) { Ok "koopagit command: $Dest\koopagit.cmd" }
    if ($register) {
        if (Test-ExplorerRegistered) { Ok 'Explorer extension registered' }
        else { Bad 'Explorer extension is not registered'; $failed = $true }
    }
    if ($failed) { Fail 'installation finished with problems (see above); run with -Check for details' }

    Step 'Done'
    if ($register) { Info 'Right-click inside a repository in Explorer (Windows 11: Show more options).' }
    Info 'From a terminal:  koopagit log   |   koopagit commit   |   koopagit help'
    Info 'Update later by running this script again; remove with -Uninstall.'
}

function Invoke-Uninstall {
    Step 'Uninstalling KoopaGit'
    if (-not (Test-Path $Dest) -and -not (Test-ExplorerRegistered)) { Info "Nothing installed in $Dest" }
    elseif (Confirm-Choice "Remove KoopaGit from $Dest?") {
        if (-not (Test-Checkout $script:Src)) { Fail 'run the uninstaller from a KoopaGit checkout (it needs unregister.reg)' }
        Invoke-Elevated 'uninstall' $false
    }
    Remove-UserPath $Dest
    $settings = Join-Path $env:APPDATA 'koopagit'
    if ((Test-Path $settings) -and (Confirm-Choice "Also delete your settings ($settings)?" $false)) {
        Remove-Item -Recurse -Force $settings
        Ok "Removed $settings"
    }
    Step 'KoopaGit removed'
}

Write-Host 'KoopaGit installer (windows)' -ForegroundColor White
$exitCode = 0
try {
    if ($Check) {
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
    Write-Host "error: $($_.Exception.Message)" -ForegroundColor Red
    $exitCode = 1
}
# Exit only when run as a file; under `irm | iex` exit would close the window.
if ($PSCommandPath) { exit $exitCode }
