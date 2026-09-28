#Requires -Version 7.2
# Windows signing (Azure Artifact Signing) and checks for .github/workflows/release.yml (the
# `app-windows` and `cli-windows` jobs). One file, so the release and the rehearsal run exactly the
# same commands, and Tauri's signCommand (app) and the CLI job sign through the same function.
#
#   preflight          fail loudly, naming what's missing, before any build
#   install-tools      pinned SignTool + Artifact Signing dlib (size and hash checked, into an
#                      emptied folder), metadata.json
#   tauri-config       the CI-only Tauri config whose signCommand runs `sign`
#   check-azure        after azure/login: Entra ID issues a token for Artifact Signing
#   sign <file>        sign and timestamp one file, then check it (Tauri's signCommand calls this)
#   verify <file>...   signature, Public Trust chain, publisher CN, timestamp
#   verify-installers <setup.exe> [<x.msi>]
#                      the installers, then every .exe/.dll they install or contain
#   sign-out           clear the Azure CLI login, delete metadata.json (runs even on failure)
#
# e.g. `pwsh -NoProfile -File .github/scripts/windows-signing.ps1 preflight`
#
# There is no Windows secret. azure/login signs in with this job's GitHub OIDC token and leaves a
# short-lived Entra token in the Azure CLI's cache; the dlib gets it through AzureCliCredential.
# Never print the output of `az account get-access-token` without `--query expiresOn`.
# Everything here is written for PowerShell 7 (pwsh) on the Windows runner.

