#!/bin/bash
# Setup script for large scale test - creates 10+ packs with mixed handlers

DOTFILES_DIR="$1"
if [ -z "$DOTFILES_DIR" ]; then
    echo "Usage: $0 <dotfiles-directory>"
    exit 1
fi

# Create 12 different packs with various handler combinations
for i in {1..12}; do
    PACK_DIR="$DOTFILES_DIR/pack-$i"
    mkdir -p "$PACK_DIR"
    
    case $i in
        1|2|3)
            # Packs 1-3: Simple symlink packs
            echo "# Config for pack-$i" > "$PACK_DIR/config-$i"
            echo "pack_id=$i" >> "$PACK_DIR/config-$i"
            echo "type=symlink" >> "$PACK_DIR/config-$i"
            ;;
        
        4|5)
            # Packs 4-5: Path handler packs
            mkdir -p "$PACK_DIR/bin"
            cat > "$PACK_DIR/bin/tool-$i" << EOF
#!/bin/bash
echo "Tool $i from pack-$i"
EOF
            chmod +x "$PACK_DIR/bin/tool-$i"
            ;;
        
        6|7)
            # Packs 6-7: Shell profile packs
            cat > "$PACK_DIR/profile.sh" << EOF
# Profile for pack-$i
export PACK_${i}_LOADED=1
alias p${i}="echo 'Pack $i loaded'"
EOF
            ;;
        
        8|9)
            # Packs 8-9: Template packs
            cat > "$PACK_DIR/config.tmpl" << EOF
# Template config for pack-$i
pack_home={{ .HOME }}
pack_user={{ .USER }}
pack_id=$i
EOF
            ;;
        
        10)
            # Pack 10: Install script with homebrew
            cat > "$PACK_DIR/install.sh" << EOF
#!/bin/bash
echo "Installing pack-$i..." >&2
mkdir -p "\$HOME/.local/pack-$i"
echo "installed" > "\$HOME/.local/pack-$i/marker.txt"
EOF
            chmod +x "$PACK_DIR/install.sh"
            
            echo "brew 'jq'" > "$PACK_DIR/Brewfile"
            ;;
        
        11)
            # Pack 11: Mixed symlink + path
            echo "# Mixed pack-$i config" > "$PACK_DIR/settings"
            mkdir -p "$PACK_DIR/bin"
            echo '#!/bin/bash' > "$PACK_DIR/bin/mixed-tool"
            echo 'echo "Mixed tool from pack-11"' >> "$PACK_DIR/bin/mixed-tool"
            chmod +x "$PACK_DIR/bin/mixed-tool"
            ;;
        
        12)
            # Pack 12: Everything pack (all handlers)
            echo "# Complete pack-$i" > "$PACK_DIR/complete-config"
            mkdir -p "$PACK_DIR/bin"
            echo '#!/bin/bash' > "$PACK_DIR/bin/complete-tool"
            echo 'echo "Complete tool"' >> "$PACK_DIR/bin/complete-tool"
            chmod +x "$PACK_DIR/bin/complete-tool"
            
            cat > "$PACK_DIR/profile.sh" << EOF
export COMPLETE_PACK_LOADED=1
alias complete="echo 'Complete pack loaded'"
EOF
            
            cat > "$PACK_DIR/data.tmpl" << EOF
complete_home={{ .HOME }}
complete_pack=12
EOF
            
            cat > "$PACK_DIR/install.sh" << EOF
#!/bin/bash
echo "Complete pack installing..." >&2
mkdir -p "\$HOME/.local/pack-12"
date > "\$HOME/.local/pack-12/install-time.txt"
EOF
            chmod +x "$PACK_DIR/install.sh"
            ;;
    esac
done

echo "Created 12 test packs in $DOTFILES_DIR"