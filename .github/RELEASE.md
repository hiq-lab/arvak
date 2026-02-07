# Arvak Release Process

This document describes how to release a new version of the Arvak Python package to PyPI.

## Prerequisites

### 1. Create a PyPI Account and API Token

1. Register at https://pypi.org/account/register/
2. Enable 2FA (required for API tokens)
3. Create an API token:
   - Go to https://pypi.org/manage/account/token/
   - Click "Add API token"
   - Token name: "arvak-github-actions"
   - Scope: "Entire account" (initially)
   - **After first release**, narrow scope to just the "arvak" project

4. Save the token securely - you'll only see it once!

### 2. Add PyPI Token to GitHub Secrets

1. Go to your GitHub repository: https://github.com/hiq-lab/arvak
2. Navigate to **Settings** → **Secrets and variables** → **Actions**
3. Click **New repository secret**
4. Name: `PYPI_API_TOKEN`
5. Value: Paste your PyPI API token (starts with `pypi-`)
6. Click **Add secret**

## Release Workflow

The GitHub Actions workflow automatically:
- ✅ Builds wheels for **macOS** (Intel + ARM64)
- ✅ Builds wheels for **Linux** (x86_64 + aarch64, glibc + musl)
- ✅ Builds wheels for **Windows** (x64)
- ✅ Supports **Python 3.9, 3.10, 3.11, 3.12**
- ✅ Publishes to PyPI automatically on git tag push

## How to Release

### Step 1: Update Version Numbers

Before releasing, ensure version numbers are consistent:

1. **Update workspace version** in `/Cargo.toml`:
   ```toml
   [workspace.package]
   version = "1.0.1"  # Increment as needed
   ```

2. **Update Python package version** in `/crates/arvak-python/pyproject.toml`:
   ```toml
   [project]
   version = "1.0.1"  # Must match Cargo.toml
   ```

3. **Update CHANGELOG.md** with release notes

### Step 2: Commit and Push Changes

```bash
git add Cargo.toml crates/arvak-python/pyproject.toml CHANGELOG.md
git commit -m "Release v1.0.1"
git push origin main
```

### Step 3: Create and Push a Git Tag

```bash
# Create an annotated tag
git tag -a v1.0.1 -m "Release version 1.0.1"

# Push the tag to GitHub (this triggers the release workflow)
git push origin v1.0.1
```

### Step 4: Monitor the Release

1. Go to **Actions** tab on GitHub: https://github.com/hiq-lab/arvak/actions
2. Watch the "Release" workflow run
3. It takes ~15-30 minutes to build all platforms
4. Once complete, check PyPI: https://pypi.org/project/arvak/

### Step 5: Verify the Release

```bash
# Create a fresh virtual environment
python3 -m venv test-env
source test-env/bin/activate

# Install from PyPI
pip install arvak

# Test it works
python -c "import arvak; print(f'Arvak version: {arvak.__version__}')"
python -c "import arvak; qc = arvak.Circuit('test', 2); qc.h(0).cx(0,1); print('Success!')"

# Clean up
deactivate
rm -rf test-env
```

## Troubleshooting

### Build Fails on Specific Platform

- Check the Actions logs for the failing job
- Common issues:
  - Missing dependencies in Cargo.toml
  - Platform-specific code issues
  - Test failures on specific Python versions

### PyPI Upload Fails

- **"Invalid token"**: Check that `PYPI_API_TOKEN` secret is set correctly
- **"File already exists"**: Version already published, increment version number
- **"Invalid metadata"**: Check pyproject.toml for required fields

### Manual Release (if GitHub Actions fails)

If you need to publish manually:

```bash
cd crates/arvak-python

# Install maturin
pip install maturin

# Build wheels for your platform
maturin build --release

# Publish to PyPI (requires credentials)
maturin publish --username __token__ --password "pypi-YOUR-TOKEN-HERE"
```

## Release Checklist

Before creating a release tag:

- [ ] All tests passing on main branch
- [ ] Version numbers updated in Cargo.toml and pyproject.toml
- [ ] CHANGELOG.md updated with release notes
- [ ] README.md is up to date
- [ ] `PYPI_API_TOKEN` secret configured in GitHub
- [ ] Changes committed and pushed to main

After creating release tag:

- [ ] GitHub Actions workflow completed successfully
- [ ] Package visible on https://pypi.org/project/arvak/
- [ ] Test installation from PyPI works
- [ ] Create GitHub Release with release notes

## Version Numbering

Arvak follows [Semantic Versioning](https://semver.org/):

- **Major** (1.0.0): Breaking changes
- **Minor** (1.1.0): New features, backward compatible
- **Patch** (1.0.1): Bug fixes, backward compatible

## Testing Pre-releases

To test the workflow without publishing:

1. Push to a branch (not main)
2. Manually trigger workflow via GitHub Actions UI
3. Or use Test PyPI:
   - Create token at https://test.pypi.org
   - Add as `TEST_PYPI_TOKEN` secret
   - Modify workflow to use test.pypi.org

## Support

For issues with releases:
- Check [GitHub Actions logs](https://github.com/hiq-lab/arvak/actions)
- Open an issue: https://github.com/hiq-lab/arvak/issues
- Contact: daniel@hal-contract.org
