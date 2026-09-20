#!/bin/bash
set -e

echo "=== Submitting AuthG 1.0.4 to microsoft/winget-pkgs ==="

# Check if gh CLI is installed
if ! command -v gh &> /dev/null; then
    echo "Error: GitHub CLI (gh) is not installed."
    echo "Please install it with: brew install gh"
    exit 1
fi

# Ensure logged in
if ! gh auth status &> /dev/null; then
    echo "Please authenticate with GitHub first: gh auth login"
    exit 1
fi

TEMP_DIR=$(mktemp -d)
echo "Working in temporary directory: $TEMP_DIR"

echo "1. Forking and cloning microsoft/winget-pkgs..."
gh repo fork microsoft/winget-pkgs --clone --depth 1 "$TEMP_DIR/winget-pkgs"

cd "$TEMP_DIR/winget-pkgs"
BRANCH_NAME="authg-1.0.4-$(date +%s)"
git checkout -b "$BRANCH_NAME"

MANIFEST_DIR="manifests/a/abhidhakal/authg/1.0.4"
mkdir -p "$MANIFEST_DIR"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cp "$SCRIPT_DIR/manifests/a/abhidhakal/authg/1.0.4/"*.yaml "$MANIFEST_DIR/"

git add "$MANIFEST_DIR"
git commit -m "New package: abhidhakal.authg version 1.0.4"

echo "2. Pushing branch to your fork..."
git push origin "$BRANCH_NAME"

echo "3. Creating Pull Request to microsoft/winget-pkgs..."
gh pr create \
    --repo microsoft/winget-pkgs \
    --title "New package: abhidhakal.authg version 1.0.4" \
    --body "### Package details
- **Package Identifier**: abhidhakal.authg
- **Package Version**: 1.0.4
- **Publisher**: Abhinav Dhakal
- **Description**: Minimal native desktop Google Authenticator with encrypted local vault
- **License**: MIT
- **Homepage**: https://github.com/abhidhakal/authg-app" \
    --head "$BRANCH_NAME" \
    --base master

echo "=== Successfully opened Winget PR! ==="
echo "Microsoft's automated validation bot will test and merge your package."
