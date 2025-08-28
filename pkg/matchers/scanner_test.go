package matchers

import (
	"io/fs"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/triggers"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Simple tests that verify the scanner logic without needing to override DefaultMatchers

// MockTrigger implements the Trigger interface for testing
type MockTrigger struct {
	shouldMatch bool
	metadata    map[string]interface{}
	triggerType types.TriggerType
}

func (m *MockTrigger) Name() string {
	return "mock-trigger"
}

func (m *MockTrigger) Description() string {
	return "Mock trigger for testing"
}

func (m *MockTrigger) Match(path string, info fs.FileInfo) (bool, map[string]interface{}) {
	return m.shouldMatch, m.metadata
}

func (m *MockTrigger) Priority() int {
	return 50
}

func (m *MockTrigger) Type() types.TriggerType {
	return m.triggerType
}

func TestScanPack_OverrideRules(t *testing.T) {
	// Clear trigger factory registry
	triggerFactoryRegistry := registry.GetRegistry[types.TriggerFactory]()
	triggerFactoryRegistry.Clear()

	fs := newMockFS()
	fs.addFile("/test/pack", "override.sh", false)

	pack := types.Pack{
		Name: "test",
		Path: "/test/pack",
		Config: config.PackConfig{
			Override: []config.OverrideRule{{
				Path:    "*.sh",
				Handler: "custom-handler",
				With:    map[string]interface{}{"key": "value"},
			}},
		},
	}

	matches, err := ScanPack(pack, fs)
	require.NoError(t, err)

	// Should have at least the override match
	overrideFound := false
	for _, match := range matches {
		if match.TriggerName == "config-override" && match.HandlerName == "custom-handler" {
			overrideFound = true
			assert.Equal(t, map[string]interface{}{"key": "value"}, match.HandlerOptions)
			break
		}
	}
	assert.True(t, overrideFound, "should create match for override rule")
}

func TestScanPack_IgnoredFiles(t *testing.T) {
	// Clear trigger factory registry
	triggerFactoryRegistry := registry.GetRegistry[types.TriggerFactory]()
	triggerFactoryRegistry.Clear()

	fs := newMockFS()
	fs.addFile("/test/pack", "ignored.tmp", false)
	fs.addFile("/test/pack", "normal.txt", false)

	pack := types.Pack{
		Name: "test",
		Path: "/test/pack",
		Config: config.PackConfig{
			Ignore: []config.IgnoreRule{{Path: "*.tmp"}},
		},
	}

	matches, err := ScanPack(pack, fs)
	require.NoError(t, err)

	// Should not have any matches for ignored.tmp
	for _, match := range matches {
		assert.NotEqual(t, "ignored.tmp", match.Path, "ignored files should not match")
	}
}

func TestScanPack_SpecialFiles(t *testing.T) {
	// Clear trigger factory registry
	triggerFactoryRegistry := registry.GetRegistry[types.TriggerFactory]()
	triggerFactoryRegistry.Clear()

	fs := newMockFS()
	fs.addFile("/test/pack", ".dodot.yaml", false)
	fs.addFile("/test/pack", "normal.txt", false)

	pack := types.Pack{
		Name: "test",
		Path: "/test/pack",
	}

	matches, err := ScanPack(pack, fs)
	require.NoError(t, err)

	// Should not have any matches for .dodot.yaml
	for _, match := range matches {
		assert.NotEqual(t, ".dodot.yaml", match.Path, "special files should not match")
	}
}

func TestTestMatcher(t *testing.T) {
	// Clear trigger factory registry
	triggerFactoryRegistry := registry.GetRegistry[types.TriggerFactory]()
	triggerFactoryRegistry.Clear()

	err := registry.RegisterTriggerFactory("test-trigger", func(options map[string]interface{}) (types.Trigger, error) {
		return &MockTrigger{
			shouldMatch: true,
			metadata:    map[string]interface{}{"key": "value"},
		}, nil
	})
	require.NoError(t, err)

	matcher := types.Matcher{
		Name:           "test",
		TriggerName:    "test-trigger",
		HandlerName:    "test-handler",
		Priority:       50,
		HandlerOptions: map[string]interface{}{"option": "test"},
	}

	pack := types.Pack{Name: "test-pack"}
	info := &mockFileInfo{name: "test.txt", mode: 0644}

	match, err := testMatcher(pack, "/abs/path", "test.txt", info, matcher)
	require.NoError(t, err)
	require.NotNil(t, match)

	assert.Equal(t, "test-trigger", match.TriggerName)
	assert.Equal(t, "test-pack", match.Pack)
	assert.Equal(t, "test.txt", match.Path)
	assert.Equal(t, "/abs/path", match.AbsolutePath)
	assert.Equal(t, "test-handler", match.HandlerName)
	assert.Equal(t, 50, match.Priority)
	assert.Equal(t, map[string]interface{}{"key": "value"}, match.Metadata)
	assert.Equal(t, map[string]interface{}{"option": "test"}, match.HandlerOptions)
}

func TestScanPackWithMatchersAndOverrides(t *testing.T) {
	// Manually register triggers needed for the test
	registry.GetRegistry[types.TriggerFactory]().Clear()
	require.NoError(t, registry.RegisterTriggerFactory("filename", func(o map[string]interface{}) (types.Trigger, error) {
		pattern, _ := o["pattern"].(string)
		return triggers.NewFileNameTrigger(pattern), nil
	}))
	require.NoError(t, registry.RegisterTriggerFactory("directory", func(o map[string]interface{}) (types.Trigger, error) { return triggers.NewDirectoryTrigger(o) }))
	require.NoError(t, registry.RegisterTriggerFactory("catchall", func(o map[string]interface{}) (types.Trigger, error) { return triggers.NewCatchallTrigger(o) }))

	// 1. Define mock filesystem
	fs := newMockFS()
	fs.addFile("/packs/pack1", "bin", true)
	fs.addFile("/packs/pack1", "install.sh", false)
	fs.addFile("/packs/pack1", "aliases.sh", false)
	fs.addFile("/packs/pack1", "ignored.log", false)
	fs.addFile("/packs/pack1", "catchall-file.txt", false)

	// 2. Create base config with mappings
	baseConfig := config.Default()
	baseConfig.Mappings = config.Mappings{
		Path:     "bin",
		Install:  "install.sh",
		Shell:    []string{"aliases.sh"},
		Homebrew: "Brewfile",
	}
	baseMatchers := ConvertConfigMatchers(baseConfig.Matchers)

	mappingMatchersConfig := baseConfig.GenerateMatchersFromMapping()
	mappingMatchers := ConvertConfigMatchers(mappingMatchersConfig)

	allMatchers := append(baseMatchers, mappingMatchers...)

	// 3. Create pack with an overriding config
	pack := types.Pack{
		Name: "pack1",
		Path: "/packs/pack1",
		Config: config.PackConfig{
			Ignore: []config.IgnoreRule{{Path: "*.log"}},
		},
	}

	// 4. Scan the pack
	matches, err := ScanPackWithMatchers(pack, fs, allMatchers)
	require.NoError(t, err)

	// 5. Assertions
	expectedMatches := map[string]string{
		"bin":               "path",
		"install.sh":        "install",
		"aliases.sh":        "shell",
		"catchall-file.txt": "symlink", // From default catch-all
	}
	assert.Len(t, matches, len(expectedMatches), "should have the correct number of matches")

	for _, match := range matches {
		// Check that the ignored file is not in the matches
		assert.NotEqual(t, "ignored.log", match.Path)

		expectedHandler, ok := expectedMatches[match.Path]
		assert.True(t, ok, "unexpected match for path: %s", match.Path)
		assert.Equal(t, expectedHandler, match.HandlerName, "incorrect handler for path: %s", match.Path)
	}
}

// Helper types from the original test file

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

type mockFS struct {
	files map[string][]fs.DirEntry
	dirs  map[string]bool
}

func newMockFS() *mockFS {
	return &mockFS{
		files: make(map[string][]fs.DirEntry),
		dirs:  make(map[string]bool),
	}
}

func (m *mockFS) addFile(dir, name string, isDir bool) {
	entry := &mockDirEntry{
		name:  name,
		isDir: isDir,
		info: &mockFileInfo{
			name:  name,
			isDir: isDir,
			mode:  0644,
		},
	}
	m.files[dir] = append(m.files[dir], entry)
}

func (m *mockFS) ReadDir(path string) ([]fs.DirEntry, error) {
	entries, ok := m.files[path]
	if !ok {
		return nil, fs.ErrNotExist
	}
	return entries, nil
}

func (m *mockFS) Stat(path string) (fs.FileInfo, error) {
	if _, ok := m.dirs[path]; ok {
		return &mockFileInfo{name: path, isDir: true}, nil
	}
	return nil, fs.ErrNotExist
}

func (m *mockFS) ReadFile(path string) ([]byte, error) {
	return nil, fs.ErrNotExist
}

func (m *mockFS) WriteFile(path string, data []byte, perm fs.FileMode) error {
	return nil
}

func (m *mockFS) MkdirAll(path string, perm fs.FileMode) error {
	m.dirs[path] = true
	return nil
}

func (m *mockFS) Remove(path string) error {
	return nil
}

func (m *mockFS) RemoveAll(path string) error {
	delete(m.dirs, path)
	return nil
}

func (m *mockFS) Lstat(path string) (fs.FileInfo, error) {
	return m.Stat(path)
}

func (m *mockFS) Symlink(target, link string) error {
	return nil
}

func (m *mockFS) Readlink(path string) (string, error) {
	return "", fs.ErrNotExist
}

type mockDirEntry struct {
	name  string
	isDir bool
	info  fs.FileInfo
}

func (m *mockDirEntry) Name() string               { return m.name }
func (m *mockDirEntry) IsDir() bool                { return m.isDir }
func (m *mockDirEntry) Type() fs.FileMode          { return m.info.Mode() }
func (m *mockDirEntry) Info() (fs.FileInfo, error) { return m.info, nil }
