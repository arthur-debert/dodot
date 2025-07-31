// Package topics provides a pluggable, topic-based help system for Cobra CLI applications.
// It extends the default Cobra help functionality to support arbitrary help topics
// loaded from files, making CLIs self-documenting.
package topics

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/spf13/cobra"
)

// TopicManager manages help topics for a Cobra application
type TopicManager struct {
	topicsDir    string
	topics       map[string]*Topic
	originalHelp func(*cobra.Command, []string)
}

// Topic represents a help topic
type Topic struct {
	Name     string
	FilePath string
	Content  string
}

// New creates a new TopicManager
func New(topicsDir string) *TopicManager {
	return &TopicManager{
		topicsDir: topicsDir,
		topics:    make(map[string]*Topic),
	}
}

// scanTopics scans the topics directory for help files
func (tm *TopicManager) scanTopics() error {
	// Check if topics directory exists
	if _, err := os.Stat(tm.topicsDir); os.IsNotExist(err) {
		// Not an error - just no topics available
		return nil
	}

	// Walk the directory to find topic files
	err := filepath.Walk(tm.topicsDir, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}

		// Skip directories
		if info.IsDir() {
			return nil
		}

		// Check if file has a supported extension
		ext := filepath.Ext(path)
		if ext != ".txt" && ext != ".txxt" && ext != ".md" {
			return nil
		}

		// Get the topic name from the filename
		basename := filepath.Base(path)
		topicName := strings.TrimSuffix(basename, ext)

		// Read the file content
		content, err := os.ReadFile(path)
		if err != nil {
			return err
		}

		// Store the topic
		tm.topics[topicName] = &Topic{
			Name:     topicName,
			FilePath: path,
			Content:  string(content),
		}

		return nil
	})

	return err
}

// GetTopic retrieves a topic by name
func (tm *TopicManager) GetTopic(name string) (*Topic, bool) {
	// Handle flag-style topics (e.g., --dry-run -> dry-run)
	name = strings.TrimPrefix(name, "--")
	name = strings.TrimPrefix(name, "-")

	topic, exists := tm.topics[name]
	return topic, exists
}

// ListTopics returns all available topic names
func (tm *TopicManager) ListTopics() []string {
	topics := make([]string, 0, len(tm.topics))
	for name := range tm.topics {
		topics = append(topics, name)
	}
	return topics
}

// Initialize sets up the topic-based help system for a Cobra application
func Initialize(rootCmd *cobra.Command, topicsDir string) error {
	tm := New(topicsDir)

	// Scan for topics
	if err := tm.scanTopics(); err != nil {
		return fmt.Errorf("failed to scan topics: %w", err)
	}

	// Store the original help function
	tm.originalHelp = rootCmd.HelpFunc()

	// Create custom help command
	helpCmd := &cobra.Command{
		Use:   "help [command or topic]",
		Short: "Help about any command or topic",
		Long: `Help provides help for any command or topic in the application.
Simply type ` + rootCmd.Name() + ` help [path to command or topic] for full details.

To see all available help topics:
  ` + rootCmd.Name() + ` help topics`,
		ValidArgsFunction: func(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
			// Combine command names and topic names for completion
			var completions []string

			// Add special keywords
			completions = append(completions, "topics")

			// Add commands
			for _, c := range rootCmd.Commands() {
				if !c.Hidden {
					completions = append(completions, c.Name())
				}
			}

			// Add topics
			completions = append(completions, tm.ListTopics()...)

			return completions, cobra.ShellCompDirectiveNoFileComp
		},
		Run: func(cmd *cobra.Command, args []string) {
			if len(args) == 0 {
				// No args - show root help
				tm.originalHelp(rootCmd, []string{})
				return
			}

			// Check if asking for topics list
			if args[0] == "topics" {
				topics := tm.ListTopics()
				if len(topics) == 0 {
					fmt.Println("No help topics available.")
				} else {
					// Sort topics alphabetically
					sort.Strings(topics)

					fmt.Println("Available help topics:")
					for _, name := range topics {
						fmt.Printf("  %s\n", name)
					}
					fmt.Println("\nUse 'dodot help <topic>' to read about a specific topic.")
				}
				return
			}

			// Check if it's a topic
			topic, exists := tm.GetTopic(args[0])
			if exists {
				fmt.Println(topic.Content)
				return
			}

			// Not a topic - fall back to original help
			tm.originalHelp(rootCmd, args)
		},
	}

	// Remove any existing help command
	for _, cmd := range rootCmd.Commands() {
		if cmd.Name() == "help" {
			rootCmd.RemoveCommand(cmd)
			break
		}
	}

	// Add our custom help command
	rootCmd.AddCommand(helpCmd)

	// Also override the help function for --help flag
	rootCmd.SetHelpFunc(func(cmd *cobra.Command, args []string) {
		// If args contain a topic, show it
		if len(args) > 0 {
			topic, exists := tm.GetTopic(args[0])
			if exists {
				fmt.Println(topic.Content)
				return
			}
		}

		// Otherwise use original help
		tm.originalHelp(cmd, args)
	})

	return nil
}
