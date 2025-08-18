package paths

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestValidatePackName(t *testing.T) {
	tests := []struct {
		name    string
		input   string
		wantErr bool
	}{
		// Valid cases
		{
			name:    "simple name",
			input:   "vim",
			wantErr: false,
		},
		{
			name:    "name with dash",
			input:   "vim-config",
			wantErr: false,
		},
		{
			name:    "name with underscore",
			input:   "vim_config",
			wantErr: false,
		},
		{
			name:    "name with numbers",
			input:   "vim2",
			wantErr: false,
		},
		{
			name:    "name with mixed case",
			input:   "VimConfig",
			wantErr: false,
		},
		// Invalid cases
		{
			name:    "empty name",
			input:   "",
			wantErr: true,
		},
		{
			name:    "name with forward slash",
			input:   "vim/config",
			wantErr: true,
		},
		{
			name:    "name with backslash",
			input:   "vim\\config",
			wantErr: true,
		},
		{
			name:    "name with colon",
			input:   "vim:config",
			wantErr: true,
		},
		{
			name:    "name with asterisk",
			input:   "vim*",
			wantErr: true,
		},
		{
			name:    "name with question mark",
			input:   "vim?",
			wantErr: true,
		},
		{
			name:    "name with double quote",
			input:   "vim\"config",
			wantErr: true,
		},
		{
			name:    "name with less than",
			input:   "vim<config",
			wantErr: true,
		},
		{
			name:    "name with greater than",
			input:   "vim>config",
			wantErr: true,
		},
		{
			name:    "name with pipe",
			input:   "vim|config",
			wantErr: true,
		},
		{
			name:    "dot only",
			input:   ".",
			wantErr: true,
		},
		{
			name:    "double dot",
			input:   "..",
			wantErr: true,
		},
		{
			name:    "name with null byte",
			input:   "vim\x00config",
			wantErr: true,
		},
		{
			name:    "name with control character",
			input:   "vim\x01config",
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidatePackName(tt.input)
			if tt.wantErr {
				assert.Error(t, err, "ValidatePackName(%q) should return error", tt.input)
			} else {
				assert.NoError(t, err, "ValidatePackName(%q) should not return error", tt.input)
			}
		})
	}
}
