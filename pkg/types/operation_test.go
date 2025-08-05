package types

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestOperationWithContext(t *testing.T) {
	tests := []struct {
		name     string
		op       *Operation
		validate func(t *testing.T, op *Operation)
	}{
		{
			name: "operation with full context",
			op: &Operation{
				Type:        OperationCreateSymlink,
				Source:      "/source/file",
				Target:      "/target/file",
				Description: "Link config file",
				Status:      StatusReady,
				Pack:        "vim",
				PowerUp:     "symlink",
				TriggerInfo: &TriggerMatchInfo{
					TriggerName:  "FileName",
					OriginalPath: ".vimrc",
					Priority:     10,
				},
				Metadata: map[string]interface{}{
					"app":      "vim",
					"category": "editor",
				},
				GroupID: "vim-config-123",
			},
			validate: func(t *testing.T, op *Operation) {
				assert.Equal(t, "vim", op.Pack)
				assert.Equal(t, "symlink", op.PowerUp)
				assert.NotNil(t, op.TriggerInfo)
				assert.Equal(t, "FileName", op.TriggerInfo.TriggerName)
				assert.Equal(t, ".vimrc", op.TriggerInfo.OriginalPath)
				assert.Equal(t, 10, op.TriggerInfo.Priority)
				assert.Equal(t, "vim", op.Metadata["app"])
				assert.Equal(t, "editor", op.Metadata["category"])
				assert.Equal(t, "vim-config-123", op.GroupID)
			},
		},
		{
			name: "operation without trigger info",
			op: &Operation{
				Type:        OperationWriteFile,
				Target:      "/target/file",
				Content:     "content",
				Description: "Write config",
				Status:      StatusReady,
				Pack:        "bash",
				PowerUp:     "profile",
			},
			validate: func(t *testing.T, op *Operation) {
				assert.Equal(t, "bash", op.Pack)
				assert.Equal(t, "profile", op.PowerUp)
				assert.Nil(t, op.TriggerInfo)
				assert.Nil(t, op.Metadata)
				assert.Empty(t, op.GroupID)
			},
		},
		{
			name: "execute operation with context",
			op: &Operation{
				Type:        OperationExecute,
				Command:     "brew",
				Args:        []string{"install", "vim"},
				Description: "Install vim via homebrew",
				Status:      StatusReady,
				Pack:        "homebrew",
				PowerUp:     "homebrew",
				Metadata: map[string]interface{}{
					"formula": "vim",
					"options": []string{"--with-lua"},
				},
			},
			validate: func(t *testing.T, op *Operation) {
				assert.Equal(t, "homebrew", op.Pack)
				assert.Equal(t, "homebrew", op.PowerUp)
				assert.Equal(t, "vim", op.Metadata["formula"])
				options := op.Metadata["options"].([]string)
				assert.Contains(t, options, "--with-lua")
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tt.validate(t, tt.op)
		})
	}
}

func TestTriggerMatchInfo(t *testing.T) {
	info := &TriggerMatchInfo{
		TriggerName:  "DirectoryTrigger",
		OriginalPath: "config/nvim",
		Priority:     20,
	}

	assert.Equal(t, "DirectoryTrigger", info.TriggerName)
	assert.Equal(t, "config/nvim", info.OriginalPath)
	assert.Equal(t, 20, info.Priority)
}

func TestOperationGrouping(t *testing.T) {
	// Test that operations can be grouped by GroupID
	ops := []*Operation{
		{
			Type:    OperationCreateSymlink,
			Pack:    "vim",
			PowerUp: "symlink",
			GroupID: "vim-config",
		},
		{
			Type:    OperationCreateDir,
			Pack:    "vim",
			PowerUp: "symlink",
			GroupID: "vim-config",
		},
		{
			Type:    OperationWriteFile,
			Pack:    "bash",
			PowerUp: "profile",
			GroupID: "bash-profile",
		},
	}

	// Group operations
	groups := make(map[string][]*Operation)
	for _, op := range ops {
		groups[op.GroupID] = append(groups[op.GroupID], op)
	}

	assert.Len(t, groups["vim-config"], 2)
	assert.Len(t, groups["bash-profile"], 1)
	assert.Equal(t, "vim", groups["vim-config"][0].Pack)
	assert.Equal(t, "bash", groups["bash-profile"][0].Pack)
}

func TestOperationMetadataPreservation(t *testing.T) {
	// Test that metadata from actions is preserved
	op := &Operation{
		Type:   OperationCreateSymlink,
		Source: "/source",
		Target: "/target",
		Metadata: map[string]interface{}{
			"originalTrigger": "FileName",
			"appName":         "vim",
			"priority":        10,
			"customData": map[string]string{
				"category": "editor",
				"version":  "9.0",
			},
		},
	}

	assert.Equal(t, "FileName", op.Metadata["originalTrigger"])
	assert.Equal(t, "vim", op.Metadata["appName"])
	assert.Equal(t, 10, op.Metadata["priority"])

	customData := op.Metadata["customData"].(map[string]string)
	assert.Equal(t, "editor", customData["category"])
	assert.Equal(t, "9.0", customData["version"])
}
