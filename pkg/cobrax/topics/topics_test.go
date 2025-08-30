// Package topics is part of the standalone cobrax application.
// This package is not maintained as part of dodot's core functionality.
// Tests in this file use standard library functions for file operations
// rather than dodot's testutil package, as this is appropriate for
// a standalone utility that doesn't follow dodot's testing conventions.

package topics

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/spf13/cobra"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestTopicManager_ScanTopics(t *testing.T) {
	// Create test directory structure
	tmpDir := t.TempDir()

	// Create topic files
	topicsDir := filepath.Join(tmpDir, "help")
	require.NoError(t, os.MkdirAll(topicsDir, 0755))

	// Create various topic files
	require.NoError(t, os.WriteFile(filepath.Join(topicsDir, "dry-run.txt"), []byte("Information about dry-run mode"), 0644))
	require.NoError(t, os.WriteFile(filepath.Join(topicsDir, "architecture.md"), []byte("# Architecture\n\nSystem architecture details"), 0644))
	require.NoError(t, os.WriteFile(filepath.Join(topicsDir, "config.txxt"), []byte("Configuration Guide\n=================="), 0644))
	require.NoError(t, os.WriteFile(filepath.Join(topicsDir, "ignore.json"), []byte("This should be ignored"), 0644))

	t.Run("default extensions", func(t *testing.T) {
		// Create TopicManager with default extensions (.txt, .md)
		tm := New(topicsDir)
		err := tm.scanTopics()
		require.NoError(t, err)

		// Verify only .txt and .md were loaded
		tests := []struct {
			name     string
			expected bool
			content  string
		}{
			{"dry-run", true, "Information about dry-run mode"},
			{"architecture", true, "# Architecture\n\nSystem architecture details"},
			{"config", false, ""}, // .txxt not in defaults
			{"ignore", false, ""},
		}

		for _, tt := range tests {
			t.Run(tt.name, func(t *testing.T) {
				topic, exists := tm.GetTopic(tt.name)
				assert.Equal(t, tt.expected, exists)
				if exists {
					assert.Equal(t, tt.content, topic.Content)
				}
			})
		}
	})

	t.Run("custom extensions", func(t *testing.T) {
		// Create TopicManager with custom extensions
		tm := NewWithOptions(topicsDir, Options{
			Extensions: []string{".txt", ".md", ".txxt"},
		})
		err := tm.scanTopics()
		require.NoError(t, err)

		// Verify all configured extensions were loaded
		tests := []struct {
			name     string
			expected bool
			content  string
		}{
			{"dry-run", true, "Information about dry-run mode"},
			{"architecture", true, "# Architecture\n\nSystem architecture details"},
			{"config", true, "Configuration Guide\n=================="},
			{"ignore", false, ""},
		}

		for _, tt := range tests {
			t.Run(tt.name, func(t *testing.T) {
				topic, exists := tm.GetTopic(tt.name)
				assert.Equal(t, tt.expected, exists)
				if exists {
					assert.Equal(t, tt.content, topic.Content)
				}
			})
		}
	})
}

func TestTopicManager_GetTopic(t *testing.T) {
	tmpDir := t.TempDir()
	topicsDir := filepath.Join(tmpDir, "help")
	require.NoError(t, os.MkdirAll(topicsDir, 0755))
	require.NoError(t, os.WriteFile(filepath.Join(topicsDir, "option-dry-run.txt"), []byte("Dry run help"), 0644))
	require.NoError(t, os.WriteFile(filepath.Join(topicsDir, "option-verbose.txt"), []byte("Verbose help"), 0644))
	require.NoError(t, os.WriteFile(filepath.Join(topicsDir, "architecture.txt"), []byte("Architecture help"), 0644))

	tm := New(topicsDir)
	err := tm.scanTopics()
	require.NoError(t, err)

	tests := []struct {
		input    string
		expected string
		exists   bool
	}{
		// Direct topic name
		{"architecture", "architecture", true},
		// Option topics with prefix
		{"option-dry-run", "option-dry-run", true},
		// Flag-style lookups should find option- prefixed files
		{"dry-run", "option-dry-run", true},
		{"--dry-run", "option-dry-run", true},
		{"-dry-run", "option-dry-run", true},
		{"verbose", "option-verbose", true},
		{"-v", "", false}, // Single letter flags don't match
		{"--verbose", "option-verbose", true},
		{"nonexistent", "", false},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			topic, exists := tm.GetTopic(tt.input)
			assert.Equal(t, tt.exists, exists)
			if exists {
				assert.Equal(t, tt.expected, topic.Name)
			}
		})
	}
}

