[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidatePattern('^v?\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$')]
    [string]$ReleaseTag,

    [switch]$Preview
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repository = 'xarthurx/whisperi'
$packageIdentifier = 'xarthurx.Whisperi'
$normalizedTag = if ($ReleaseTag.StartsWith('v')) { $ReleaseTag } else { "v$ReleaseTag" }
$version = $normalizedTag.Substring(1)

$wingetCreate = Get-Command wingetcreate -ErrorAction SilentlyContinue
if (-not $wingetCreate) {
    throw 'WinGetCreate is not installed. Run: winget install --id Microsoft.WingetCreate --exact'
}

$headers = @{
    Accept = 'application/vnd.github+json'
    'X-GitHub-Api-Version' = '2022-11-28'
    'User-Agent' = 'Whisperi-WinGet-Release'
}
$releaseApiUrl = "https://api.github.com/repos/$repository/releases/tags/$normalizedTag"

Write-Host "Resolving GitHub release $normalizedTag..."
try {
    $release = Invoke-RestMethod -Headers $headers -Uri $releaseApiUrl
} catch {
    throw "Unable to resolve published release $normalizedTag from $releaseApiUrl. $($_.Exception.Message)"
}

if ($release.draft -or $release.prerelease) {
    throw "Release $normalizedTag must be published and non-prerelease before submitting it to WinGet."
}

$installerAssets = @($release.assets | Where-Object { $_.name -like '*_x64-setup.exe' })
if ($installerAssets.Count -ne 1) {
    throw "Expected exactly one x64 setup asset on release $normalizedTag, found $($installerAssets.Count)."
}

$installerAsset = $installerAssets[0]
$installerUrl = [string]$installerAsset.browser_download_url
$releaseDate = ([DateTimeOffset]$release.published_at).UtcDateTime.ToString('yyyy-MM-dd')
$releaseNotesUrl = "https://github.com/$repository/releases/tag/$normalizedTag"

$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$workDirectory = Join-Path $tempRoot ("whisperi-winget-$version-" + [guid]::NewGuid().ToString('N'))
$outputDirectory = Join-Path $workDirectory 'manifests'
$locationPushed = $false

function Assert-ManifestMatch {
    param(
        [Parameter(Mandatory = $true)][string]$Content,
        [Parameter(Mandatory = $true)][string]$Pattern,
        [Parameter(Mandatory = $true)][string]$Message
    )

    if ($Content -notmatch $Pattern) {
        throw $Message
    }
}

