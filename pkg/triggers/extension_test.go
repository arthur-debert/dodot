package triggers

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
)

func TestExtensionTrigger_Creation(t *testing.T) {
	tests := []struct {
		name        string
		options     map[string]interface{}
		expectedExt string
		expectError bool
	}{
		{
			name: "extension with dot",
			options: map[string]interface{}{
				"extension": ".sh",
			},
			expectedExt: ".sh",
		},
		{
			name: "extension without dot",
			options: map[string]interface{}{
				"extension": "sh",
			},
			expectedExt: ".sh",
		},
		{
			name: "complex extension",
			options: map[string]interface{}{
				"extension": ".tar.gz",
			},
			expectedExt: ".tar.gz",
		},
		{
			name:        "missing extension",
			options:     map[string]interface{}{},
			expectError: true,
		},
		{
			name:        "nil options",
			options:     nil,
			expectError: true,
		},
		{
			name: "non-string extension",
			options: map[string]interface{}{
				"extension": 123,
			},
			expectError: true,
		},
		{
			name: "empty extension",
			options: map[string]interface{}{
				"extension": "",
			},
			expectError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			trigger, err := NewExtensionTrigger(tt.options)
			if tt.expectError {
				testutil.AssertError(t, err)
				testutil.AssertNil(t, trigger)
			} else {
				testutil.AssertNoError(t, err)
				testutil.AssertNotNil(t, trigger)
				testutil.AssertEqual(t, ExtensionTriggerName, trigger.Name())
				testutil.AssertEqual(t, 80, trigger.Priority())
				testutil.AssertEqual(t, tt.expectedExt, trigger.extension)
			}
		})
	}
}

func TestExtensionTrigger_Match(t *testing.T) {
	tests := []struct {
		name          string
		extension     string
		path          string
		info          mockFileInfo
		expectMatch   bool
		checkMetadata func(t *testing.T, metadata map[string]interface{})
	}{
		{
			name:        "exact match .sh",
			extension:   ".sh",
			path:        "/home/user/dotfiles/pack/script.sh",
			info:        mockFileInfo{name: "script.sh", isDir: false},
			expectMatch: true,
			checkMetadata: func(t *testing.T, metadata map[string]interface{}) {
				testutil.AssertEqual(t, ".sh", metadata["extension"])
				testutil.AssertEqual(t, "script", metadata["basename"])
			},
		},
		{
			name:        "case insensitive match",
			extension:   ".sh",
			path:        "/home/user/dotfiles/pack/script.SH",
			info:        mockFileInfo{name: "script.SH", isDir: false},
			expectMatch: true,
		},
		{
			name:        "no extension",
			extension:   ".sh",
			path:        "/home/user/dotfiles/pack/script",
			info:        mockFileInfo{name: "script", isDir: false},
			expectMatch: false,
		},
		{
			name:        "different extension",
			extension:   ".sh",
			path:        "/home/user/dotfiles/pack/script.py",
			info:        mockFileInfo{name: "script.py", isDir: false},
			expectMatch: false,
		},
		{
			name:        "directory with extension (should not match)",
			extension:   ".sh",
			path:        "/home/user/dotfiles/pack/scripts.sh",
			info:        mockFileInfo{name: "scripts.sh", isDir: true},
			expectMatch: false,
		},
		{
			name:        "double extension match",
			extension:   ".gz", // Only match the last extension since filepath.Ext returns only that
			path:        "/home/user/dotfiles/pack/archive.tar.gz",
			info:        mockFileInfo{name: "archive.tar.gz", isDir: false},
			expectMatch: true,
			checkMetadata: func(t *testing.T, metadata map[string]interface{}) {
				testutil.AssertEqual(t, ".gz", metadata["extension"]) // filepath.Ext returns only last extension
				testutil.AssertEqual(t, "archive.tar", metadata["basename"])
			},
		},
		{
			name:        "hidden file with extension",
			extension:   ".conf",
			path:        "/home/user/dotfiles/pack/.hidden.conf",
			info:        mockFileInfo{name: ".hidden.conf", isDir: false},
			expectMatch: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			trigger, err := NewExtensionTrigger(map[string]interface{}{
				"extension": tt.extension,
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

func TestExtensionTrigger_ValidateOptions(t *testing.T) {
	trigger := &ExtensionTrigger{}

	tests := []struct {
		name        string
		options     map[string]interface{}
		expectError bool
	}{
		{
			name: "valid options",
			options: map[string]interface{}{
				"extension": ".sh",
			},
		},
		{
			name:        "nil options",
			options:     nil,
			expectError: true,
		},
		{
			name:        "missing extension",
			options:     map[string]interface{}{},
			expectError: true,
		},
		{
			name: "non-string extension",
			options: map[string]interface{}{
				"extension": 123,
			},
			expectError: true,
		},
		{
			name: "unknown option",
			options: map[string]interface{}{
				"extension": ".sh",
				"unknown":   "value",
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

func TestExtensionTrigger_Description(t *testing.T) {
	trigger, err := NewExtensionTrigger(map[string]interface{}{
		"extension": ".sh",
	})
	testutil.AssertNoError(t, err)

	desc := trigger.Description()
	testutil.AssertContains(t, desc, ".sh")
	testutil.AssertContains(t, desc, "extension")
}
