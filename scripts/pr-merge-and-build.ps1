# PR: create + merge (squash) + checkout main + pull + delete branch + ClipWave Tauri build
# Run from repo root. Requires: git, gh (GitHub CLI), npm.

$ErrorActionPreference = "Stop"
$branch = git branch --show-current
if ($branch -eq "main") {
  Write-Error "On main. Create a feature branch first, commit, then run this script."
  exit 1
}

Write-Host "Branch: $branch"
Write-Host "Pushing..."
git push -u origin HEAD
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# Non-interactive PR create (title/body from last commit)
$title = git log -1 --pretty=%s
$body  = git log -1 --pretty=%b
if ([string]::IsNullOrWhiteSpace($body)) { $body = "Auto PR from $branch" }
Write-Host "Creating PR..."
gh pr create --title $title --body $body
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Merging PR (squash)..."
gh pr merge --squash
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Checkout main and pull..."
git checkout main
git pull
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Deleting branch (local + remote)..."
git branch -d $branch
git push origin --delete $branch
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Building ClipWave (Tauri app/installer)..."
npm run build:app
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "Done: PR merged, branch deleted, app built."
