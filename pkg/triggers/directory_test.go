package triggers

import (
	"io/fs"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/testutil"
)

// mockDirInfo implements fs.FileInfo for testing directories
type mockDirInfo struct {
	name  string
	isDir bool
}

func (m mockDirInfo) Name() string       { return m.name }
func (m mockDirInfo) Size() int64        { return 0 }
func (m mockDirInfo) Mode() fs.FileMode  { return fs.ModeDir | 0755 }
func (m mockDirInfo) ModTime() time.Time { return time.Now() }
func (m mockDirInfo) IsDir() bool        { return m.isDir }
func (m mockDirInfo) Sys() interface{}   { return nil }

func TestDirectoryTrigger_Creation(t *testing.T) {
	tests := []struct {
		name        string
		options     map[string]interface{}
		expectError bool
	}{
		{
			name: "valid pattern",
			options: map[string]interface{}{
				"pattern": "bin",
			},
		},
		{
			name: "wildcard pattern",
			options: map[string]interface{}{
				"pattern": ".*",
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
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			trigger, err := NewDirectoryTrigger(tt.options)
			if tt.expectError {
				testutil.AssertError(t, err)
				testutil.AssertNil(t, trigger)
			} else {
				testutil.AssertNoError(t, err)
				testutil.AssertNotNil(t, trigger)
				testutil.AssertEqual(t, DirectoryTriggerName, trigger.Name())
				testutil.AssertEqual(t, 100, trigger.Priority())
			}
		})
	}
}

func TestDirectoryTrigger_Match(t *testing.T) {
	tests := []struct {
		name          string
		pattern       string
		path          string
		info          fs.FileInfo
		expectMatch   bool
		checkMetadata func(t *testing.T, metadata map[string]interface{})
	}{
		{
			name:        "exact match directory",
			pattern:     "bin",
			path:        "/home/user/dotfiles/pack/bin",
			info:        mockDirInfo{name: "bin", isDir: true},
			expectMatch: true,
			checkMetadata: func(t *testing.T, metadata map[string]interface{}) {
				testutil.AssertEqual(t, "bin", metadata["directory"])
				testutil.AssertEqual(t, "bin", metadata["pattern"])
			},
		},
		{
			name:        "exact match file (should not match)",
			pattern:     "bin",
			path:        "/home/user/dotfiles/pack/bin",
			info:        mockDirInfo{name: "bin", isDir: false},
			expectMatch: false,
		},
		{
			name:        "non-matching directory",
			pattern:     "bin",
			path:        "/home/user/dotfiles/pack/config",
			info:        mockDirInfo{name: "config", isDir: true},
			expectMatch: false,
		},
		{
			name:        "wildcard match",
			pattern:     ".*",
			path:        "/home/user/dotfiles/pack/.config",
			info:        mockDirInfo{name: ".config", isDir: true},
			expectMatch: true,
		},
		{
			name:        "glob pattern match",
			pattern:     "test*",
			path:        "/home/user/dotfiles/pack/test-dir",
			info:        mockDirInfo{name: "test-dir", isDir: true},
			expectMatch: true,
		},
		{
			name:        "nested directory",
			pattern:     "bin",
			path:        "/home/user/dotfiles/pack/subdir/bin",
			info:        mockDirInfo{name: "bin", isDir: true},
			expectMatch: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			trigger, err := NewDirectoryTrigger(map[string]interface{}{
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

func TestDirectoryTrigger_ValidateOptions(t *testing.T) {
	trigger := &DirectoryTrigger{}

	tests := []struct {
		name        string
		options     map[string]interface{}
		expectError bool
	}{
		{
			name: "valid options",
			options: map[string]interface{}{
				"pattern": "bin",
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
				"pattern": "bin",
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

func TestDirectoryTrigger_Description(t *testing.T) {
	trigger, err := NewDirectoryTrigger(map[string]interface{}{
		"pattern": "bin",
	})
	testutil.AssertNoError(t, err)

	desc := trigger.Description()
	testutil.AssertContains(t, desc, "bin")
	testutil.AssertContains(t, desc, "directories")
}
