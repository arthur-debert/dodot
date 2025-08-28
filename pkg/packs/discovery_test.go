// Test Type: Unit Test
// Description: Tests for the packs package - pack discovery functions

package packs_test

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/packs"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGetPackCandidatesFS(t *testing.T) {
	tests := []struct {
		name          string
		setup         func(env *testutil.TestEnvironment)
		expectedPacks []string
		expectError   bool
		errorCode     errors.ErrorCode
	}{
		{
			name: "finds_valid_pack_directories",
			setup: func(env *testutil.TestEnvironment) {
				// Create valid pack directories
				require.NoError(t, env.FS.MkdirAll(filepath.Join(env.DotfilesRoot, "vim"), 0755))
				require.NoError(t, env.FS.MkdirAll(filepath.Join(env.DotfilesRoot, "bash"), 0755))
				require.NoError(t, env.FS.MkdirAll(filepath.Join(env.DotfilesRoot, "git"), 0755))

				// Create a file (should be ignored)
				require.NoError(t, env.FS.WriteFile(
					filepath.Join(env.DotfilesRoot, "README.md"),
					[]byte("# Dotfiles"),
					0644,
				))
			},
			expectedPacks: []string{"bash", "git", "vim"},
		},
		{
			name: "ignores_hidden_directories_except_config",
			setup: func(env *testutil.TestEnvironment) {
				require.NoError(t, env.FS.MkdirAll(filepath.Join(env.DotfilesRoot, ".hidden"), 0755))
				require.NoError(t, env.FS.MkdirAll(filepath.Join(env.DotfilesRoot, ".config"), 0755))
				require.NoError(t, env.FS.MkdirAll(filepath.Join(env.DotfilesRoot, "visible"), 0755))
			},
			expectedPacks: []string{".config", "visible"},
		},
		{
			name: "returns_empty_for_empty_directory",
			setup: func(env *testutil.TestEnvironment) {
				// Directory exists but is empty
			},
			expectedPacks: []string{},
		},
		{
			name: "error_when_root_does_not_exist",
			setup: func(env *testutil.TestEnvironment) {
				// Remove the dotfiles root
				env.DotfilesRoot = "/nonexistent"
			},
			expectError: true,
			errorCode:   errors.ErrNotFound,
		},
		{
			name: "error_when_root_is_file",
			setup: func(env *testutil.TestEnvironment) {
				// Create a file at dotfiles root instead of directory
				require.NoError(t, env.FS.WriteFile(env.DotfilesRoot, []byte("not a dir"), 0644))
			},
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
			defer env.Cleanup()

			tt.setup(env)

			candidates, err := packs.GetPackCandidatesFS(env.DotfilesRoot, env.FS)

			if tt.expectError {
				assert.Error(t, err)
				if terr, ok := err.(*errors.DodotError); ok {
					assert.Equal(t, tt.errorCode, terr.Code)
				}
			} else {
				assert.NoError(t, err)
				// Extract just the base names for comparison
				baseNames := make([]string, len(candidates))
				for i, candidate := range candidates {
					baseNames[i] = filepath.Base(candidate)
				}
				assert.Equal(t, tt.expectedPacks, baseNames)
			}
		})
	}
}

func TestGetPacksFS(t *testing.T) {
	tests := []struct {
		name          string
		candidates    []string
		setup         func(env *testutil.TestEnvironment)
		expectedCount int
		expectedPacks []string
	}{
		{
			name: "loads_valid_packs",
			candidates: []string{
				"/dotfiles/vim",
				"/dotfiles/bash",
			},
			setup: func(env *testutil.TestEnvironment) {
				// Create pack directories
				require.NoError(t, env.FS.MkdirAll("/dotfiles/vim", 0755))
				require.NoError(t, env.FS.MkdirAll("/dotfiles/bash", 0755))

				// Add some content
				require.NoError(t, env.FS.WriteFile("/dotfiles/vim/.vimrc", []byte("set number"), 0644))
				require.NoError(t, env.FS.WriteFile("/dotfiles/bash/.bashrc", []byte("export PS1"), 0644))
			},
			expectedCount: 2,
			expectedPacks: []string{"bash", "vim"},
		},
		{
			name: "loads_all_valid_packs_including_numeric_start",
			candidates: []string{
				"/dotfiles/vim",
				"/dotfiles/123invalid",
				"/dotfiles/bash",
			},
			setup: func(env *testutil.TestEnvironment) {
				require.NoError(t, env.FS.MkdirAll("/dotfiles/vim", 0755))
				require.NoError(t, env.FS.MkdirAll("/dotfiles/123invalid", 0755))
				require.NoError(t, env.FS.MkdirAll("/dotfiles/bash", 0755))
			},
			expectedCount: 3,
			expectedPacks: []string{"123invalid", "bash", "vim"},
		},
		{
			name: "loads_pack_with_config",
			candidates: []string{
				"/dotfiles/vim",
			},
			setup: func(env *testutil.TestEnvironment) {
				require.NoError(t, env.FS.MkdirAll("/dotfiles/vim", 0755))

				// Create a pack config file
				configContent := `# vim pack config
[mappings]
vimrc = ".vimrc"
`
				require.NoError(t, env.FS.WriteFile(
					"/dotfiles/vim/.dodot.toml",
					[]byte(configContent),
					0644,
				))
			},
			expectedCount: 1,
			expectedPacks: []string{"vim"},
		},
		{
			name: "skips_non_directory_candidates",
			candidates: []string{
				"/dotfiles/vim",
				"/dotfiles/file.txt",
			},
			setup: func(env *testutil.TestEnvironment) {
				require.NoError(t, env.FS.MkdirAll("/dotfiles/vim", 0755))
				require.NoError(t, env.FS.WriteFile("/dotfiles/file.txt", []byte("not a dir"), 0644))
			},
			expectedCount: 1,
			expectedPacks: []string{"vim"},
		},
		{
			name:          "empty_candidates_returns_empty",
			candidates:    []string{},
			setup:         func(env *testutil.TestEnvironment) {},
			expectedCount: 0,
			expectedPacks: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
			defer env.Cleanup()

			tt.setup(env)

			packs, err := packs.GetPacksFS(tt.candidates, env.FS)
			assert.NoError(t, err)
			assert.Len(t, packs, tt.expectedCount)

			// Check pack names
			packNames := make([]string, len(packs))
			for i, pack := range packs {
				packNames[i] = pack.Name
			}
			assert.Equal(t, tt.expectedPacks, packNames)
		})
	}
}

func TestValidatePack(t *testing.T) {
	t.Skip("ValidatePack uses os.Stat directly - needs refactoring to support FS abstraction")
}

func TestGetPackCandidates_Deprecated(t *testing.T) {
	t.Skip("GetPackCandidates is deprecated and uses os.Stat directly")
}

func TestGetPacks_Deprecated(t *testing.T) {
	t.Skip("GetPacks is deprecated and uses os.Stat directly")
}