try {
    New-Item -ItemType Directory -Path $outputDirectory | Out-Null
    Push-Location $workDirectory
    $locationPushed = $true

    Write-Host "Generating manifests for $packageIdentifier $version..."
    & $wingetCreate.Source update `
        $packageIdentifier `
        --urls $installerUrl `
        --version $version `
        --release-date $releaseDate `
        --out $outputDirectory

    if ($LASTEXITCODE -ne 0) {
        throw "WinGetCreate manifest generation failed with exit code $LASTEXITCODE."
    }

    $manifestFiles = @(Get-ChildItem -LiteralPath $outputDirectory -Recurse -File -Filter '*.yaml')
    $installerManifests = @($manifestFiles | Where-Object { $_.Name -like '*.installer.yaml' })
    $localeManifests = @($manifestFiles | Where-Object { $_.Name -like '*.locale.en-US.yaml' })
    $versionManifests = @($manifestFiles | Where-Object {
        $_.Name -notlike '*.installer.yaml' -and $_.Name -notlike '*.locale.*.yaml'
    })

    if ($manifestFiles.Count -ne 3 -or $installerManifests.Count -ne 1 -or
        $localeManifests.Count -ne 1 -or $versionManifests.Count -ne 1) {
        throw "Expected one installer, one en-US locale, and one version manifest; found $($manifestFiles.Count) YAML files."
    }

    $installerManifest = $installerManifests[0]
    $localeManifest = $localeManifests[0]
    $versionManifest = $versionManifests[0]
    $manifestDirectories = @($manifestFiles | ForEach-Object { $_.Directory.FullName } | Sort-Object -Unique)
    if ($manifestDirectories.Count -ne 1) {
        throw 'Generated WinGet manifests were not written to one version directory.'
    }

    $installerContent = Get-Content -LiteralPath $installerManifest.FullName -Raw
    $localeContent = Get-Content -LiteralPath $localeManifest.FullName -Raw
    $versionContent = Get-Content -LiteralPath $versionManifest.FullName -Raw
    $allContent = @($installerContent, $localeContent, $versionContent)
    $escapedVersion = [regex]::Escape($version)

    foreach ($content in $allContent) {
        Assert-ManifestMatch $content '(?m)^PackageIdentifier:\s*xarthurx\.Whisperi\s*$' 'Generated manifest has the wrong package identifier.'
        Assert-ManifestMatch $content "(?m)^PackageVersion:\s*$escapedVersion\s*$" 'Generated manifest has the wrong package version.'
    }

    Assert-ManifestMatch $installerContent '(?m)^InstallerType:\s*nullsoft\s*$' 'Generated installer manifest is not NSIS/nullsoft.'
    Assert-ManifestMatch $installerContent '(?m)^\s*-?\s*Architecture:\s*x64\s*$' 'Generated installer manifest is not x64.'
    Assert-ManifestMatch $installerContent ("(?m)^\s*InstallerUrl:\s*" + [regex]::Escape($installerUrl) + '\s*$') 'Generated installer URL does not match the GitHub release asset.'
    Assert-ManifestMatch $installerContent ("(?m)^ReleaseDate:\s*" + [regex]::Escape($releaseDate) + '\s*$') 'Generated release date does not match the GitHub publication date.'

    $digestProperty = $installerAsset.PSObject.Properties['digest']
    if ($digestProperty -and ([string]$digestProperty.Value).StartsWith('sha256:')) {
        $expectedSha256 = ([string]$digestProperty.Value).Substring(7).ToUpperInvariant()
        Assert-ManifestMatch $installerContent "(?m)^\s*InstallerSha256:\s*$expectedSha256\s*$" 'Generated installer hash does not match the GitHub asset digest.'
    }

    Assert-ManifestMatch $localeContent '(?m)^License:\s*MIT\s*$' 'WinGet locale manifest must use the SPDX license identifier MIT.'
    Assert-ManifestMatch $localeContent '(?m)^LicenseUrl:\s*https://github\.com/xarthurx/whisperi/blob/main/LICENSE\s*$' 'WinGet locale manifest has the wrong or missing LicenseUrl.'
    Assert-ManifestMatch $localeContent ("(?m)^ReleaseNotesUrl:\s*" + [regex]::Escape($releaseNotesUrl) + '\s*$') 'WinGet locale manifest has the wrong release-notes URL.'

    $shortDescriptionMatch = [regex]::Match($localeContent, '(?m)^ShortDescription:\s*(?<value>\S.*)$')
    if (-not $shortDescriptionMatch.Success -or $shortDescriptionMatch.Groups['value'].Value.Trim().Length -gt 120) {
        throw 'WinGet ShortDescription must be one concise line of at most 120 characters.'
    }

    Write-Host 'Manifest metadata validation passed.'
    if ($Preview) {
        Write-Host 'Preview requested; no WinGet pull request was submitted.'
        return
    }

    $manifestDirectory = $manifestDirectories[0]
    $pullRequestTitle = "New version: $packageIdentifier version $version"
    Write-Host 'Submitting manifests with WinGetCreate cached OAuth authentication...'
    & $wingetCreate.Source submit $manifestDirectory --prtitle $pullRequestTitle --no-open

    if ($LASTEXITCODE -ne 0) {
        throw "WinGetCreate submission failed with exit code $LASTEXITCODE. Run 'wingetcreate token -s' to refresh the cached OAuth login."
    }
} finally {
    if ($locationPushed) {
        Pop-Location
    }

    $resolvedWorkDirectory = [IO.Path]::GetFullPath($workDirectory)
    $resolvedTempRoot = [IO.Path]::GetFullPath($tempRoot).TrimEnd('\') + '\'
    $safeLeafName = [IO.Path]::GetFileName($resolvedWorkDirectory).StartsWith('whisperi-winget-')
    $insideTempRoot = $resolvedWorkDirectory.StartsWith($resolvedTempRoot, [StringComparison]::OrdinalIgnoreCase)
    if ((Test-Path -LiteralPath $resolvedWorkDirectory) -and $safeLeafName -and $insideTempRoot) {
        Remove-Item -LiteralPath $resolvedWorkDirectory -Recurse -Force
    }
}
