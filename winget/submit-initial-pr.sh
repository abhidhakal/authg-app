#!/bin/bash
# Submits winget/manifests/a/abhidhakal/authg/<version>/ to microsoft/winget-pkgs as a PR.
# Only needed once: after the package exists, the release workflow's winget job updates it.
# Usage: winget/submit-initial-pr.sh [version]   (default: version from package.json)
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=${1:-$(node -p "require('./package.json').version")}
SRC="winget/manifests/a/abhidhakal/authg/$VERSION"
DEST="manifests/a/abhidhakal/authg/$VERSION"
BRANCH="authg-$VERSION"
[ -d "$SRC" ] || { echo "No manifests at $SRC"; exit 1; }
gh auth status >/dev/null 2>&1 || { echo "Run: gh auth login"; exit 1; }
ME=$(gh api user --jq .login)

echo "Forking microsoft/winget-pkgs (no-op if the fork exists)..."
gh repo fork microsoft/winget-pkgs --clone=false >/dev/null 2>&1 || true
until gh api "repos/$ME/winget-pkgs" >/dev/null 2>&1; do sleep 3; done

# Branch from upstream master via the API: no multi-GB clone needed
BASE=$(gh api repos/microsoft/winget-pkgs/git/ref/heads/master --jq .object.sha)
gh api -X POST "repos/$ME/winget-pkgs/git/refs" -f ref="refs/heads/$BRANCH" -f sha="$BASE" >/dev/null

for f in "$SRC"/*.yaml; do
  name=$(basename "$f")
  echo "Adding $name"
  gh api -X PUT "repos/$ME/winget-pkgs/contents/$DEST/$name" \
    -f message="New package: abhidhakal.authg version $VERSION" \
    -f content="$(base64 < "$f" | tr -d '\n')" \
    -f branch="$BRANCH" >/dev/null
done

gh pr create --repo microsoft/winget-pkgs --base master --head "$ME:$BRANCH" \
  --title "New package: abhidhakal.authg version $VERSION" \
  --body "- **Package Identifier**: abhidhakal.authg
- **Version**: $VERSION
- **Publisher**: Abhinav Dhakal
- **License**: MIT
- **Homepage**: https://github.com/abhidhakal/authg-app"
