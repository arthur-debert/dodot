package triggers

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
)

func TestPathPatternTrigger_Creation(t *testing.T) {
	tests := []struct {
		name        string
		options     map[string]interface{}
		expectError bool
	}{
		{
			name: "simple pattern",
			options: map[string]interface{}{
				"pattern": "*.conf",
			},
		},
		{
			name: "path pattern",
			options: map[string]interface{}{
				"pattern": "config/*.yaml",
			},
		},
		{
			name: "complex pattern",
			options: map[string]interface{}{
				"pattern": "**/test/*.go",
			},
		},
		{
			name:        "missing pattern",
			options:     map[string]interface{}{},
			expectError: true,
		},
		{
			name:        "nil options",
			options:     nil,
			expectError: true,
		},
		{
			name: "non-string pattern",
			options: map[string]interface{}{
				"pattern": 123,
			},
			expectError: true,
		},
		{
			name: "empty pattern",
			options: map[string]interface{}{
				"pattern": "",
			},
			expectError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			trigger, err := NewPathPatternTrigger(tt.options)
			if tt.expectError {
				testutil.AssertError(t, err)
				testutil.AssertNil(t, trigger)
			} else {
				testutil.AssertNoError(t, err)
				testutil.AssertNotNil(t, trigger)
				testutil.AssertEqual(t, PathPatternTriggerName, trigger.Name())
				testutil.AssertEqual(t, 70, trigger.Priority())
			}
		})
	}
}

func TestPathPatternTrigger_Match(t *testing.T) {
	tests := []struct {
		name          string
		pattern       string
		path          string
		info          mockFileInfo
		expectMatch   bool
		checkMetadata func(t *testing.T, metadata map[string]interface{})
	}{
		{
			name:        "simple file match",
			pattern:     "*.conf",
			path:        "app.conf",
			info:        mockFileInfo{name: "app.conf", isDir: false},
			expectMatch: true,
			checkMetadata: func(t *testing.T, metadata map[string]interface{}) {
				testutil.AssertEqual(t, "*.conf", metadata["pattern"])
				testutil.AssertEqual(t, "app.conf", metadata["fullPath"])
				testutil.AssertEqual(t, false, metadata["isDir"])
			},
		},
		{
			name:        "directory path match",
			pattern:     "config/*",
			path:        "config/settings.yaml",
			info:        mockFileInfo{name: "settings.yaml", isDir: false},
			expectMatch: true,
		},
		{
			name:        "exact path match",
			pattern:     ".github/workflows/ci.yml",
			path:        ".github/workflows/ci.yml",
			info:        mockFileInfo{name: "ci.yml", isDir: false},
			expectMatch: true,
		},
		{
			name:        "no match",
			pattern:     "*.conf",
			path:        "script.sh",
			info:        mockFileInfo{name: "script.sh", isDir: false},
			expectMatch: false,
		},
		{
			name:        "directory match",
			pattern:     "bin",
			path:        "bin",
			info:        mockFileInfo{name: "bin", isDir: true},
			expectMatch: true,
			checkMetadata: func(t *testing.T, metadata map[string]interface{}) {
				testutil.AssertEqual(t, true, metadata["isDir"])
			},
		},
		{
			name:        "nested pattern match",
			pattern:     "*/bin/*",
			path:        "pack1/bin/tool",
			info:        mockFileInfo{name: "tool", isDir: false},
			expectMatch: true,
		},
		{
			name:        "pattern with extension",
			pattern:     "scripts/*.sh",
			path:        "scripts/install.sh",
			info:        mockFileInfo{name: "install.sh", isDir: false},
			expectMatch: true,
		},
		{
			name:        "hidden file pattern",
			pattern:     ".*",
			path:        ".gitignore",
			info:        mockFileInfo{name: ".gitignore", isDir: false},
			expectMatch: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			trigger, err := NewPathPatternTrigger(map[string]interface{}{
				"pattern": tt.pattern,
			})
			testutil.AssertNoError(t, err)

			matched, metadata := trigger.Match(tt.path, tt.info)
			testutil.AssertEqual(t, tt.expectMatch, matched)

			if tt.expectMatch && tt.checkMetadata != nil {
				testutil.AssertNotNil(t, metadata)
				tt.checkMetadata(t, metadata)
			}
		})
	}
}

func TestPathPatternTrigger_ValidateOptions(t *testing.T) {
	trigger := &PathPatternTrigger{}

	tests := []struct {
		name        string
		options     map[string]interface{}
		expectError bool
	}{
		{
			name: "valid options",
			options: map[string]interface{}{
				"pattern": "*.conf",
			},
		},
		{
			name:        "nil options",
			options:     nil,
			expectError: true,
		},
		{
			name:        "missing pattern",
			options:     map[string]interface{}{},
			expectError: true,
		},
		{
			name: "non-string pattern",
			options: map[string]interface{}{
				"pattern": 123,
			},
			expectError: true,
		},
		{
			name: "unknown option",
			options: map[string]interface{}{
				"pattern": "*.conf",
				"unknown": "value",
			},
			expectError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := trigger.ValidateOptions(tt.options)
			if tt.expectError {
				testutil.AssertError(t, err)
			} else {
				testutil.AssertNoError(t, err)
			}
		})
	}
}

func TestPathPatternTrigger_Description(t *testing.T) {
	trigger, err := NewPathPatternTrigger(map[string]interface{}{
		"pattern": "config/*.yaml",
	})
	testutil.AssertNoError(t, err)

	desc := trigger.Description()
	testutil.AssertContains(t, desc, "config/*.yaml")
	testutil.AssertContains(t, desc, "pattern")
}
