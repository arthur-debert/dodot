package triggers

import (
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestCatchallTrigger_Creation(t *testing.T) {
	tests := []struct {
		name           string
		options        map[string]interface{}
		wantErr        bool
		expectExcludes []string
	}{
		{
			name:    "default excludes",
			options: nil,
			wantErr: false,
			expectExcludes: []string{
				".dodot.toml",
				".dodotignore",
			},
		},
		{
			name: "additional excludes as []string",
			options: map[string]interface{}{
				"excludePatterns": []string{"*.tmp", "*.bak"},
			},
			wantErr: false,
			expectExcludes: []string{
				".dodot.toml",
				".dodotignore",
				"*.tmp",
				"*.bak",
			},
		},
		{
			name: "additional excludes as []interface{}",
			options: map[string]interface{}{
				"excludePatterns": []interface{}{"*.log", "temp*"},
			},
			wantErr: false,
			expectExcludes: []string{
				".dodot.toml",
				".dodotignore",
				"*.log",
				"temp*",
			},
		},
		{
			name:    "empty options",
			options: map[string]interface{}{},
			wantErr: false,
			expectExcludes: []string{
				".dodot.toml",
				".dodotignore",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			trigger, err := NewCatchallTrigger(tt.options)

			if tt.wantErr {
				testutil.AssertError(t, err)
				return
			}

			testutil.AssertNoError(t, err)
			testutil.AssertNotNil(t, trigger)
			testutil.AssertEqual(t, CatchallTriggerName, trigger.Name())
			testutil.AssertEqual(t, CatchallTriggerPriority, trigger.Priority())
			testutil.AssertEqual(t, types.TriggerTypeCatchall, trigger.Type())
			testutil.AssertSliceEqual(t, tt.expectExcludes, trigger.excludePatterns)
		})
	}
}

func TestCatchallTrigger_Match(t *testing.T) {
	// Create a catchall trigger with custom excludes for testing
	trigger, err := NewCatchallTrigger(map[string]interface{}{
		"excludePatterns": []string{"*.tmp", "test-*"},
	})
	testutil.AssertNoError(t, err)

	tests := []struct {
		name        string
		path        string
		isDir       bool
		shouldMatch bool
	}{
		// Default excludes
		{
			name:        "exclude .dodot.toml",
			path:        "/home/user/dotfiles/pack/.dodot.toml",
			isDir:       false,
			shouldMatch: false,
		},
		{
			name:        "exclude .dodotignore",
			path:        "/home/user/dotfiles/pack/.dodotignore",
			isDir:       false,
			shouldMatch: false,
		},
		// Custom excludes
		{
			name:        "exclude .tmp file",
			path:        "/home/user/dotfiles/pack/cache.tmp",
			isDir:       false,
			shouldMatch: false,
		},
		{
			name:        "exclude test- prefix",
			path:        "/home/user/dotfiles/pack/test-file.txt",
			isDir:       false,
			shouldMatch: false,
		},
		// Should match
		{
			name:        "match regular file",
			path:        "/home/user/dotfiles/pack/.vimrc",
			isDir:       false,
			shouldMatch: true,
		},
		{
			name:        "match config file",
			path:        "/home/user/dotfiles/pack/.config/nvim/init.vim",
			isDir:       false,
			shouldMatch: true,
		},
		{
			name:        "match directory",
			path:        "/home/user/dotfiles/pack/bin",
			isDir:       true,
			shouldMatch: true,
		},
		{
			name:        "match hidden file",
			path:        "/home/user/dotfiles/pack/.gitignore",
			isDir:       false,
			shouldMatch: true,
		},
		{
			name:        "match file with spaces",
			path:        "/home/user/dotfiles/pack/my file.txt",
			isDir:       false,
			shouldMatch: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			info := &mockFileInfo{
				name:    tt.path,
				isDir:   tt.isDir,
				mode:    0644,
				modTime: time.Now(),
			}

			matched, metadata := trigger.Match(tt.path, info)

			testutil.AssertEqual(t, tt.shouldMatch, matched)

			if matched {
				testutil.AssertNotNil(t, metadata)
				testutil.AssertEqual(t, tt.isDir, metadata["isDir"])
				testutil.AssertNotNil(t, metadata["basename"])
			}
		})
	}
}

func TestCatchallTrigger_Description(t *testing.T) {
	trigger, err := NewCatchallTrigger(nil)
	testutil.AssertNoError(t, err)

	desc := trigger.Description()
	testutil.AssertContains(t, desc, "all files")
	testutil.AssertContains(t, desc, "not matched")
}

func TestCatchallTrigger_FactoryRegistration(t *testing.T) {
	// Test that the factory is registered
	factory, err := registry.GetTriggerFactory(CatchallTriggerName)
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, factory)

	// Test creating a trigger through the factory
	trigger, err := factory(map[string]interface{}{
		"excludePatterns": []string{"test.txt"},
	})
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, trigger)
	testutil.AssertEqual(t, CatchallTriggerName, trigger.Name())
	testutil.AssertEqual(t, types.TriggerTypeCatchall, trigger.Type())
}
