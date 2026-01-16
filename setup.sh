#!/bin/bash

# Setup script for linking skills to AI coding agents

SKILLS_DIR="$HOME/.skills"
AGENTS=(".claude" ".codex" ".opencode")

if [ ! -d "$SKILLS_DIR" ]; then
    echo "Error: Skills directory not found at $SKILLS_DIR"
    echo "Please clone this repository to ~/.skills first"
    exit 1
fi

for agent in "${AGENTS[@]}"; do
    agent_dir="$HOME/$agent"
    link_path="$agent_dir/.skills"

    if [ -d "$agent_dir" ]; then
        if [ -L "$link_path" ]; then
            echo "[$agent] Link already exists, skipping"
        elif [ -e "$link_path" ]; then
            echo "[$agent] Warning: .skills exists but is not a symlink, skipping"
        else
            ln -s "$SKILLS_DIR" "$link_path"
            echo "[$agent] Created symlink"
        fi
    else
        echo "[$agent] Not found, skipping"
    fi
done

echo "Done!"
