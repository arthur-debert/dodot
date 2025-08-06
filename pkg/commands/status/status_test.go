package status

import (
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestDisplayPackStatusAggregation tests the pack status aggregation rules
func TestDisplayPackStatusAggregation(t *testing.T) {
	tests := []struct {
		name           string
		files          []types.DisplayFile
		expectedStatus types.DisplayStatus
	}{
		{
			name:           "empty pack returns queue",
			files:          []types.DisplayFile{},
			expectedStatus: types.DisplayStatusQueue,
		},
		{
			name: "all success returns success",
			files: []types.DisplayFile{
				{Path: ".vimrc", PowerUp: "symlink", Status: types.DisplayStatusSuccess},
				{Path: ".vim/", PowerUp: "symlink", Status: types.DisplayStatusSuccess},
			},
			expectedStatus: types.DisplayStatusSuccess,
		},
		{
			name: "any error returns error",
			files: []types.DisplayFile{
				{Path: ".vimrc", PowerUp: "symlink", Status: types.DisplayStatusSuccess},
				{Path: "install.sh", PowerUp: "install", Status: types.DisplayStatusError},
			},
			expectedStatus: types.DisplayStatusError,
		},
		{
			name: "all queue returns queue",
			files: []types.DisplayFile{
				{Path: ".vimrc", PowerUp: "symlink", Status: types.DisplayStatusQueue},
				{Path: ".vim/", PowerUp: "symlink", Status: types.DisplayStatusQueue},
			},
			expectedStatus: types.DisplayStatusQueue,
		},
		{
			name: "mixed success and queue returns queue",
			files: []types.DisplayFile{
				{Path: ".vimrc", PowerUp: "symlink", Status: types.DisplayStatusSuccess},
				{Path: "install.sh", PowerUp: "install", Status: types.DisplayStatusQueue},
			},
			expectedStatus: types.DisplayStatusQueue,
		},
		{
			name: "config files are ignored in aggregation",
			files: []types.DisplayFile{
				{Path: ".dodot.toml", PowerUp: "config", Status: types.DisplayStatusConfig},
				{Path: ".vimrc", PowerUp: "symlink", Status: types.DisplayStatusSuccess},
			},
			expectedStatus: types.DisplayStatusSuccess,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			pack := &types.DisplayPack{
				Name:  "test-pack",
				Files: tt.files,
			}

			status := pack.GetPackStatus()
			assert.Equal(t, tt.expectedStatus, status)
		})
	}
}

// TestStatusPacksWithNewModel tests StatusPacks using the new display model
// This test is written for the refactored version that will use the core pipeline
func TestStatusPacksWithNewModel(t *testing.T) {
	// This test will be implemented after refactoring StatusPacks
	// to use the core pipeline and return DisplayResult

	tests := []struct {
		name           string
		opts           StatusPacksOptions
		expectedResult *types.DisplayResult
		expectedError  bool
	}{
		{
			name: "status shows files with their power-ups",
			opts: StatusPacksOptions{
				DotfilesRoot: "/test/dotfiles",
				PackNames:    []string{"vim"},
			},
			expectedResult: &types.DisplayResult{
				Command: "status",
				Packs: []types.DisplayPack{
					{
						Name:   "vim",
						Status: types.DisplayStatusQueue,
						Files: []types.DisplayFile{
							{
								Path:    ".vimrc",
								PowerUp: "symlink",
								Status:  types.DisplayStatusQueue,
								Message: "will be linked to target",
							},
							{
								Path:    "init.vim",
								PowerUp: "symlink",
								Status:  types.DisplayStatusQueue,
								Message: "will be linked to target",
							},
						},
					},
				},
			},
			expectedError: false,
		},
		{
			name: "status shows installed files with past tense",
			opts: StatusPacksOptions{
				DotfilesRoot: "/test/dotfiles",
				PackNames:    []string{"brew"},
			},
			expectedResult: &types.DisplayResult{
				Command: "status",
				Packs: []types.DisplayPack{
					{
						Name:   "brew",
						Status: types.DisplayStatusSuccess,
						Files: []types.DisplayFile{
							{
								Path:         "Brewfile",
								PowerUp:      "homebrew",
								Status:       types.DisplayStatusSuccess,
								Message:      "executed",
								LastExecuted: timePtr(time.Now().Add(-24 * time.Hour)),
							},
						},
					},
				},
			},
			expectedError: false,
		},
		{
			name: "status shows config files",
			opts: StatusPacksOptions{
				DotfilesRoot: "/test/dotfiles",
				PackNames:    []string{"configured"},
			},
			expectedResult: &types.DisplayResult{
				Command: "status",
				Packs: []types.DisplayPack{
					{
						Name:      "configured",
						Status:    types.DisplayStatusQueue,
						HasConfig: true,
						Files: []types.DisplayFile{
							{
								Path:    ".dodot.toml",
								PowerUp: "config",
								Status:  types.DisplayStatusConfig,
								Message: "dodot config file found",
							},
							{
								Path:       "*custom.sh",
								PowerUp:    "install",
								Status:     types.DisplayStatusQueue,
								Message:    "to be executed",
								IsOverride: true,
							},
						},
					},
				},
			},
			expectedError: false,
		},
	}

	// These tests will fail until StatusPacks is refactored
	// They serve as the specification for the refactoring
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Skip("Waiting for StatusPacks refactoring")

			result, err := StatusPacks(tt.opts)

			if tt.expectedError {
				require.Error(t, err)
				return
			}

			require.NoError(t, err)
			require.NotNil(t, result)

			// Compare the results
			// assert.Equal(t, tt.expectedResult.Command, result.Command)
			// assert.Len(t, result.Packs, len(tt.expectedResult.Packs))

			// Detailed pack comparison would go here
		})
	}
}

// TestConvertOperationsToDisplayFiles tests converting operations to display files
// This will be used in the refactored StatusPacks
func TestConvertOperationsToDisplayFiles(t *testing.T) {
	tests := []struct {
		name          string
		operations    []types.Operation
		expectedFiles []types.DisplayFile
	}{
		{
			name: "symlink operations become display files",
			operations: []types.Operation{
				{
					Type:    types.OperationCreateSymlink,
					Source:  "/dotfiles/vim/.vimrc",
					Target:  "/home/user/.vimrc",
					PowerUp: "symlink",
					Pack:    "vim",
					Status:  types.StatusReady,
					TriggerInfo: &types.TriggerMatchInfo{
						OriginalPath: ".vimrc",
					},
				},
			},
			expectedFiles: []types.DisplayFile{
				{
					Path:    ".vimrc",
					PowerUp: "symlink",
					Status:  types.DisplayStatusQueue,
					Message: "will be linked to target",
				},
			},
		},
		{
			name: "install operations with override",
			operations: []types.Operation{
				{
					Type:    types.OperationExecute,
					Source:  "/dotfiles/vim/install.sh",
					PowerUp: "install",
					Pack:    "vim",
					Status:  types.StatusReady,
					TriggerInfo: &types.TriggerMatchInfo{
						TriggerName:  "override-rule",
						OriginalPath: "install.sh",
					},
				},
			},
			expectedFiles: []types.DisplayFile{
				{
					Path:       "*install.sh",
					PowerUp:    "install",
					Status:     types.DisplayStatusQueue,
					Message:    "to be executed",
					IsOverride: true,
				},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// This function will be implemented as part of the refactoring
			// files := convertOperationsToDisplayFiles(tt.operations)
			// assert.Equal(t, tt.expectedFiles, files)
		})
	}
}

func timePtr(t time.Time) *time.Time {
	return &t
}