param(
  [Parameter(Position = 0)] [string] $Command = '',
  [Parameter(Position = 1, ValueFromRemainingArguments = $true)] [string[]] $Rest = @()
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
# Native commands report through $LASTEXITCODE, which every call below checks.
$PSNativeCommandUseErrorActionPreference = $false
Set-StrictMode -Version 3.0

$TempRoot = [Environment]::GetEnvironmentVariable('RUNNER_TEMP')
if (-not $TempRoot) {
  Write-Host '::error::RUNNER_TEMP is not set (this script runs on a GitHub Actions runner)'
  exit 1
}
# The tools, metadata.json and the Tauri config live here while a job runs.
$SigningDir = Join-Path $TempRoot 'pagelamp-signing'

$TimestampUrl = 'http://timestamp.acs.microsoft.com'
$Description = 'PageLamp'
$DescriptionUrl = 'https://github.com/Euswbnix/pagelamp'
# Artifact Signing's Public Trust certificates chain to this root (Microsoft Learn, "Artifact
# Signing trust models"); the intermediate CA changes over time, the root doesn't.
$PublicTrustRoot = 'Microsoft Identity Verification Root Certificate Authority 2020'
# The token audience of the Artifact Signing service.
$SigningScope = 'https://codesigning.azure.net/.default'

# The variables the owner sets in the GitHub environment `release` (none of them is a secret).
$Variables = @(
  'AZURE_CLIENT_ID'
  'AZURE_TENANT_ID'
  'AZURE_SUBSCRIPTION_ID'
  'AZURE_ARTIFACT_SIGNING_ENDPOINT'
  'AZURE_ARTIFACT_SIGNING_ACCOUNT'
  'AZURE_ARTIFACT_SIGNING_PROFILE'
  'AZURE_ARTIFACT_SIGNING_PUBLISHER'
)

# The pair of packages Microsoft's own Azure/artifact-signing-action pins (action.yml on main,
# 2026-09-27). Size and SHA-512 are nuget.org's (catalog entry `packageSize`/`packageHash`), so a
# changed download fails here. To update: bump both versions together and copy the new values from
# https://api.nuget.org/v3/registration5-gz-semver2/<id>/<version>.json (follow catalogEntry).
$Packages = @(
  [pscustomobject]@{
    Id      = 'Microsoft.Windows.SDK.BuildTools'
    Version = '10.0.26100.4188'
    Size    = 22380379
    Sha512  = '3A489185D0EBD27803EBEC26D04CDDBB59A057C66E9275DD1F08F37875598D84978BD6C291892AF4DB3F5FF69BF41A076FE4EAFF9FD19976DD356B42D2A1E2A5'
    File    = 'bin\10.0.26100.0\x64\signtool.exe'
    EnvName = 'PAGELAMP_SIGNTOOL'
  }
  [pscustomobject]@{
    Id      = 'Microsoft.ArtifactSigning.Client'
    Version = '1.0.128'
    Size    = 14653224
    Sha512  = '98F06A691F4FC2FA22F19DCF8556733E98607FBEF91A312C453B9B0798CC9088DAE0ACB36E389B552A11B4D2320324785B8541C2B51091A724C05BC5DF5CBF95'
    File    = 'bin\x64\Azure.CodeSigning.Dlib.dll'
    EnvName = 'PAGELAMP_SIGNING_DLIB'
  }
)

# ---- output --------------------------------------------------------------------------------------

# Write-Host, not Write-Output, so nothing ends up in a function's return value. The signing log
# also keeps what Tauri swallows when a signCommand fails (it shows only "failed to run pwsh").
function Write-Log([string] $Text) {
  Write-Host $Text
  $log = [Environment]::GetEnvironmentVariable('PAGELAMP_SIGNING_LOG')
  if ($log) {
    try { Add-Content -LiteralPath $log -Value $Text } catch { Write-Host "(couldn't write the signing log: $_)" }
  }
}

function Fail([string] $Message) {
  Write-Log "::error::$Message"
  exit 1
}

# Runs a native command, logs its output (stdout and stderr) and returns only its exit code.
function Invoke-Native([string] $Exe, [string[]] $Arguments) {
  $lines = & $Exe @Arguments 2>&1 | ForEach-Object { "$_" }
  $code = $LASTEXITCODE
  foreach ($line in @($lines)) { Write-Log "  $line" }
  return $code
}

function Set-JobEnv([string] $Name, [string] $Value) {
  $file = Get-Var 'GITHUB_ENV'
  if (-not $file) { Fail 'GITHUB_ENV is not set' }
  Add-Content -LiteralPath $file -Value "$Name=$Value"
}

function Get-Var([string] $Name) {
  return [Environment]::GetEnvironmentVariable($Name)
}

function Assert-Vars([string[]] $Names) {
  foreach ($n in $Names) {
    if ([string]::IsNullOrWhiteSpace((Get-Var $n))) { Fail "$n is not set in this step (see 'Check signing setup')" }
  }
}

# ---- setup ---------------------------------------------------------------------------------------

function Invoke-Preflight {
  $missing = @($Variables | Where-Object { [string]::IsNullOrWhiteSpace((Get-Var $_)) })
  if ($missing.Count -gt 0) {
    Fail ("Windows signing is not set up: missing variable(s) {0} in the GitHub environment 'release' (Settings > Environments > release > Environment variables). Windows builds are never released unsigned; add them and re-run this job." -f ($missing -join ', '))
  }
  $problems = @()
  $guid = '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'
  foreach ($n in 'AZURE_CLIENT_ID', 'AZURE_TENANT_ID', 'AZURE_SUBSCRIPTION_ID') {
    if ((Get-Var $n) -notmatch $guid) { $problems += "$n must be a GUID (xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx) with nothing around it" }
  }
  if ($env:AZURE_ARTIFACT_SIGNING_ENDPOINT -notmatch '^https://[a-z0-9]+\.codesigning\.azure\.net/?$') {
    $problems += "AZURE_ARTIFACT_SIGNING_ENDPOINT must be the account's regional endpoint, e.g. https://eus.codesigning.azure.net (East US)"
  }
  # Microsoft Learn, Artifact Signing quickstart: "Naming constraints".
  if ($env:AZURE_ARTIFACT_SIGNING_ACCOUNT -notmatch '^[A-Za-z](?!.*--)[A-Za-z0-9-]{1,22}[A-Za-z0-9]$') {
    $problems += 'AZURE_ARTIFACT_SIGNING_ACCOUNT must be the Artifact Signing account name (3-24 letters, digits or single hyphens)'
  }
  if ($env:AZURE_ARTIFACT_SIGNING_PROFILE -notmatch '^[A-Za-z](?!.*--)[A-Za-z0-9-]{3,98}[A-Za-z0-9]$') {
    $problems += 'AZURE_ARTIFACT_SIGNING_PROFILE must be the certificate profile name (5-100 letters, digits or single hyphens)'
  }
  $publisher = $env:AZURE_ARTIFACT_SIGNING_PUBLISHER
  if ($publisher -cne $publisher.Trim() -or $publisher -match '^\s*(CN|O)\s*=') {
    $problems += "AZURE_ARTIFACT_SIGNING_PUBLISHER must be only the certificate's CN (your validated legal name as the certificate profile shows it), without 'CN=' and without spaces around it"
  }
  if ($problems.Count -gt 0) {
    Fail ('Windows signing setup in the GitHub environment ''release'' is wrong: ' + ($problems -join '; '))
  }
  if ((Get-Var 'REHEARSAL') -eq 'true' -and (Get-Var 'INPUT_TAG')) {
    Fail "A rehearsal builds the branch or tag picked under 'Use workflow from'. Leave the 'tag' input empty."
  }
  Write-Log "Windows signing setup complete: all $($Variables.Count) variables are present (values not shown)."
}

function Show-ToolSignature([string] $File) {
  $sig = Get-AuthenticodeSignature -LiteralPath $File
  $subject = if ($sig.SignerCertificate) { $sig.SignerCertificate.Subject } else { '(no signature)' }
  $leaf = Split-Path -Leaf $File
  Write-Log "  ${leaf}: $($sig.Status), signed by $subject"
  if ($sig.Status -ne 'Valid' -or $subject -notmatch 'O=Microsoft Corporation') {
    Write-Log "::warning::$leaf isn't validly signed by Microsoft ($($sig.Status)); its nuget.org size and SHA-512 matched, so continuing."
  }
}

function Install-Tools {
  Assert-Vars @('AZURE_ARTIFACT_SIGNING_ENDPOINT', 'AZURE_ARTIFACT_SIGNING_ACCOUNT', 'AZURE_ARTIFACT_SIGNING_PROFILE')
  # The dlib is a .NET 8 component (Microsoft Learn, "Set up signing integrations"). The runner
  # image has had it; fail clearly if an image update drops it.
  if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) { Fail 'dotnet is not on this runner; the Artifact Signing dlib needs the .NET 8 runtime (add actions/setup-dotnet with dotnet-version 8.0.x before this step)' }
  $runtimes = @(& dotnet --list-runtimes 2>&1 | ForEach-Object { "$_" })
  if ($LASTEXITCODE -ne 0 -or @($runtimes -match '^Microsoft\.NETCore\.App 8\.').Count -eq 0) {
    Fail 'The .NET 8 runtime is missing on this runner; the Artifact Signing dlib needs it (add actions/setup-dotnet with dotnet-version 8.0.x before this step)'
  }

  Add-Type -AssemblyName System.IO.Compression.ZipFile
  # Start empty, so the tools folder holds only the hash-checked packages: extracting over files
  # an earlier step left there (the build ran on this runner) would keep any extra DLL next to
  # SignTool or the dlib. If something still holds a file in there, this fails, as it should.
  $logDir = Join-Path $TempRoot 'signing-logs'
  foreach ($dir in $SigningDir, $logDir) {
    if (-not (Test-Path -LiteralPath $dir)) { continue }
    try { Remove-Item -LiteralPath $dir -Recurse -Force }
    catch { Fail "Couldn't empty $dir before installing the signing tools (a process from an earlier step may still be using it): $_" }
  }
  $tools = Join-Path $SigningDir 'tools'
  New-Item -ItemType Directory -Force -Path $tools | Out-Null
  New-Item -ItemType Directory -Force -Path $logDir | Out-Null

  foreach ($p in $Packages) {
    $id = $p.Id.ToLowerInvariant()
    $version = $p.Version.ToLowerInvariant()
    $url = "https://api.nuget.org/v3-flatcontainer/$id/$version/$id.$version.nupkg"
    # A .nupkg is a zip file.
    $zip = Join-Path $tools "$id.$version.zip"
    Write-Log "Downloading $($p.Id) $($p.Version) from nuget.org..."
    Invoke-WebRequest -Uri $url -OutFile $zip -MaximumRetryCount 3 -RetryIntervalSec 10
    $size = (Get-Item -LiteralPath $zip).Length
    $hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA512).Hash
    if ($size -ne $p.Size -or $hash -ne $p.Sha512) {
      Fail "$($p.Id) $($p.Version) from nuget.org isn't the pinned package (size $size, SHA-512 $hash; expected $($p.Size), $($p.Sha512))"
    }
    $dest = Join-Path $tools "$id.$version"
    [System.IO.Compression.ZipFile]::ExtractToDirectory($zip, $dest, $true)
    # GetFullPath gives backslashes, which LoadLibrary needs for the dlib path.
    $file = [System.IO.Path]::GetFullPath((Join-Path $dest $p.File))
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { Fail "$($p.File) is missing from $($p.Id) $($p.Version)" }
    Show-ToolSignature $file
    Set-Item -Path "env:$($p.EnvName)" -Value $file
    Set-JobEnv $p.EnvName $file
  }
  # Tauri checks with this SignTool whether the sidecar is signed already.
  Set-JobEnv 'TAURI_WINDOWS_SIGNTOOL_PATH' $env:PAGELAMP_SIGNTOOL

  # Only the Azure CLI login from azure/login may be used, so every other credential type that
  # DefaultAzureCredential would try is excluded, including EnvironmentCredential (tried before the
  # CLI; it would pick up an AZURE_CLIENT_SECRET or certificate set later). This also avoids slow
  # probes (e.g. the managed-identity endpoint). The names are those in Microsoft Learn's
  # metadata.json example plus EnvironmentCredential, all in dlib 1.0.128's own list; the dlib
  # rejects a name it doesn't know ("is not a valid option").
  $metadata = [ordered]@{
    Endpoint               = $env:AZURE_ARTIFACT_SIGNING_ENDPOINT
    CodeSigningAccountName = $env:AZURE_ARTIFACT_SIGNING_ACCOUNT
    CertificateProfileName = $env:AZURE_ARTIFACT_SIGNING_PROFILE
    CorrelationId          = "pagelamp-${env:GITHUB_RUN_ID}-${env:GITHUB_RUN_ATTEMPT}-${env:GITHUB_JOB}"
    ExcludeCredentials     = @(
      'EnvironmentCredential'
      'ManagedIdentityCredential'
      'WorkloadIdentityCredential'
      'SharedTokenCacheCredential'
      'VisualStudioCredential'
      'VisualStudioCodeCredential'
      'AzurePowerShellCredential'
      'AzureDeveloperCliCredential'
      'InteractiveBrowserCredential'
    )
  }
  $metadataPath = Join-Path $SigningDir 'metadata.json'
  $metadata | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $metadataPath -Encoding utf8NoBOM
  Set-JobEnv 'PAGELAMP_SIGNING_METADATA' $metadataPath
  Set-JobEnv 'PAGELAMP_SIGNING_LOG' (Join-Path $logDir 'windows-signing.log')
  Write-Log 'Signing tools ready: SignTool, the Artifact Signing dlib and metadata.json.'
}

