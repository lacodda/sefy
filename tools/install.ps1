# sefy installer for Windows:
#   irm https://raw.githubusercontent.com/lacodda/sefy/main/tools/install.ps1 | iex
$ErrorActionPreference = "Stop"

$repo = "lacodda/sefy"

# The tag comes from the /releases/latest redirect rather than the REST API:
# unauthenticated API calls are capped at 60 per hour per IP, and an installer
# that fails because someone else on the same address ran it is no installer.
# $env:SEFY_VERSION pins a specific release.
$tag = $env:SEFY_VERSION
if (-not $tag) {
    $request = [Net.HttpWebRequest]::Create("https://github.com/$repo/releases/latest")
    $request.AllowAutoRedirect = $false
    $request.UserAgent = "sefy-installer"
    try {
        $response = $request.GetResponse()
        $tag = ($response.Headers["Location"] -split "/")[-1]
        $response.Close()
    } catch {
        throw "Cannot resolve the latest release of ${repo}: $($_.Exception.Message)"
    }
}
if (-not $tag -or $tag -notmatch '^v\d') {
    throw "Cannot resolve the latest release of $repo - set `$env:SEFY_VERSION to a tag like v0.18.0"
}

$name = "sefy-$tag-x86_64-pc-windows-msvc"
$url = "https://github.com/$repo/releases/download/$tag/$name.zip"
$dir = if ($env:SEFY_INSTALL_DIR) { $env:SEFY_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\sefy" }
$tmp = Join-Path ([IO.Path]::GetTempPath()) "sefy-install-$([guid]::NewGuid())"
New-Item -ItemType Directory -Force $tmp | Out-Null

try {
    Write-Host "Downloading $url"
    Invoke-WebRequest $url -OutFile (Join-Path $tmp "sefy.zip")
    Expand-Archive (Join-Path $tmp "sefy.zip") -DestinationPath $tmp -Force
    $binary = Get-ChildItem -Path $tmp -Filter "sefy.exe" -Recurse | Select-Object -First 1
    if (-not $binary) { throw "The archive did not contain sefy.exe" }
    New-Item -ItemType Directory -Force $dir | Out-Null
    Copy-Item $binary.FullName $dir -Force

    # Transports go where sefy looks for them, which is its data directory -
    # not beside the binary, and not beside the vault, where a plugins folder
    # would annotate a file that gives nothing away.
    $plugins = Get-ChildItem -Path $tmp -Filter "sefy-plugin-*.exe" -Recurse
    if ($plugins) {
        $pluginDir = Join-Path $env:APPDATA "sefy\plugins"
        New-Item -ItemType Directory -Force $pluginDir | Out-Null
        foreach ($plugin in $plugins) {
            Copy-Item $plugin.FullName $pluginDir -Force
            Write-Host "Installed $($plugin.Name) to $pluginDir"
        }
    }
} finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

# Add the directory to the user PATH in the registry, keeping the value's
# type. PATH is almost always REG_EXPAND_SZ, with entries like %JAVA_HOME%\bin
# stored unexpanded; the .NET environment API reads them expanded and writes
# the result back as a plain REG_SZ, so every such entry is frozen at whatever
# the variable happened to be during the install and stops following it after.
# The damage is done to somebody else's PATH by an installer for this program,
# and nothing reports it - found on rigger's own installer at v0.1.0.
#
# So: read the raw value unexpanded, compare case-insensitively and without a
# trailing slash, write it back as an expandable string, and tell running
# shells about it. A PATH failure must not fail the install - the binary is
# already in place and can be run by its full path.
try {
    $key = Get-Item "HKCU:\Environment"
    $raw = [string]$key.GetValue("Path", "", "DoNotExpandEnvironmentNames")
    $entries = @($raw -split ";" | Where-Object { $_ })
    $wanted = $dir.TrimEnd("\")
    $present = $entries | Where-Object { $_.TrimEnd("\") -ieq $wanted }
    if (-not $present) {
        $value = if ($entries.Count -gt 0) { ($entries + $wanted) -join ";" } else { $wanted }
        Set-ItemProperty -Path "HKCU:\Environment" -Name Path -Value $value -Type ExpandString
        # Without the broadcast the new PATH reaches only processes started
        # after the next sign-in; Explorer picks it up here and hands it to
        # every terminal opened afterwards.
        if (-not ("SefyInstall.Env" -as [type])) {
            Add-Type -Namespace SefyInstall -Name Env -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("user32.dll", SetLastError = true, CharSet = System.Runtime.InteropServices.CharSet.Unicode)]
public static extern System.IntPtr SendMessageTimeout(System.IntPtr hWnd, uint Msg, System.UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out System.UIntPtr lpdwResult);
'@
        }
        $result = [System.UIntPtr]::Zero
        # HWND_BROADCAST = 0xffff, WM_SETTINGCHANGE = 0x1A, SMTO_ABORTIFHUNG = 0x2
        [SefyInstall.Env]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, "Environment", 0x2, 5000, [ref]$result) | Out-Null
        Write-Host "Added $dir to your user PATH - open a new terminal to pick it up."
    }
} catch {
    Write-Host "Note: could not update the user PATH ($($_.Exception.Message)); add $dir to it yourself."
}
Write-Host "Installed sefy $tag to $dir\sefy.exe"

# The desktop app: the release's own installer, run silently for this user
# only - no administrator, nothing outside your account. It is what puts sefy
# in the Start menu and in Apps. A window that is open holds an unlocked
# vault, and replacing the program under it is the person's to do, so a
# running one is left as it is and the note says how to finish.
$version = $tag.Substring(1)
$app = Join-Path $env:LOCALAPPDATA "sefy\sefy-gui.exe"
if (Get-Process sefy-gui -ErrorAction SilentlyContinue) {
    Write-Host "Note: the desktop app is open - lock it, close it and run the installer again to update it."
} else {
    $setupName = "sefy_${version}_x64-setup.exe"
    $tmp = Join-Path ([IO.Path]::GetTempPath()) "sefy-install-$([guid]::NewGuid())"
    New-Item -ItemType Directory -Force $tmp | Out-Null
    try {
        $setup = Join-Path $tmp $setupName
        $setupUrl = "https://github.com/$repo/releases/download/$tag/$setupName"
        Write-Host "Downloading $setupUrl"
        # The progress bar of Windows PowerShell slows the download itself
        # down by an order of magnitude.
        $progress = $ProgressPreference
        $ProgressPreference = "SilentlyContinue"
        try {
            Invoke-WebRequest $setupUrl -OutFile $setup -UseBasicParsing
        } finally {
            $ProgressPreference = $progress
        }
        # Started as a process of its own and waited for: the arguments reach
        # the installer intact only this way.
        $run = Start-Process -FilePath $setup -ArgumentList "/S", "/CURRENTUSER" -PassThru -Wait
        if ($run.ExitCode -eq 2) {
            # An upgrade can stop at the old version's uninstaller, which the
            # installer runs first and gives up on when it does not answer 0.
            # Its update mode installs over the old files instead.
            $run = Start-Process -FilePath $setup -ArgumentList "/S", "/UPDATE", "/CURRENTUSER" -PassThru -Wait
        }
        if ($run.ExitCode -ne 0) {
            throw "The desktop app's installer stopped with code $($run.ExitCode)"
        }
    } finally {
        Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
    # The file, not the exit code, says what is installed: an installer can
    # finish cleanly and leave the old version in place.
    if (-not (Test-Path $app)) {
        throw "The desktop app's installer finished, but $app is not there"
    }
    $installed = (Get-Item $app).VersionInfo.FileVersion
    if ($installed -ne $version) {
        throw "The desktop app's installer finished, but $app is version $installed rather than $version"
    }
    Write-Host "Installed the sefy desktop app $tag to $app - start it from the Start menu."
}
