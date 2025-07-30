package triggers

import (
	"io/fs"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// mockFileInfo implements fs.FileInfo for testing
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

func TestFileNameTrigger_ExactMatch(t *testing.T) {
	tests := []struct {
		name        string
		pattern     string
		filePath    string
		fileName    string
		shouldMatch bool
	}{
		{
			name:        "exact match - simple filename",
			pattern:     ".vimrc",
			filePath:    "/home/user/.vimrc",
			fileName:    ".vimrc",
			shouldMatch: true,
		},
		{
			name:        "exact match - with extension",
			pattern:     "config.json",
			filePath:    "/etc/app/config.json",
			fileName:    "config.json",
			shouldMatch: true,
		},
		{
			name:        "no match - different filename",
			pattern:     ".vimrc",
			filePath:    "/home/user/.bashrc",
			fileName:    ".bashrc",
			shouldMatch: false,
		},
		{
			name:        "no match - partial filename",
			pattern:     "vimrc",
			filePath:    "/home/user/.vimrc",
			fileName:    ".vimrc",
			shouldMatch: false,
		},
		{
			name:        "no match - case sensitive",
			pattern:     ".VIMRC",
			filePath:    "/home/user/.vimrc",
			fileName:    ".vimrc",
			shouldMatch: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			trigger := NewFileNameTrigger(tt.pattern)
			fileInfo := mockFileInfo{name: tt.fileName, isDir: false}

			matched, metadata := trigger.Match(tt.filePath, fileInfo)

			assert.Equal(t, tt.shouldMatch, matched)
			if matched {
				assert.NotNil(t, metadata)
				assert.Equal(t, tt.pattern, metadata["pattern"])
				assert.Equal(t, tt.fileName, metadata["filename"])
				assert.Equal(t, false, metadata["is_glob"])
			}
		})
	}
}

func TestFileNameTrigger_GlobMatch(t *testing.T) {
	tests := []struct {
		name        string
		pattern     string
		filePath    string
		fileName    string
		shouldMatch bool
	}{
		{
			name:        "glob match - wildcard suffix",
			pattern:     "*.go",
			filePath:    "/project/main.go",
			fileName:    "main.go",
			shouldMatch: true,
		},
		{
			name:        "glob match - wildcard prefix",
			pattern:     "*.config",
			filePath:    "/home/user/app.config",
			fileName:    "app.config",
			shouldMatch: true,
		},
		{
			name:        "glob match - multiple wildcards",
			pattern:     "test_*.go",
			filePath:    "/project/test_utils.go",
			fileName:    "test_utils.go",
			shouldMatch: true,
		},
		{
			name:        "glob match - question mark",
			pattern:     "file?.txt",
			filePath:    "/docs/file1.txt",
			fileName:    "file1.txt",
			shouldMatch: true,
		},
		{
			name:        "glob match - character class",
			pattern:     "file[0-9].txt",
			filePath:    "/docs/file5.txt",
			fileName:    "file5.txt",
			shouldMatch: true,
		},
		{
			name:        "no glob match - wrong extension",
			pattern:     "*.go",
			filePath:    "/project/main.py",
			fileName:    "main.py",
			shouldMatch: false,
		},
		{
			name:        "no glob match - character class",
			pattern:     "file[0-9].txt",
			filePath:    "/docs/fileA.txt",
			fileName:    "fileA.txt",
			shouldMatch: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			trigger := NewFileNameTrigger(tt.pattern)
			fileInfo := mockFileInfo{name: tt.fileName, isDir: false}

			matched, metadata := trigger.Match(tt.filePath, fileInfo)

			assert.Equal(t, tt.shouldMatch, matched)
			if matched {
				assert.NotNil(t, metadata)
				assert.Equal(t, tt.pattern, metadata["pattern"])
				assert.Equal(t, tt.fileName, metadata["filename"])
				assert.Equal(t, true, metadata["is_glob"])
			}
		})
	}
}

func TestFileNameTrigger_SkipsDirectories(t *testing.T) {
	trigger := NewFileNameTrigger("config")
	dirInfo := mockFileInfo{name: "config", isDir: true}

	matched, metadata := trigger.Match("/home/user/config", dirInfo)

	assert.False(t, matched)
	assert.Nil(t, metadata)
}

func TestFileNameTrigger_Properties(t *testing.T) {
	t.Run("exact match properties", func(t *testing.T) {
		trigger := NewFileNameTrigger(".vimrc")

		assert.Equal(t, FileNameTriggerName, trigger.Name())
		assert.Equal(t, "Matches files by exact name: .vimrc", trigger.Description())
		assert.Equal(t, FileNameTriggerPriority, trigger.Priority())
	})

	t.Run("glob match properties", func(t *testing.T) {
		trigger := NewFileNameTrigger("*.go")

		assert.Equal(t, FileNameTriggerName, trigger.Name())
		assert.Equal(t, "Matches files by glob pattern: *.go", trigger.Description())
		assert.Equal(t, FileNameTriggerPriority, trigger.Priority())
	})
}

func TestContainsGlobChars(t *testing.T) {
	tests := []struct {
		pattern string
		isGlob  bool
	}{
		{".vimrc", false},
		{"config.json", false},
		{"*.go", true},
		{"file?.txt", true},
		{"file[0-9].txt", true},
		{"file{a,b}.txt", true},
		{"normal-file.txt", false},
	}

	for _, tt := range tests {
		t.Run(tt.pattern, func(t *testing.T) {
			assert.Equal(t, tt.isGlob, containsGlobChars(tt.pattern))
		})
	}
}

func TestFileNameTrigger_InvalidGlobPattern(t *testing.T) {
	// Test with an invalid glob pattern
	trigger := NewFileNameTrigger("[")
	fileInfo := mockFileInfo{name: "test.txt", isDir: false}

	matched, metadata := trigger.Match("/path/test.txt", fileInfo)

	// Should not match and return false on error
	assert.False(t, matched)
	assert.Nil(t, metadata)
}

func TestFileNameTrigger_FactoryRegistration(t *testing.T) {
	// Test that the factory is registered
	factory, err := registry.GetTriggerFactory(FileNameTriggerName)
	require.NoError(t, err)
	require.NotNil(t, factory)

	// Test factory with pattern config
	trigger, err := factory(map[string]interface{}{
		"pattern": "*.md",
	})
	require.NoError(t, err)
	require.NotNil(t, trigger)

	assert.Equal(t, FileNameTriggerName, trigger.Name())
	assert.Equal(t, "Matches files by glob pattern: *.md", trigger.Description())

	// Test factory with default pattern
	trigger2, err := factory(map[string]interface{}{})
	require.NoError(t, err)
	require.NotNil(t, trigger2)

	assert.Equal(t, FileNameTriggerName, trigger2.Name())
	assert.Equal(t, "Matches files by glob pattern: *", trigger2.Description())
}
