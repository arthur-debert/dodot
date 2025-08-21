package types

import (
	"io/fs"
	"testing"
	"time"
)

// Mock implementations for testing interfaces

type mockTrigger struct {
	name        string
	description string
	priority    int
	shouldMatch bool
	metadata    map[string]interface{}
}

func (m *mockTrigger) Name() string        { return m.name }
func (m *mockTrigger) Description() string { return m.description }
func (m *mockTrigger) Priority() int       { return m.priority }
func (m *mockTrigger) Match(path string, info fs.FileInfo) (bool, map[string]interface{}) {
	return m.shouldMatch, m.metadata
}
func (m *mockTrigger) Type() TriggerType { return TriggerTypeSpecific }

type mockHandler struct {
	name        string
	description string
	actions     []Action
	err         error
}

func (m *mockHandler) Name() string        { return m.name }
func (m *mockHandler) Description() string { return m.description }
func (m *mockHandler) RunMode() RunMode    { return RunModeMany }
func (m *mockHandler) Process(matches []TriggerMatch) ([]Action, error) {
	return m.actions, m.err
}
func (m *mockHandler) ValidateOptions(options map[string]interface{}) error {
	return nil
}

type mockFileInfo struct {
	name    string
	size    int64
	mode    fs.FileMode
	modTime time.Time
	isDir   bool
}

func (m mockFileInfo) Name() string       { return m.name }
func (m mockFileInfo) Size() int64        { return m.size }
func (m mockFileInfo) Mode() fs.FileMode  { return m.mode }
func (m mockFileInfo) ModTime() time.Time { return m.modTime }
func (m mockFileInfo) IsDir() bool        { return m.isDir }
func (m mockFileInfo) Sys() interface{}   { return nil }

func TestTriggerInterface(t *testing.T) {
	trigger := &mockTrigger{
		name:        "test-trigger",
		description: "A test trigger",
		priority:    10,
		shouldMatch: true,
		metadata:    map[string]interface{}{"key": "value"},
	}

	// Test interface methods
	if trigger.Name() != "test-trigger" {
		t.Errorf("Name() = %s, want %s", trigger.Name(), "test-trigger")
	}

	if trigger.Description() != "A test trigger" {
		t.Errorf("Description() = %s, want %s", trigger.Description(), "A test trigger")
	}

	if trigger.Priority() != 10 {
		t.Errorf("Priority() = %d, want %d", trigger.Priority(), 10)
	}

	// Test Match
	info := mockFileInfo{name: "test.txt", isDir: false}
	matched, metadata := trigger.Match("test.txt", info)
	if !matched {
		t.Error("Match() should return true")
	}
	if metadata["key"] != "value" {
		t.Errorf("Match() metadata = %v, want key=value", metadata)
	}
}

func TestHandlerInterface(t *testing.T) {
	actions := []Action{
		{
			Type:        ActionTypeLink,
			Description: "Link file",
			Source:      "/source",
			Target:      "/target",
		},
	}

	powerUp := &mockHandler{
		name:        "test-handler",
		description: "A test power-up",
		actions:     actions,
		err:         nil,
	}

	// Test interface methods
	if powerUp.Name() != "test-handler" {
		t.Errorf("Name() = %s, want %s", powerUp.Name(), "test-handler")
	}

	if powerUp.Description() != "A test power-up" {
		t.Errorf("Description() = %s, want %s", powerUp.Description(), "A test power-up")
	}

	// Test Process
	matches := []TriggerMatch{
		{
			TriggerName: "test-trigger",
			Path:        "test.txt",
		},
	}

	resultActions, err := powerUp.Process(matches)
	if err != nil {
		t.Fatalf("Process() error = %v", err)
	}

	if len(resultActions) != 1 {
		t.Fatalf("Process() returned %d actions, want 1", len(resultActions))
	}

	if resultActions[0].Type != ActionTypeLink {
		t.Errorf("Action type = %s, want %s", resultActions[0].Type, ActionTypeLink)
	}
}

