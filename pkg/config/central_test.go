package config

import (
	"testing"
)

func TestDefault(t *testing.T) {
	cfg := Default()

	// Test Security configuration
	t.Run("Security", func(t *testing.T) {

		// Test protected paths
		expectedProtected := []string{
			".ssh/authorized_keys",
			".ssh/id_rsa",
			".ssh/id_ed25519",
			".gnupg",
			".password-store",
			".config/gh/hosts.yml",
			".aws/credentials",
			".kube/config",
			".docker/config.json",
		}

		for _, path := range expectedProtected {
			if !cfg.Security.ProtectedPaths[path] {
				t.Errorf("expected %s to be in protected paths", path)
			}
		}

		if len(cfg.Security.ProtectedPaths) != len(expectedProtected) {
			t.Errorf("expected %d protected paths, got %d", len(expectedProtected), len(cfg.Security.ProtectedPaths))
		}
	})

	// Test Patterns configuration
	t.Run("Patterns", func(t *testing.T) {
		expectedIgnore := []string{
			".git", ".svn", ".hg", "node_modules",
			".DS_Store", "*.swp", "*~", "#*#",
		}

		if len(cfg.Patterns.PackIgnore) != len(expectedIgnore) {
			t.Errorf("expected %d ignore patterns, got %d", len(expectedIgnore), len(cfg.Patterns.PackIgnore))
		}

		for i, pattern := range expectedIgnore {
			if i < len(cfg.Patterns.PackIgnore) && cfg.Patterns.PackIgnore[i] != pattern {
				t.Errorf("expected ignore pattern %d to be %s, got %s", i, pattern, cfg.Patterns.PackIgnore[i])
			}
		}

		// Test special files
		if cfg.Patterns.SpecialFiles.PackConfig != ".dodot.toml" {
			t.Errorf("expected PackConfig to be .dodot.toml, got %s", cfg.Patterns.SpecialFiles.PackConfig)
		}
		if cfg.Patterns.SpecialFiles.IgnoreFile != ".dodotignore" {
			t.Errorf("expected IgnoreFile to be .dodotignore, got %s", cfg.Patterns.SpecialFiles.IgnoreFile)
		}

		// Test catchall excludes
		expectedExcludes := []string{".dodot.toml", ".dodotignore"}
		if len(cfg.Patterns.CatchallExclude) != len(expectedExcludes) {
			t.Errorf("expected %d catchall excludes, got %d", len(expectedExcludes), len(cfg.Patterns.CatchallExclude))
		}
	})

	// Test Priorities configuration
	t.Run("Priorities", func(t *testing.T) {
		// Test trigger priorities
		if cfg.Priorities.Triggers["filename"] != 100 {
			t.Errorf("expected filename trigger priority to be 100, got %d", cfg.Priorities.Triggers["filename"])
		}
		if cfg.Priorities.Triggers["catchall"] != 0 {
			t.Errorf("expected catchall trigger priority to be 0, got %d", cfg.Priorities.Triggers["catchall"])
		}

		// Test handler priorities
		if cfg.Priorities.Handlers["symlink"] != 100 {
			t.Errorf("expected symlink handler priority to be 100, got %d", cfg.Priorities.Handlers["symlink"])
		}
		if cfg.Priorities.Handlers["path"] != 90 {
			t.Errorf("expected path handler priority to be 90, got %d", cfg.Priorities.Handlers["path"])
		}
		if cfg.Priorities.Handlers["template"] != 70 {
			t.Errorf("expected template handler priority to be 70, got %d", cfg.Priorities.Handlers["template"])
		}
	})

	// Test Matchers configuration
	t.Run("Matchers", func(t *testing.T) {
		if len(cfg.Matchers) != 9 {
			t.Errorf("expected 9 default matchers, got %d", len(cfg.Matchers))
		}

		// Test specific matchers
		for _, matcher := range cfg.Matchers {
			switch matcher.Name {
			case "install-script":
				if matcher.Priority != 90 {
					t.Errorf("expected install-script priority to be 90, got %d", matcher.Priority)
				}
				if matcher.TriggerType != "filename" {
					t.Errorf("expected install-script trigger type to be filename, got %s", matcher.TriggerType)
				}
			case "symlink-catchall":
				if matcher.Priority != 0 {
					t.Errorf("expected symlink-catchall priority to be 0, got %d", matcher.Priority)
				}
				if matcher.TriggerType != "catchall" {
					t.Errorf("expected symlink-catchall trigger type to be catchall, got %s", matcher.TriggerType)
				}
			}
		}
	})

	// Test FilePermissions configuration
	t.Run("FilePermissions", func(t *testing.T) {
		if cfg.FilePermissions.Directory != 0755 {
			t.Errorf("expected directory permissions to be 0755, got %o", cfg.FilePermissions.Directory)
		}
		if cfg.FilePermissions.File != 0644 {
			t.Errorf("expected file permissions to be 0644, got %o", cfg.FilePermissions.File)
		}
		if cfg.FilePermissions.Executable != 0755 {
			t.Errorf("expected executable permissions to be 0755, got %o", cfg.FilePermissions.Executable)
		}
	})

	// Test ShellIntegration configuration
	t.Run("ShellIntegration", func(t *testing.T) {
		expectedBashSnippet := `[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && source "$HOME/.local/share/dodot/shell/dodot-init.sh"`
		if cfg.ShellIntegration.BashZshSnippet != expectedBashSnippet {
			t.Errorf("expected bash snippet to match, got %s", cfg.ShellIntegration.BashZshSnippet)
		}

		if cfg.ShellIntegration.FishSnippet == "" {
			t.Errorf("expected fish snippet to be non-empty")
		}
	})

	// Test Paths configuration (currently empty, reserved for future use)
	t.Run("Paths", func(t *testing.T) {
		// Paths struct is intentionally empty for now
		// Internal datastore paths are in pkg/paths/paths.go
	})
}
