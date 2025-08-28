package core

import (
	"fmt"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestFilterConfigurationActions(t *testing.T) {
	tests := []struct {
		name          string
		actions       []types.Action
		expectedCount int
		expectedTypes []string
	}{
		{
			name: "filter configuration actions",
			actions: []types.Action{
				&types.LinkAction{PackName: "test", SourceFile: "src", TargetFile: "target"},
				&types.AddToPathAction{PackName: "test", DirPath: "/path"},
				&types.RunScriptAction{PackName: "test", ScriptPath: "script.sh"},
			},
			expectedCount: 2,
			expectedTypes: []string{"*types.LinkAction", "*types.AddToPathAction"},
		},
		{
			name: "all provisioning actions",
			actions: []types.Action{
				&types.RunScriptAction{PackName: "test", ScriptPath: "script.sh"},
				&types.BrewAction{PackName: "test", BrewfilePath: "Brewfile"},
			},
			expectedCount: 0,
			expectedTypes: []string{},
		},
		{
			name:          "empty actions",
			actions:       []types.Action{},
			expectedCount: 0,
			expectedTypes: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			filtered := FilterConfigurationActions(tt.actions)

			assert.Equal(t, tt.expectedCount, len(filtered))

			// Check that we got the expected types
			for i, action := range filtered {
				if i < len(tt.expectedTypes) {
					assert.Equal(t, tt.expectedTypes[i], typeString(action))
				}
			}
		})
	}
}

func TestFilterCodeExecutionActions(t *testing.T) {
	tests := []struct {
		name          string
		actions       []types.Action
		expectedCount int
		expectedTypes []string
	}{
		{
			name: "filter code execution actions",
			actions: []types.Action{
				&types.LinkAction{PackName: "test", SourceFile: "src", TargetFile: "target"},
				&types.RunScriptAction{PackName: "test", ScriptPath: "script.sh"},
				&types.BrewAction{PackName: "test", BrewfilePath: "Brewfile"},
			},
			expectedCount: 2,
			expectedTypes: []string{"*types.RunScriptAction", "*types.BrewAction"},
		},
		{
			name: "all configuration actions",
			actions: []types.Action{
				&types.LinkAction{PackName: "test", SourceFile: "src", TargetFile: "target"},
				&types.AddToPathAction{PackName: "test", DirPath: "/path"},
			},
			expectedCount: 0,
			expectedTypes: []string{},
		},
		{
			name:          "empty actions",
			actions:       []types.Action{},
			expectedCount: 0,
			expectedTypes: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			filtered := FilterCodeExecutionActions(tt.actions)

			assert.Equal(t, tt.expectedCount, len(filtered))

			// Check that we got the expected types
			for i, action := range filtered {
				if i < len(tt.expectedTypes) {
					assert.Equal(t, tt.expectedTypes[i], typeString(action))
				}
			}
		})
	}
}

func TestFilterProvisioningActions(t *testing.T) {
	// Mock data store
	store := &mockDataStore{
		needsProvisioning: map[string]bool{
			"test-script.sh.sentinel":     true,
			"test-homebrew-test.sentinel": false,
		},
	}

	actions := []types.Action{
		&types.LinkAction{PackName: "test", SourceFile: "src", TargetFile: "target"},
		&types.RunScriptAction{
			PackName:     "test",
			ScriptPath:   "script.sh",
			Checksum:     "abc123",
			SentinelName: "script.sh.sentinel",
		},
		&types.BrewAction{
			PackName:     "test",
			BrewfilePath: "Brewfile",
			Checksum:     "def456",
		},
	}

	t.Run("force mode includes all actions", func(t *testing.T) {
		filtered, err := FilterProvisioningActions(actions, true, store)
		assert.NoError(t, err)
		assert.Equal(t, 3, len(filtered))
	})

	t.Run("non-force mode filters based on provisioning status", func(t *testing.T) {
		filtered, err := FilterProvisioningActions(actions, false, store)
		assert.NoError(t, err)
		assert.Equal(t, 2, len(filtered)) // LinkAction and RunScriptAction (needs provisioning)
	})
}

// Helper function to get type string
func typeString(v interface{}) string {
	return fmt.Sprintf("%T", v)
}

// Mock data store for testing
type mockDataStore struct {
	needsProvisioning map[string]bool
}

func (m *mockDataStore) Link(pack, sourceFile string) (string, error) {
	return "", nil
}

func (m *mockDataStore) Unlink(pack, sourceFile string) error {
	return nil
}

func (m *mockDataStore) AddToPath(pack, dirPath string) error {
	return nil
}

func (m *mockDataStore) AddToShellProfile(pack, scriptPath string) error {
	return nil
}

func (m *mockDataStore) RecordProvisioning(pack, sentinelName, checksum string) error {
	return nil
}

func (m *mockDataStore) NeedsProvisioning(pack, sentinelName, checksum string) (bool, error) {
	key := fmt.Sprintf("%s-%s", pack, sentinelName)
	if needs, ok := m.needsProvisioning[key]; ok {
		return needs, nil
	}
	return true, nil
}

func (m *mockDataStore) GetStatus(pack, sourceFile string) (types.Status, error) {
	return types.Status{}, nil
}

func (m *mockDataStore) GetSymlinkStatus(pack, sourceFile string) (types.Status, error) {
	return types.Status{}, nil
}

func (m *mockDataStore) GetPathStatus(pack, dirPath string) (types.Status, error) {
	return types.Status{}, nil
}

func (m *mockDataStore) GetShellProfileStatus(pack, scriptPath string) (types.Status, error) {
	return types.Status{}, nil
}

func (m *mockDataStore) GetProvisioningStatus(pack, sentinelName, currentChecksum string) (types.Status, error) {
	return types.Status{}, nil
}

func (m *mockDataStore) GetBrewStatus(pack, brewfilePath, currentChecksum string) (types.Status, error) {
	return types.Status{}, nil
}

func (m *mockDataStore) DeleteProvisioningState(packName, handlerName string) error {
	return nil
}

func (m *mockDataStore) GetProvisioningHandlers(packName string) ([]string, error) {
	return []string{}, nil
}

func (m *mockDataStore) ListProvisioningState(packName string) (map[string][]string, error) {
	return map[string][]string{}, nil
}