func TestActionTypes(t *testing.T) {
	// Test that action type constants are defined correctly
	actionTypes := []ActionType{
		ActionTypeLink,
		ActionTypeCopy,
		ActionTypeWrite,
		ActionTypeAppend,
		ActionTypeMkdir,
		ActionTypeShellSource,
		ActionTypePathAdd,
		ActionTypeRun,
	}

	expectedValues := []string{
		"link",
		"copy",
		"write",
		"append",
		"mkdir",
		"shell_source",
		"path_add",
		"run",
	}

	for i, at := range actionTypes {
		if string(at) != expectedValues[i] {
			t.Errorf("ActionType[%d] = %s, want %s", i, at, expectedValues[i])
		}
	}
}

func TestPackStructure(t *testing.T) {
	pack := Pack{
		Name: "test-pack",
		Path: "/path/to/pack",
		Config: PackConfig{
			Ignore: []IgnoreRule{
				{Path: "*.bak"},
			},
			Override: []OverrideRule{
				{Path: "test.conf", Handler: "symlink"},
			},
		},
		Metadata: map[string]interface{}{
			"author": "test",
		},
	}

	// Verify pack fields
	if pack.Name != "test-pack" {
		t.Errorf("Pack.Name = %s, want test-pack", pack.Name)
	}

	if len(pack.Config.Ignore) != 1 {
		t.Errorf("len(pack.Config.Ignore) = %d, want 1", len(pack.Config.Ignore))
	}

	if len(pack.Config.Override) != 1 {
		t.Errorf("len(pack.Config.Override) = %d, want 1", len(pack.Config.Override))
	}

	// Test IsIgnored
	if !pack.Config.IsIgnored("backup.bak") {
		t.Error("IsIgnored(backup.bak) should be true")
	}
	if pack.Config.IsIgnored("test.conf") {
		t.Error("IsIgnored(test.conf) should be false")
	}

	// Test FindOverride
	if override := pack.Config.FindOverride("test.conf"); override == nil {
		t.Error("FindOverride(test.conf) should not be nil")
	} else if override.Handler != "symlink" {
		t.Errorf("FindOverride(test.conf).Handler = %s, want symlink", override.Handler)
	}

	if override := pack.Config.FindOverride("other.file"); override != nil {
		t.Error("FindOverride(other.file) should be nil")
	}
}

func TestMatcherStructure(t *testing.T) {
	matcher := Matcher{
		Name:        "test-matcher",
		TriggerName: "filename",
		HandlerName: "symlink",
		Priority:    10,
		Options:     map[string]interface{}{"global": true},
		TriggerOptions: map[string]interface{}{
			"pattern": "*.conf",
		},
		HandlerOptions: map[string]interface{}{
			"target": "$HOME/.config",
		},
		Enabled: true,
	}

	// Verify matcher fields
	if matcher.TriggerName != "filename" {
		t.Errorf("Matcher.TriggerName = %s, want filename", matcher.TriggerName)
	}

	if matcher.HandlerName != "symlink" {
		t.Errorf("Matcher.HandlerName = %s, want symlink", matcher.HandlerName)
	}

	if matcher.TriggerOptions["pattern"] != "*.conf" {
		t.Errorf("Matcher.TriggerOptions[pattern] = %v, want *.conf", matcher.TriggerOptions["pattern"])
	}
}

func TestTriggerMatchStructure(t *testing.T) {
	pack := Pack{Name: "test-pack", Path: "/test"}

	match := TriggerMatch{
		TriggerName:  "filename",
		Pack:         pack.Name,
		Path:         "config.conf",
		AbsolutePath: "/test/config.conf",
		Metadata: map[string]interface{}{
			"extension": ".conf",
		},
		HandlerName: "symlink",
		HandlerOptions: map[string]interface{}{
			"target": "$HOME/.config",
		},
		Priority: 1,
	}

	// Verify fields
	if match.Pack != "test-pack" {
		t.Errorf("TriggerMatch.Pack = %s, want test-pack", match.Pack)
	}

	if match.Metadata["extension"] != ".conf" {
		t.Errorf("TriggerMatch.Metadata[extension] = %v, want .conf", match.Metadata["extension"])
	}
}