# A config file merged over tauri.conf.json by `tauri bundle --config`, so the committed file stays
# as it is. Tauri replaces "%1" with each file to sign: the sidecar, the app executable, NSIS
# plugins, the uninstaller (from makensis's !uninstfinalize), the WiX extension DLLs it uses
# locally (not shipped), the installers. The script path uses forward slashes: Tauri hands the
# uninstaller command to NSIS as Rust's Debug form of the command, which would double every
# backslash.
function Write-TauriConfig {
  $scriptPath = $PSCommandPath -replace '\\', '/'
  $config = [ordered]@{
    bundle = [ordered]@{
      windows = [ordered]@{
        signCommand = [ordered]@{
          cmd  = 'pwsh'
          args = @('-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $scriptPath, 'sign', '%1')
        }
      }
    }
  }
  New-Item -ItemType Directory -Force -Path $SigningDir | Out-Null
  $path = Join-Path $SigningDir 'tauri.windows-signing.json'
  $config | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $path -Encoding utf8NoBOM
  Write-Log "Tauri signing config ($path):"
  Write-Log (Get-Content -Raw -LiteralPath $path)
  Set-JobEnv 'PAGELAMP_TAURI_SIGNING_CONFIG' $path
}

# Right after azure/login: the GitHub OIDC token it exchanged is valid for minutes only, and the
# Azure CLI needs it once more to get a token for the signing service. Getting that token now puts
# it in the CLI's cache (Entra access tokens for an app last about an hour), where the dlib finds
# it. Only the expiry is printed, never the token.
function Test-AzureAccess {
  if (-not (Get-Command az -ErrorAction SilentlyContinue)) { Fail 'The Azure CLI (az) is not on this runner' }
  $out = @(& az account get-access-token --scope $SigningScope --query expiresOn --output tsv --only-show-errors 2>&1 | ForEach-Object { "$_" })
  if ($LASTEXITCODE -ne 0 -or $out.Count -eq 0) {
    foreach ($line in $out) { Write-Log "  $line" }
    Fail "Signed in to Azure, but Microsoft Entra ID won't issue a token for Artifact Signing ($SigningScope). Check AZURE_TENANT_ID and AZURE_CLIENT_ID."
  }
  Write-Log "Azure sign-in works; a token for Artifact Signing is cached until $($out[-1]) (not shown)."
}

function Invoke-SignOut {
  if (Get-Command az -ErrorAction SilentlyContinue) {
    # What azure/login's own cleanup runs.
    $code = Invoke-Native 'az' @('account', 'clear', '--only-show-errors')
    if ($code -ne 0) { Write-Log "::warning::az account clear exited with $code" }
  }
  $metadataPath = Join-Path $SigningDir 'metadata.json'
  if (Test-Path -LiteralPath $metadataPath) { Remove-Item -LiteralPath $metadataPath -Force }
  Write-Log 'Signed out of Azure (Azure CLI accounts cleared); metadata.json removed.'
}

# ---- signing -------------------------------------------------------------------------------------

function Assert-SigningTools {
  foreach ($n in 'PAGELAMP_SIGNTOOL', 'PAGELAMP_SIGNING_DLIB', 'PAGELAMP_SIGNING_METADATA') {
    $value = Get-Var $n
    if (-not $value -or -not (Test-Path -LiteralPath $value -PathType Leaf)) { Fail "$n is not set or missing; run 'install-tools' first" }
  }
}

# Microsoft Learn's SignTool + dlib command, plus a description (Windows shows it as the program
# name in the UAC prompt for the .msi). Transient service or timestamp errors get two retries;
# signing an already signed file replaces its signature.
function Invoke-Sign([string] $File) {
  Assert-SigningTools
  if (-not (Test-Path -LiteralPath $File -PathType Leaf)) { Fail "Nothing to sign at $File" }
  $leaf = Split-Path -Leaf $File
  $arguments = @(
    'sign', '/v', '/fd', 'SHA256', '/tr', $TimestampUrl, '/td', 'SHA256',
    '/d', $Description, '/du', $DescriptionUrl,
    '/dlib', $env:PAGELAMP_SIGNING_DLIB, '/dmdf', $env:PAGELAMP_SIGNING_METADATA,
    $File
  )
  $code = 1
  for ($attempt = 1; $attempt -le 3; $attempt++) {
    Write-Log "Signing $File (attempt $attempt)..."
    $code = Invoke-Native $env:PAGELAMP_SIGNTOOL $arguments
    if ($code -eq 0) { break }
    if ($attempt -lt 3) {
      Write-Log "SignTool exited with $code; retrying in $(20 * $attempt) s..."
      Start-Sleep -Seconds (20 * $attempt)
    }
  }
  if ($code -ne 0) {
    Fail "SignTool couldn't sign $leaf (exit $code). A 403 means: the app registration lacks the 'Artifact Signing Certificate Profile Signer' role on the account, or AZURE_ARTIFACT_SIGNING_ENDPOINT/ACCOUNT/PROFILE don't match the account (see the signing log)."
  }
  Test-FileSignature $File
}

# signtool: valid under the default Authenticode policy, timestamped (/tw turns a missing timestamp
# into exit code 2), chained to Microsoft's Public Trust root. PowerShell: Windows' own verdict
# (Valid), the publisher's CN, and a timestamp countersignature.
function Test-FileSignature([string] $File) {
  Assert-Vars @('PAGELAMP_SIGNTOOL', 'AZURE_ARTIFACT_SIGNING_PUBLISHER')
  if (-not (Test-Path -LiteralPath $File -PathType Leaf)) { Fail "No file at $File" }
  $leaf = Split-Path -Leaf $File
  Write-Log "== $File"
  $code = Invoke-Native $env:PAGELAMP_SIGNTOOL @('verify', '/pa', '/all', '/tw', '/v', '/r', $PublicTrustRoot, $File)
  if ($code -ne 0) {
    Fail "signtool verify rejects $leaf (exit $code; 2 = no timestamp): it must be validly signed, timestamped and chained to '$PublicTrustRoot'"
  }
  $sig = Get-AuthenticodeSignature -LiteralPath $File
  if ($sig.Status -ne 'Valid') { Fail "Windows doesn't accept the signature on ${leaf}: $($sig.Status) ($($sig.StatusMessage))" }
  $cert = $sig.SignerCertificate
  $cn = $cert.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false)
  if ($cn -cne $env:AZURE_ARTIFACT_SIGNING_PUBLISHER) {
    Fail "$leaf is signed by '$cn', not by '$($env:AZURE_ARTIFACT_SIGNING_PUBLISHER)' (the variable AZURE_ARTIFACT_SIGNING_PUBLISHER)"
  }
  if (-not $sig.TimeStamperCertificate) { Fail "$leaf has no timestamp; its signature would stop being valid when the 3-day certificate expires" }
  Write-Log "  OK: publisher '$($cert.Subject)', issuer '$($cert.Issuer)', certificate $($cert.NotBefore.ToString('u')) to $($cert.NotAfter.ToString('u')), timestamp by '$($sig.TimeStamperCertificate.Subject)'"
}

