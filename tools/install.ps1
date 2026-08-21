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
    throw "Cannot resolve the latest release of $repo - set `$env:SEFY_VERSION to a tag like v0.4.0"
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

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if (($userPath -split ";") -notcontains $dir) {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$dir", "User")
    Write-Host "Added $dir to your user PATH - restart the terminal to pick it up."
}
Write-Host "Installed sefy $tag to $dir\sefy.exe"
