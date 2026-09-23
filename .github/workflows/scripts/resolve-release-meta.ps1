# Resolves the release version and tag for the Release workflow and decides
# whether a release should be produced.
#
# Reads the package version from the root Cargo.toml and derives the only valid
# release tag, v<version>. Tag-triggered runs must use that exact tag. It checks
# whether a release (draft included) already exists and writes only the outputs
# consumed by later workflow steps.

$cargo = Get-Content Cargo.toml -Raw
if ($cargo -notmatch '(?ms)^\[package\]\s*(?<packageBody>(?:(?!^\s*\[).)*)') {
  Write-Error "could not find [package] table in Cargo.toml"
  exit 1
}
$packageBody = $Matches['packageBody']
if ($packageBody -notmatch '(?m)^\s*version\s*=\s*"([^"]+)"') {
  Write-Error "could not find [package] version in Cargo.toml"
  exit 1
}
$cargoVersion = $Matches[1]

if ($env:GITHUB_REF_TYPE -eq 'tag') {
  # A tag push: the tag is authoritative, but must match Cargo.toml.
  $tag = $env:GITHUB_REF_NAME
  $version = $tag -replace '^v', ''
  if ($version -ne $cargoVersion) {
    Write-Error "tag '$tag' (version '$version') does not match Cargo.toml version '$cargoVersion'"
    exit 1
  }
}
else {
  # Branch pushes and manual dispatches always derive the tag from Cargo.toml.
  $version = $cargoVersion
  $tag = "v$version"
}

# Reject a malformed resolved tag before exposing it to later steps.
if ($tag -notmatch '^v\d+\.\d+\.\d+([-.+][0-9A-Za-z.-]*)?$') {
  Write-Error "resolved tag '$tag' is not a valid version tag (expected e.g. v1.2.3)"
  exit 1
}

Write-Host "Release tag: $tag (version $version)"
"tag=$tag"         | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8

# Idempotency guard: `gh release view` sees DRAFT releases too (a draft
# does not create the git tag until published, so a tag lookup would
# miss it). If a release already exists for this tag, this run is a
# no-op: unbumped pushes to master and workflow re-runs never
# re-release.
gh release view "$tag" 2>$null | Out-Null
# Capture the lookup result immediately: a missing release makes `gh`
# exit 1, and the cmdlets below do not reset $LASTEXITCODE, so the script
# must end on an explicit `exit 0` (GitHub appends `exit $LASTEXITCODE`
# to pwsh steps) to avoid failing the create-a-release path.
$exists = ($LASTEXITCODE -eq 0)
if ($exists) {
  Write-Host "Release $tag already exists; skipping build and release."
  "should_release=false" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
}
else {
  Write-Host "No release exists for $tag yet; proceeding."
  "should_release=true" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
}
exit 0