func TestTopicManager_ListTopics(t *testing.T) {
	tmpDir := t.TempDir()
	topicsDir := filepath.Join(tmpDir, "help")
	require.NoError(t, os.MkdirAll(topicsDir, 0755))

	// Create some topics
	topics := []string{"provision", "link", "dry-run", "config"}
	for _, topic := range topics {
		require.NoError(t, os.WriteFile(filepath.Join(topicsDir, topic+".txt"), []byte("Help for "+topic), 0644))
	}

	tm := New(topicsDir)
	err := tm.scanTopics()
	require.NoError(t, err)

	// Get list of topics
	list := tm.ListTopics()
	assert.Equal(t, len(topics), len(list))

	// Verify all expected topics are in the list
	topicMap := make(map[string]bool)
	for _, topic := range list {
		topicMap[topic] = true
	}

	for _, expected := range topics {
		if !topicMap[expected] {
			t.Errorf("Expected topic %s not found in list", expected)
		}
	}
}

func TestInitialize(t *testing.T) {
	tmpDir := t.TempDir()
	topicsDir := filepath.Join(tmpDir, "help")
	require.NoError(t, os.MkdirAll(topicsDir, 0755))
	require.NoError(t, os.WriteFile(filepath.Join(topicsDir, "test-topic.txt"), []byte("Test topic content"), 0644))

	// Create a root command
	rootCmd := &cobra.Command{
		Use:   "testapp",
		Short: "Test application",
	}

	// Add a regular command
	rootCmd.AddCommand(&cobra.Command{
		Use:   "link",
		Short: "Deploy something",
		Run:   func(cmd *cobra.Command, args []string) {},
	})

	// Initialize topic system
	err := Initialize(rootCmd, topicsDir)
	require.NoError(t, err)

	// Check that help command exists
	helpCmd, _, err := rootCmd.Find([]string{"help"})
	require.NoError(t, err)
	assert.Equal(t, "help", helpCmd.Name())

	// Test help command with topic
	// We can't easily test the actual output in unit tests,
	// but we can verify the command structure
	assert.Equal(t, "help [command or topic]", helpCmd.Use)
}

func TestNonexistentTopicsDir(t *testing.T) {
	// Test that missing topics directory doesn't cause an error
	tm := New("/nonexistent/directory")
	err := tm.scanTopics()
	require.NoError(t, err)

	// Should have no topics
	assert.Equal(t, 0, len(tm.ListTopics()))
}

func TestEmptyTopicsDir(t *testing.T) {
	tmpDir := t.TempDir()
	topicsDir := filepath.Join(tmpDir, "help")
	require.NoError(t, os.MkdirAll(topicsDir, 0755))

	tm := New(topicsDir)
	err := tm.scanTopics()
	require.NoError(t, err)

	// Should have no topics
	assert.Equal(t, 0, len(tm.ListTopics()))
}

func TestSubdirectoryTopics(t *testing.T) {
	tmpDir := t.TempDir()
	topicsDir := filepath.Join(tmpDir, "help")
	require.NoError(t, os.MkdirAll(topicsDir, 0755))
	require.NoError(t, os.MkdirAll(filepath.Join(topicsDir, "advanced"), 0755))

	// Create topics in subdirectory
	require.NoError(t, os.WriteFile(filepath.Join(topicsDir, "advanced", "plugins.txt"), []byte("Plugin help"), 0644))

	tm := New(topicsDir)
	err := tm.scanTopics()
	require.NoError(t, err)

	// Currently we don't handle subdirectories, so this should be found as "plugins"
	topic, exists := tm.GetTopic("plugins")
	assert.True(t, exists)
	assert.Equal(t, "Plugin help", topic.Content)
}

// Integration test helper - captures output
func captureOutput(f func()) string {
	r, w, _ := os.Pipe()
	stdout := os.Stdout
	os.Stdout = w

	f()

	_ = w.Close()
	os.Stdout = stdout

	out := make([]byte, 1024)
	n, _ := r.Read(out)
	return string(out[:n])
}

func TestIntegration_HelpCommand(t *testing.T) {
	tmpDir := t.TempDir()
	topicsDir := filepath.Join(tmpDir, "help")
	require.NoError(t, os.MkdirAll(topicsDir, 0755))
	require.NoError(t, os.WriteFile(filepath.Join(topicsDir, "dry-run.txt"), []byte("DRY RUN MODE\nThis is a test of dry run help."), 0644))

	rootCmd := &cobra.Command{
		Use:   "testapp",
		Short: "Test application",
	}

	err := Initialize(rootCmd, topicsDir)
	require.NoError(t, err)

	// Test help for topic
	output := captureOutput(func() {
		rootCmd.SetArgs([]string{"help", "dry-run"})
		_ = rootCmd.Execute()
	})

	if !strings.Contains(output, "DRY RUN MODE") {
		t.Errorf("Expected output to contain 'DRY RUN MODE', got: %s", output)
	}
}
