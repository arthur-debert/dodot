package execution

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestExecuteOperations(t *testing.T) {
	tests := []struct {
		name    string
		opts    ExecuteOperationsOptions
		wantNil bool
		wantErr bool
	}{
		{
			name: "dry_run_returns_nil",
			opts: ExecuteOperationsOptions{
				Operations: []types.Operation{{Type: types.OperationCreateSymlink}},
				DryRun:     true,
			},
			wantNil: true,
			wantErr: false,
		},
		{
			name: "no_operations_returns_nil",
			opts: ExecuteOperationsOptions{
				Operations: []types.Operation{},
				DryRun:     false,
			},
			wantNil: true,
			wantErr: false,
		},
		{
			name: "uses_combined_executor_when_specified",
			opts: ExecuteOperationsOptions{
				Operations: []types.Operation{
					{
						Type:   types.OperationCreateSymlink,
						Source: "/tmp/test-source",
						Target: "/tmp/test-target",
					},
				},
				DryRun:              false,
				EnableHomeSymlinks:  true,
				UseCombinedExecutor: true,
			},
			wantNil: false,
			wantErr: false,
		},
		{
			name: "uses_synthfs_executor_when_not_combined",
			opts: ExecuteOperationsOptions{
				Operations: []types.Operation{
					{
						Type:    types.OperationWriteFile,
						Target:  "/tmp/test-file",
						Content: "test content",
					},
				},
				DryRun:              false,
				UseCombinedExecutor: false,
			},
			wantNil: false,
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := ExecuteOperations(tt.opts)

			if tt.wantErr {
				require.Error(t, err)
			} else {
				require.NoError(t, err)
			}

			if tt.wantNil {
				assert.Nil(t, got)
			} else {
				assert.NotNil(t, got)
			}
		})
	}
}