# ---- installers ----------------------------------------------------------------------------------

function Get-SignableFiles([string] $Dir) {
  if (-not (Test-Path -LiteralPath $Dir -PathType Container)) { Fail "Nothing was installed into $Dir" }
  $found = @(Get-ChildItem -LiteralPath $Dir -Recurse -File | Where-Object { $_.Extension -in '.exe', '.dll' })
  if ($found.Count -eq 0) { Fail "No .exe or .dll under $Dir" }
  Write-Log ('Found: ' + (($found | ForEach-Object { $_.FullName.Substring($Dir.Length).TrimStart('\') }) -join ', '))
  return $found
}

function Wait-Installer([System.Diagnostics.Process] $Process, [string] $What) {
  if (-not $Process.WaitForExit(15 * 60 * 1000)) {
    $Process.Kill()
    Fail "$What didn't finish within 15 minutes"
  }
  return $Process.ExitCode
}

# What people actually get: install the -setup.exe silently (NSIS: /S; /D= sets the folder, last
# and unquoted) into a folder of our own and check every .exe/.dll it wrote. That includes
# uninstall.exe, which makensis signs through !uninstfinalize without checking the result, so an
# unsigned uninstaller would otherwise go unnoticed. The runner is discarded afterwards.
function Test-NsisInstaller([string] $Setup) {
  Test-FileSignature $Setup
  $dir = Join-Path $TempRoot 'pagelamp-install-check\nsis'
  if (Test-Path -LiteralPath $dir) { Remove-Item -LiteralPath $dir -Recurse -Force }
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $target = Join-Path $dir 'PageLamp'
  Write-Log "Installing $(Split-Path -Leaf $Setup) silently into $target to check what it installs..."
  $process = Start-Process -FilePath $Setup -ArgumentList @('/S', "/D=$target") -PassThru
  # Keeps the process handle, without which ExitCode can come back empty.
  $null = $process.Handle
  $exit = Wait-Installer $process 'The NSIS installer'
  if ($exit -ne 0) { Fail "The NSIS installer exited with $exit" }
  $files = @(Get-SignableFiles $target)
  $names = @($files | ForEach-Object { $_.Name.ToLowerInvariant() })
  foreach ($required in 'pagelamp.exe', 'uninstall.exe') {
    if ($names -notcontains $required) { Fail "The NSIS installer didn't install $required" }
  }
  if (@($files | Where-Object { $_.Extension -eq '.exe' }).Count -lt 3) {
    Fail 'Expected at least the app, pagelamp.exe and uninstall.exe from the NSIS installer'
  }
  foreach ($f in $files) { Test-FileSignature $f.FullName }
}

# An administrative install (msiexec /a) only extracts the files; nothing is installed.
function Test-MsiInstaller([string] $Msi) {
  Test-FileSignature $Msi
  $root = Join-Path $TempRoot 'pagelamp-install-check'
  $dir = Join-Path $root 'msi'
  if (Test-Path -LiteralPath $dir) { Remove-Item -LiteralPath $dir -Recurse -Force }
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $log = Join-Path $root 'msiexec-admin.log'
  Write-Log "Extracting $(Split-Path -Leaf $Msi) (administrative install) into $dir to check its files..."
  $process = Start-Process -FilePath 'msiexec.exe' -ArgumentList "/a `"$Msi`" /qn TARGETDIR=`"$dir`" /L*v `"$log`"" -PassThru
  $null = $process.Handle
  $exit = Wait-Installer $process 'msiexec /a'
  if ($exit -ne 0) {
    if (Test-Path -LiteralPath $log) { Get-Content -LiteralPath $log -Tail 40 | ForEach-Object { Write-Log "  $_" } }
    Fail "msiexec /a exited with $exit"
  }
  $files = @(Get-SignableFiles $dir)
  $names = @($files | ForEach-Object { $_.Name.ToLowerInvariant() })
  if ($names -notcontains 'pagelamp.exe') { Fail "The .msi doesn't contain pagelamp.exe" }
  if (@($files | Where-Object { $_.Extension -eq '.exe' }).Count -lt 2) { Fail 'Expected at least the app and pagelamp.exe in the .msi' }
  foreach ($f in $files) { Test-FileSignature $f.FullName }
}

function Test-Installers([string[]] $Paths) {
  $list = @($Paths | Where-Object { $_ })
  if ($list.Count -eq 0) { Fail 'usage: verify-installers <setup.exe> [<x.msi>]' }
  foreach ($installer in $list) {
    switch ([System.IO.Path]::GetExtension($installer).ToLowerInvariant()) {
      '.exe' { Test-NsisInstaller $installer }
      '.msi' { Test-MsiInstaller $installer }
      default { Fail "Not an installer: $installer" }
    }
  }
  Write-Log "Installers and every file they install are signed by '$($env:AZURE_ARTIFACT_SIGNING_PUBLISHER)' and timestamped."
}

# ---- main ----------------------------------------------------------------------------------------

$Rest = @($Rest | Where-Object { $null -ne $_ })
switch ($Command) {
  'preflight' { Invoke-Preflight }
  'install-tools' { Install-Tools }
  'tauri-config' { Write-TauriConfig }
  'check-azure' { Test-AzureAccess }
  'sign' {
    if ($Rest.Count -ne 1) { Fail 'usage: sign <file>' }
    Invoke-Sign $Rest[0]
  }
  'verify' {
    if ($Rest.Count -eq 0) { Fail 'usage: verify <file>...' }
    foreach ($f in $Rest) { Test-FileSignature $f }
  }
  'verify-installers' { Test-Installers $Rest }
  'sign-out' { Invoke-SignOut }
  default {
    Write-Host 'usage: windows-signing.ps1 preflight|install-tools|tauri-config|check-azure|sign <file>|verify <file>...|verify-installers <setup.exe> [<x.msi>]|sign-out'
    exit 2
  }
}
exit 0
