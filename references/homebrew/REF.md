---
name: homebrew
type: link
url: https://github.com/Homebrew/brew
description: macOS/Linux package manager with Ruby DSL formulae
---

# Homebrew

The missing package manager for macOS (and Linux).

## Why Reference This

- Most popular package manager for macOS
- Mature "tap" (registry) system
- Well-documented formula specification
- Good model for community contributions

## Key Concepts

### Taps (Registries)

Taps are Git repositories containing formulae:
- `homebrew/core` - Default tap with curated formulae
- `homebrew/cask` - GUI applications
- Third-party taps: `user/repo` format

### Formula Format (Ruby DSL)

```ruby
class Example < Formula
  desc "Example formula"
  homepage "https://example.com"
  url "https://example.com/example-1.0.tar.gz"
  sha256 "abc123..."
  license "MIT"

  depends_on "dependency"

  def install
    bin.install "example"
  end

  test do
    system "#{bin}/example", "--version"
  end
end
```

### Relevant Patterns for Skillshub

1. **Tap system** - `brew tap user/repo` adds third-party registries
2. **Formula naming** - Lowercase, hyphenated names
3. **Versioning** - `@version` suffix for multiple versions
4. **Cask separation** - Different manifest type for different content types
5. **API endpoint** - `formulae.brew.sh` provides JSON API for discovery

## Key Files to Study

- `Library/Homebrew/formula.rb` - Formula base class
- `Library/Homebrew/tap.rb` - Tap management
- `Library/Homebrew/cmd/install.rb` - Install logic
- `Library/Homebrew/cmd/tap.rb` - Tap add/remove

## Differences from Skillshub

- Homebrew uses Ruby DSL; we use YAML frontmatter (simpler)
- Homebrew compiles/downloads binaries; we copy text files
- Homebrew has complex dependency resolution; we likely don't need it
