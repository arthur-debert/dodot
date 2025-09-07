package config

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestTransformUserToInternal(t *testing.T) {
	tests := []struct {
		name     string
		input    map[string]interface{}
		expected map[string]interface{}
	}{
		{
			name: "pack_ignore_transformation",
			input: map[string]interface{}{
				"pack": map[string]interface{}{
					"ignore": []interface{}{"test-*", "backup", "*.tmp"},
				},
			},
			expected: map[string]interface{}{
				"patterns": map[string]interface{}{
					"pack_ignore": []interface{}{"test-*", "backup", "*.tmp"},
				},
			},
		},
		{
			name: "symlink_protected_paths_transformation",
			input: map[string]interface{}{
				"symlink": map[string]interface{}{
					"protected_paths": []interface{}{".ssh/id_rsa", ".gnupg"},
				},
			},
			expected: map[string]interface{}{
				"security": map[string]interface{}{
					"protected_paths": map[string]bool{
						".ssh/id_rsa": true,
						".gnupg":      true,
					},
				},
			},
		},
		{
			name: "symlink_force_home_transformation",
			input: map[string]interface{}{
				"symlink": map[string]interface{}{
					"force_home": []interface{}{"ssh", "aws", "bashrc"},
				},
			},
			expected: map[string]interface{}{
				"link_paths": map[string]interface{}{
					"force_home": map[string]bool{
						"ssh":    true,
						"aws":    true,
						"bashrc": true,
					},
				},
			},
		},
		{
			name: "mappings_passthrough",
			input: map[string]interface{}{
				"mappings": map[string]interface{}{
					"path":     "bin",
					"install":  "install.sh",
					"shell":    []interface{}{"aliases.sh", "profile.sh"},
					"homebrew": "Brewfile",
				},
			},
			expected: map[string]interface{}{
				"mappings": map[string]interface{}{
					"path":     "bin",
					"install":  "install.sh",
					"shell":    []interface{}{"aliases.sh", "profile.sh"},
					"homebrew": "Brewfile",
				},
			},
		},
		{
			name: "combined_transformation",
			input: map[string]interface{}{
				"pack": map[string]interface{}{
					"ignore": []interface{}{"custom-ignore"},
				},
				"symlink": map[string]interface{}{
					"force_home":      []interface{}{"custom"},
					"protected_paths": []interface{}{".custom/secret"},
				},
				"mappings": map[string]interface{}{
					"path": "scripts",
				},
			},
			expected: map[string]interface{}{
				"patterns": map[string]interface{}{
					"pack_ignore": []interface{}{"custom-ignore"},
				},
				"link_paths": map[string]interface{}{
					"force_home": map[string]bool{"custom": true},
				},
				"security": map[string]interface{}{
					"protected_paths": map[string]bool{".custom/secret": true},
				},
				"mappings": map[string]interface{}{
					"path": "scripts",
				},
			},
		},
		{
			name: "flattened_input",
			input: map[string]interface{}{
				"pack.ignore":             []interface{}{"test"},
				"symlink.force_home":      []interface{}{"vim"},
				"symlink.protected_paths": []interface{}{".vim/secret"},
			},
			expected: map[string]interface{}{
				"patterns": map[string]interface{}{
					"pack_ignore": []interface{}{"test"},
				},
				"link_paths": map[string]interface{}{
					"force_home": map[string]bool{"vim": true},
				},
				"security": map[string]interface{}{
					"protected_paths": map[string]bool{".vim/secret": true},
				},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := transformUserToInternal(tt.input)
			assert.Equal(t, tt.expected, result)
		})
	}
}
