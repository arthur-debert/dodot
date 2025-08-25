package types

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
)

func TestPackStructure(t *testing.T) {
	pack := Pack{
		Name: "test-pack",
		Path: "/path/to/pack",
		Config: config.PackConfig{
			Ignore: []config.IgnoreRule{
				{Path: "*.bak"},
			},
		},
	}

	if pack.Name != "test-pack" {
		t.Errorf("Expected pack name 'test-pack', got '%s'", pack.Name)
	}

	if pack.Path != "/path/to/pack" {
		t.Errorf("Expected pack path '/path/to/pack', got '%s'", pack.Path)
	}

	if len(pack.Config.Ignore) != 1 {
		t.Errorf("Expected 1 ignore rule, got %d", len(pack.Config.Ignore))
	}
}

func TestMatcherStructure(t *testing.T) {
	matcher := Matcher{
		Name:        "test-matcher",
		TriggerName: "filename",
		HandlerName: "symlink",
		Priority:    10,
		TriggerOptions: map[string]interface{}{
			"pattern": "*.sh",
		},
		HandlerOptions: map[string]interface{}{
			"target": "~/bin",
		},
	}

	if matcher.Name != "test-matcher" {
		t.Errorf("Expected matcher name 'test-matcher', got '%s'", matcher.Name)
	}
}

func TestTriggerMatchStructure(t *testing.T) {
	pack := Pack{Name: "test-pack", Path: "/test"}

	match := TriggerMatch{
		TriggerName:  "filename",
		Pack:         pack.Name,
		Path:         "file.txt",
		AbsolutePath: "/test/file.txt",
		Priority:     10,
		Metadata: map[string]interface{}{
			"pattern": "*.txt",
		},
		HandlerName:    "symlink",
		HandlerOptions: map[string]interface{}{},
	}

	if match.Pack != "test-pack" {
		t.Errorf("Expected pack 'test-pack', got '%s'", match.Pack)
	}

	if match.Path != "file.txt" {
		t.Errorf("Expected path 'file.txt', got '%s'", match.Path)
	}
}
