package topics

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil_old"
	"github.com/spf13/cobra"
)

func TestTopicManager_ScanTopics(t *testing.T) {
	// Create test directory structure
	tmpDir := testutil.TempDir(t, "topics-test")

	// Create topic files
	topicsDir := filepath.Join(tmpDir, "help")
	testutil.CreateDir(t, tmpDir, "help")

	// Create various topic files
	testutil.CreateFile(t, topicsDir, "dry-run.txt", "Information about dry-run mode")
	testutil.CreateFile(t, topicsDir, "architecture.md", "# Architecture\n\nSystem architecture details")
	testutil.CreateFile(t, topicsDir, "config.txxt", "Configuration Guide\n==================")
	testutil.CreateFile(t, topicsDir, "ignore.json", "This should be ignored")

	t.Run("default extensions", func(t *testing.T) {
		// Create TopicManager with default extensions (.txt, .md)
		tm := New(topicsDir)
		err := tm.scanTopics()
		testutil.AssertNoError(t, err)

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
				testutil.AssertEqual(t, tt.expected, exists)
				if exists {
					testutil.AssertEqual(t, tt.content, topic.Content)
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
		testutil.AssertNoError(t, err)

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
				testutil.AssertEqual(t, tt.expected, exists)
				if exists {
					testutil.AssertEqual(t, tt.content, topic.Content)
				}
			})
		}
	})
}

func TestTopicManager_GetTopic(t *testing.T) {
	tmpDir := testutil.TempDir(t, "topics-flags-test")
	topicsDir := filepath.Join(tmpDir, "help")
	testutil.CreateDir(t, tmpDir, "help")
	testutil.CreateFile(t, topicsDir, "option-dry-run.txt", "Dry run help")
	testutil.CreateFile(t, topicsDir, "option-verbose.txt", "Verbose help")
	testutil.CreateFile(t, topicsDir, "architecture.txt", "Architecture help")

	tm := New(topicsDir)
	err := tm.scanTopics()
	testutil.AssertNoError(t, err)

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
			testutil.AssertEqual(t, tt.exists, exists)
			if exists {
				testutil.AssertEqual(t, tt.expected, topic.Name)
			}
		})
	}
}

func TestTopicManager_ListTopics(t *testing.T) {
	tmpDir := testutil.TempDir(t, "topics-list-test")
	topicsDir := filepath.Join(tmpDir, "help")
	testutil.CreateDir(t, tmpDir, "help")

	// Create some topics
	topics := []string{"provision", "link", "dry-run", "config"}
	for _, topic := range topics {
		testutil.CreateFile(t, topicsDir, topic+".txt", "Help for "+topic)
	}

	tm := New(topicsDir)
	err := tm.scanTopics()
	testutil.AssertNoError(t, err)

	// Get list of topics
	list := tm.ListTopics()
	testutil.AssertEqual(t, len(topics), len(list))

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
	tmpDir := testutil.TempDir(t, "topics-init-test")
	topicsDir := filepath.Join(tmpDir, "help")
	testutil.CreateDir(t, tmpDir, "help")
	testutil.CreateFile(t, topicsDir, "test-topic.txt", "Test topic content")

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
	testutil.AssertNoError(t, err)

	// Check that help command exists
	helpCmd, _, err := rootCmd.Find([]string{"help"})
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, "help", helpCmd.Name())

	// Test help command with topic
	// We can't easily test the actual output in unit tests,
	// but we can verify the command structure
	testutil.AssertEqual(t, "help [command or topic]", helpCmd.Use)
}

func TestNonexistentTopicsDir(t *testing.T) {
	// Test that missing topics directory doesn't cause an error
	tm := New("/nonexistent/directory")
	err := tm.scanTopics()
	testutil.AssertNoError(t, err)

	// Should have no topics
	testutil.AssertEqual(t, 0, len(tm.ListTopics()))
}

func TestEmptyTopicsDir(t *testing.T) {
	tmpDir := testutil.TempDir(t, "topics-empty-test")
	topicsDir := filepath.Join(tmpDir, "help")
	testutil.CreateDir(t, tmpDir, "help")

	tm := New(topicsDir)
	err := tm.scanTopics()
	testutil.AssertNoError(t, err)

	// Should have no topics
	testutil.AssertEqual(t, 0, len(tm.ListTopics()))
}

func TestSubdirectoryTopics(t *testing.T) {
	tmpDir := testutil.TempDir(t, "topics-subdir-test")
	topicsDir := filepath.Join(tmpDir, "help")
	testutil.CreateDir(t, tmpDir, "help")
	testutil.CreateDir(t, topicsDir, "advanced")

	// Create topics in subdirectory
	testutil.CreateFile(t, filepath.Join(topicsDir, "advanced"), "plugins.txt", "Plugin help")

	tm := New(topicsDir)
	err := tm.scanTopics()
	testutil.AssertNoError(t, err)

	// Currently we don't handle subdirectories, so this should be found as "plugins"
	topic, exists := tm.GetTopic("plugins")
	testutil.AssertTrue(t, exists)
	testutil.AssertEqual(t, "Plugin help", topic.Content)
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
	tmpDir := testutil.TempDir(t, "topics-integration-test")
	topicsDir := filepath.Join(tmpDir, "help")
	testutil.CreateDir(t, tmpDir, "help")
	testutil.CreateFile(t, topicsDir, "dry-run.txt", "DRY RUN MODE\nThis is a test of dry run help.")

	rootCmd := &cobra.Command{
		Use:   "testapp",
		Short: "Test application",
	}

	err := Initialize(rootCmd, topicsDir)
	testutil.AssertNoError(t, err)

	// Test help for topic
	output := captureOutput(func() {
		rootCmd.SetArgs([]string{"help", "dry-run"})
		_ = rootCmd.Execute()
	})

	if !strings.Contains(output, "DRY RUN MODE") {
		t.Errorf("Expected output to contain 'DRY RUN MODE', got: %s", output)
	}
}
