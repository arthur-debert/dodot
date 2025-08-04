package core

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestResolveOperationConflicts(t *testing.T) {
	tests := []struct {
		name          string
		ops           []types.Operation
		force         bool
		wantConflicts bool
		checkOps      func(t *testing.T, ops []types.Operation)
	}{
		{
			name: "no conflicts - different targets",
			ops: []types.Operation{
				{Type: types.OperationCreateSymlink, Target: "/home/user/.vimrc", Status: types.StatusReady},
				{Type: types.OperationCreateSymlink, Target: "/home/user/.bashrc", Status: types.StatusReady},
			},
			force:         false,
			wantConflicts: false,
		},
		{
			name: "conflict - multiple symlinks to same target",
			ops: []types.Operation{
				{Type: types.OperationCreateSymlink, Target: "/home/user/.gitconfig", Status: types.StatusReady},
				{Type: types.OperationCreateSymlink, Target: "/home/user/.gitconfig", Status: types.StatusReady},
			},
			force:         false,
			wantConflicts: true,
			checkOps: func(t *testing.T, ops []types.Operation) {
				assert.Equal(t, types.StatusConflict, ops[0].Status)
				assert.Equal(t, types.StatusConflict, ops[1].Status)
			},
		},
		{
			name: "conflict - multiple symlinks to same target with force",
			ops: []types.Operation{
				{Type: types.OperationCreateSymlink, Target: "/home/user/.gitconfig", Status: types.StatusReady},
				{Type: types.OperationCreateSymlink, Target: "/home/user/.gitconfig", Status: types.StatusReady},
			},
			force:         true,
			wantConflicts: false,
			checkOps: func(t *testing.T, ops []types.Operation) {
				assert.Equal(t, types.StatusReady, ops[0].Status)
				assert.Equal(t, types.StatusReady, ops[1].Status)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ctx := NewExecutionContext(tt.force)
			resolveOperationConflicts(&tt.ops, ctx)

			if tt.checkOps != nil {
				tt.checkOps(t, tt.ops)
			}

			hasConflict := false
			for _, op := range tt.ops {
				if op.Status == types.StatusConflict {
					hasConflict = true
					break
				}
			}
			assert.Equal(t, tt.wantConflicts, hasConflict)
		})
	}
}

func TestConvertActionsToOperations_WithConflictResolution(t *testing.T) {
	actions := []types.Action{
		{Type: types.ActionTypeLink, Source: "/dotfiles/vim/.vimrc", Target: "~/.vimrc"},
		{Type: types.ActionTypeWrite, Target: "~/.vimrc", Content: "\"vimrc\""},
	}

	ops, err := ConvertActionsToOperations(actions)
	require.NoError(t, err) // Should not error, but mark ops as conflict

	conflictCount := 0
	for _, op := range ops {
		if op.Status == types.StatusConflict {
			conflictCount++
		}
	}
	assert.True(t, conflictCount > 0, "Expected at least one operation to be marked as conflict")
}

func TestAreOperationsCompatible(t *testing.T) {
	tests := []struct {
		name       string
		ops        []*types.Operation
		compatible bool
	}{
		{
			name:       "multiple dir creates are compatible",
			ops:        []*types.Operation{{Type: types.OperationCreateDir}, {Type: types.OperationCreateDir}},
			compatible: true,
		},
		{
			name:       "dir create with other is incompatible",
			ops:        []*types.Operation{{Type: types.OperationCreateDir}, {Type: types.OperationCreateSymlink}},
			compatible: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := operations.AreOperationsCompatible(tt.ops)
			assert.Equal(t, tt.compatible, result)
		})
	}
}
