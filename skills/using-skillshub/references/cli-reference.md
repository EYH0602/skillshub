# CLI Reference

## Adding Skills from URLs
```bash
skillshub add <github-url>                  # Add skill directly from GitHub URL
skillshub add <gist-url>                    # Add skill(s) from a GitHub Gist
```

## Skill Management
```bash
skillshub list                              # List all available skills
skillshub search <query>                    # Search skills across all taps
skillshub install <owner/repo/skill>[@commit]  # Install a skill
skillshub uninstall <owner/repo/skill>      # Remove installed skill
skillshub update [owner/repo/skill]         # Update skill(s) to latest
skillshub info <owner/repo/skill>           # Show skill details
skillshub install-all                       # Install all from all added taps
```

## Star List Import
```bash
skillshub star-list <url>                   # Add all repos from a star list as taps
skillshub star-list <url> --install         # Also install all skills from each tap
```

Requires `GITHUB_TOKEN` (GraphQL API requires authentication).

## Tap Management

The default tap is `EYH0602/skillshub` (bundled).

```bash
skillshub tap list                          # List configured taps
skillshub tap add <owner/repo>              # Add a tap (defaults to GitHub)
skillshub tap add <github-url>              # Add a tap with full URL
skillshub tap add <owner/repo> --install    # Add tap and install all skills
skillshub tap remove <owner/repo>           # Remove a tap and uninstall its skills
skillshub tap remove <owner/repo> --keep-skills  # Remove tap but keep skills installed
skillshub tap update [owner/repo]           # Refresh tap registry
skillshub tap install-all <owner/repo>      # Install all skills from a tap
```

## Agent Management
```bash
skillshub link                              # Link skills to detected agents
skillshub agents                            # Show detected agents
```

## External Skills Management
```bash
skillshub external list                     # List discovered external skills
skillshub external scan                     # Scan for external skills
skillshub external forget <name>            # Stop tracking an external skill
```

External skills are skills found in agent directories that weren't installed via skillshub (e.g., from Claude marketplace or manual installation). They are automatically discovered during `skillshub link` and synced to all agents.

## Cleanup
```bash
skillshub clean cache                       # Clear cached registry data from taps
skillshub clean links                       # Remove all skillshub-managed symlinks
skillshub clean links --remove-skills       # Remove symlinks AND delete all installed skills
skillshub clean all                         # Full uninstall: remove all skillshub state
skillshub clean all --confirm               # Skip interactive confirmation prompt
```

## Migration
```bash
skillshub migrate                           # Migrate old-style installations
```
