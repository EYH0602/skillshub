---
name: scoop
type: link
url: https://github.com/ScoopInstaller/Scoop
description: Windows command-line installer with simple JSON manifests
---

# Scoop

A command-line installer for Windows, designed for simplicity.

## Why Reference This

- Simple JSON-based manifest format (no DSL)
- "Bucket" concept similar to our registry model
- Lightweight approach - no admin rights needed
- Clean separation between app manifests and installer logic

## Key Concepts

### Buckets (Registries)

Buckets are Git repositories containing app manifests:
- `main` bucket: curated apps
- `extras` bucket: community apps
- Users can add custom buckets

### Manifest Format

```json
{
    "version": "1.0.0",
    "description": "App description",
    "homepage": "https://...",
    "license": "MIT",
    "url": "https://download-url",
    "hash": "sha256:...",
    "bin": "app.exe",
    "shortcuts": [["app.exe", "App Name"]]
}
```

### Relevant Patterns for Skillshub

1. **Registry as Git repo** - Buckets are just Git repos with JSON files
2. **Simple manifest** - No complex DSL, just declarative JSON/YAML
3. **User-addable sources** - `scoop bucket add <name> <url>`
4. **Versioning** - Manifests can specify versions, support updates

## Key Files to Study

- `lib/manifest.ps1` - Manifest parsing
- `lib/buckets.ps1` - Bucket management
- `bucket/` directory in any bucket repo - Example manifests
