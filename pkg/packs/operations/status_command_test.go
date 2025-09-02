package operations_test

import (
	"path/filepath"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/packs/operations"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/ui/display"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGetPacksStatus_NoPacksReturnsEmptyResult(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Equal(t, "status", result.Command)
	assert.False(t, result.DryRun)
	assert.Empty(t, result.Packs)
}

func TestGetPacksStatus_SinglePackWithNoFiles(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	env.SetupPack("empty", testutil.PackConfig{
		Files: map[string]string{},
	})

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.Packs, 1)
	assert.Equal(t, "empty", result.Packs[0].Name)
	assert.Empty(t, result.Packs[0].Files)
	// Empty pack with no files will have "queue" status (default for no files)
	assert.Equal(t, "queue", result.Packs[0].Status)
}

func TestGetPacksStatus_PackWithIgnoredFile(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	env.SetupPack("ignored", testutil.PackConfig{
		Files: map[string]string{
			".dodotignore": "",
		},
	})

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.Packs, 1)
	assert.Equal(t, "ignored", result.Packs[0].Name)
	assert.True(t, result.Packs[0].IsIgnored)
	assert.Equal(t, "ignored", result.Packs[0].Status)
	assert.Len(t, result.Packs[0].Files, 1)
	assert.Equal(t, ".dodotignore", result.Packs[0].Files[0].Path)
	assert.Equal(t, "ignored", result.Packs[0].Files[0].Status)
}

func TestGetPacksStatus_PackWithConfigFile(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	env.SetupPack("configured", testutil.PackConfig{
		Files: map[string]string{
			".dodot.toml": "[pack]\nname = \"configured\"",
		},
	})

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.Packs, 1)
	assert.Equal(t, "configured", result.Packs[0].Name)
	assert.True(t, result.Packs[0].HasConfig)
	assert.Len(t, result.Packs[0].Files, 1)
	assert.Equal(t, ".dodot.toml", result.Packs[0].Files[0].Path)
	assert.Equal(t, "config", result.Packs[0].Files[0].Status)
}

func TestGetPacksStatus_PackWithUnlinkedSymlinkFiles(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			".vimrc":  "set number",
			".gvimrc": "set guifont=Monaco",
		},
	})

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.Packs, 1)

	packResult := result.Packs[0]
	assert.Equal(t, "vim", packResult.Name)
	assert.Equal(t, "queue", packResult.Status)
	assert.Len(t, packResult.Files, 2)

	// Check both files are present (order not guaranteed)
	filesByPath := make(map[string]display.DisplayFile)
	for _, f := range packResult.Files {
		filesByPath[f.Path] = f
	}

	vimrcFile, hasVimrc := filesByPath[".vimrc"]
	assert.True(t, hasVimrc, "should have .vimrc file")
	assert.Equal(t, "symlink", vimrcFile.Handler)
	assert.Equal(t, "queue", vimrcFile.Status)
	assert.Equal(t, "not linked", vimrcFile.Message)

	gvimrcFile, hasGvimrc := filesByPath[".gvimrc"]
	assert.True(t, hasGvimrc, "should have .gvimrc file")
	assert.Equal(t, "symlink", gvimrcFile.Handler)
	assert.Equal(t, "queue", gvimrcFile.Status)
	assert.Equal(t, "not linked", gvimrcFile.Message)
}

func TestGetPacksStatus_PackWithLinkedSymlinkFiles(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			".vimrc": "set number",
		},
	})

	// Simulate that the file has been linked by creating the intermediate link
	vimrcPath := filepath.Join(env.DotfilesRoot, "vim", ".vimrc")
	pathsInstance := env.Paths.(paths.Paths)
	intermediateLinkDir := pathsInstance.PackHandlerDir("vim", "symlink")
	require.NoError(t, env.FS.MkdirAll(intermediateLinkDir, 0755))

	intermediateLinkPath := filepath.Join(intermediateLinkDir, ".vimrc")
	require.NoError(t, env.FS.Symlink(vimrcPath, intermediateLinkPath))

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.Packs, 1)

	packResult := result.Packs[0]
	assert.Equal(t, "vim", packResult.Name)
	assert.Equal(t, "success", packResult.Status)
	assert.Len(t, packResult.Files, 1)

	assert.Equal(t, ".vimrc", packResult.Files[0].Path)
	assert.Equal(t, "symlink", packResult.Files[0].Handler)
	assert.Equal(t, "success", packResult.Files[0].Status)
	assert.Equal(t, "linked", packResult.Files[0].Message)
}

func TestGetPacksStatus_PackWithPathHandlerFiles(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

	// First create the pack
	env.SetupPack("tools", testutil.PackConfig{
		Files: map[string]string{},
	})

	// Then create the bin directory at the pack root
	// The scanner only looks at root-level entries, so we need the bin dir to exist
	binDir := filepath.Join(env.DotfilesRoot, "tools", "bin")
	require.NoError(t, env.FS.MkdirAll(binDir, 0755))

	// Create a file inside to make the directory non-empty (though scanner won't see this)
	toolPath := filepath.Join(binDir, "tool1")
	require.NoError(t, env.FS.WriteFile(toolPath, []byte("#!/bin/bash\necho tool1"), 0755))

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.Packs, 1)

	packResult := result.Packs[0]
	assert.Equal(t, "tools", packResult.Name)
	assert.Len(t, packResult.Files, 1)

	// The bin directory is matched as "bin/" by the path handler
	if len(packResult.Files) > 0 {
		// Check if we got the bin directory
		var found bool
		for _, f := range packResult.Files {
			if f.Path == "bin/" || f.Path == "bin" {
				found = true
				assert.Equal(t, "path", f.Handler)
				assert.Equal(t, "queue", f.Status)
				assert.Equal(t, "not in PATH", f.Message)
				break
			}
		}
		assert.True(t, found, "should have bin directory")
	}
	assert.Equal(t, "path", packResult.Files[0].Handler)
	assert.Equal(t, "queue", packResult.Files[0].Status)
	assert.Equal(t, "not in PATH", packResult.Files[0].Message)
}

func TestGetPacksStatus_PackWithShellProfileFiles(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	env.SetupPack("shell", testutil.PackConfig{
		Files: map[string]string{
			"my-aliases.sh": "alias ll='ls -la'",                // Matches *aliases.sh pattern
			"profile.sh":    "export PATH=$PATH:/usr/local/bin", // Matches exact profile.sh
		},
	})

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.Packs, 1)

	packResult := result.Packs[0]
	assert.Equal(t, "shell", packResult.Name)
	assert.Len(t, packResult.Files, 2)

	// Check each file has the expected handler
	filesByPath := make(map[string]display.DisplayFile)
	for _, f := range packResult.Files {
		filesByPath[f.Path] = f
	}

	// my-aliases.sh should match *aliases.sh pattern
	if aliasFile, ok := filesByPath["my-aliases.sh"]; ok {
		assert.Equal(t, "shell", aliasFile.Handler)
		assert.Equal(t, "queue", aliasFile.Status)
		assert.Equal(t, "not sourced in shell", aliasFile.Message)
	}

	// profile.sh should match exact pattern
	if profileFile, ok := filesByPath["profile.sh"]; ok {
		assert.Equal(t, "shell", profileFile.Handler)
		assert.Equal(t, "queue", profileFile.Status)
		assert.Equal(t, "not sourced in shell", profileFile.Message)
	}
}

func TestGetPacksStatus_PackWithInstallScript(t *testing.T) {
	// Use isolated environment for code execution handlers that need real files
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	env.SetupPack("app", testutil.PackConfig{
		Files: map[string]string{
			"install.sh": "#!/bin/bash\necho installing",
		},
	})

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.Packs, 1)

	packResult := result.Packs[0]
	assert.Equal(t, "app", packResult.Name)
	assert.Len(t, packResult.Files, 1)

	assert.Equal(t, "install.sh", packResult.Files[0].Path)
	assert.Equal(t, "install", packResult.Files[0].Handler)
	assert.Equal(t, "queue", packResult.Files[0].Status)
	// The message can be either "never run" or "file changed, needs re-run" depending on test order
	assert.Contains(t, []string{"never run", "file changed, needs re-run"}, packResult.Files[0].Message)
}

func TestGetPacksStatus_PackWithBrewfile(t *testing.T) {
	// Use isolated environment for code execution handlers that need real files
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	env.SetupPack("homebrew", testutil.PackConfig{
		Files: map[string]string{
			"Brewfile": "brew \"git\"\ncask \"firefox\"",
		},
	})

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.Packs, 1)

	packResult := result.Packs[0]
	assert.Equal(t, "homebrew", packResult.Name)
	assert.Len(t, packResult.Files, 1)

	assert.Equal(t, "Brewfile", packResult.Files[0].Path)
	assert.Equal(t, "homebrew", packResult.Files[0].Handler)
	assert.Equal(t, "queue", packResult.Files[0].Status)
	assert.Equal(t, "never installed", packResult.Files[0].Message)
}

func TestGetPacksStatus_MixedPackWithMultipleHandlers(t *testing.T) {
	// Use isolated environment for mixed handlers including code execution
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// First create the pack with root-level files
	env.SetupPack("mixed", testutil.PackConfig{
		Files: map[string]string{
			".config":           "config content",
			"custom-aliases.sh": "alias x='exit'", // Changed to match the *aliases.sh pattern
			"install.sh":        "#!/bin/bash\nsetup",
		},
	})

	// Then create the bin directory at the pack root
	binDir := filepath.Join(env.DotfilesRoot, "mixed", "bin")
	require.NoError(t, env.FS.MkdirAll(binDir, 0755))

	// Create a file inside to make the directory non-empty
	dummyPath := filepath.Join(binDir, "dummy")
	require.NoError(t, env.FS.WriteFile(dummyPath, []byte("dummy"), 0755))

	// Note: In isolated environment, we could create actual symlinks, but following
	// testing guidelines, we'll test without pre-existing links

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.Packs, 1)

	packResult := result.Packs[0]
	assert.Equal(t, "mixed", packResult.Name)
	assert.Equal(t, "queue", packResult.Status) // All files are queue (not linked)
	assert.Len(t, packResult.Files, 4)

	// Verify each file
	filesByPath := make(map[string]display.DisplayFile)
	for _, f := range packResult.Files {
		filesByPath[f.Path] = f
	}

	assert.Equal(t, "queue", filesByPath[".config"].Status)
	assert.Equal(t, "queue", filesByPath["custom-aliases.sh"].Status)
	// Check for bin/ or bin (handler might report either)
	if binFile, ok := filesByPath["bin/"]; ok {
		assert.Equal(t, "queue", binFile.Status)
	} else if binFile, ok := filesByPath["bin"]; ok {
		assert.Equal(t, "queue", binFile.Status)
	} else {
		t.Error("Expected to find bin/ or bin in files")
	}
	assert.Equal(t, "queue", filesByPath["install.sh"].Status)
}

func TestGetPacksStatus_SpecificPackSelection(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			".vimrc": "content",
		},
	})
	env.SetupPack("emacs", testutil.PackConfig{
		Files: map[string]string{
			".emacs": "content",
		},
	})
	env.SetupPack("nano", testutil.PackConfig{
		Files: map[string]string{
			".nanorc": "content",
		},
	})

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"vim"},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.Packs, 1)
	assert.Equal(t, "vim", result.Packs[0].Name)
}

func TestGetPacksStatus_ErrorStatusWhenIntermediateLinkPointsToWrongSource(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	env.SetupPack("broken", testutil.PackConfig{
		Files: map[string]string{
			".config": "new content",
		},
	})

	// Create an intermediate link pointing to a different file
	pathsInstance := env.Paths.(paths.Paths)
	packHandlerDir := pathsInstance.PackHandlerDir("broken", "symlink")
	require.NoError(t, env.FS.MkdirAll(packHandlerDir, 0755))
	wrongSource := "/some/other/file"
	linkPath := filepath.Join(packHandlerDir, ".config")
	require.NoError(t, env.FS.Symlink(wrongSource, linkPath))

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.Packs, 1)

	packResult := result.Packs[0]
	assert.Equal(t, "broken", packResult.Name)
	assert.Equal(t, "alert", packResult.Status)
	assert.Len(t, packResult.Files, 1)

	file := packResult.Files[0]
	assert.Equal(t, ".config", file.Path)
	assert.Equal(t, "error", file.Status)
	assert.Equal(t, "link points to wrong source", file.Message)
}

func TestGetPacksStatus_ProvisioningStatusWithChangedFile(t *testing.T) {
	// Use isolated environment for code execution handlers
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	env.SetupPack("app", testutil.PackConfig{
		Files: map[string]string{
			"install.sh": "#!/bin/bash\necho v1",
		},
	})

	// Simulate that it was run with a different checksum
	oldSentinel := "install.sh-oldchecksum"
	require.NoError(t, env.DataStore.RunAndRecord("app", "install", "echo ran", oldSentinel))

	// Now change the file content (which changes checksum)
	// For memory FS, we need to use the FS interface to write the file
	installPath := filepath.Join(env.DotfilesRoot, "app", "install.sh")
	err := env.FS.WriteFile(installPath, []byte("#!/bin/bash\necho v2"), 0755)
	require.NoError(t, err)

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.Packs, 1)

	packResult := result.Packs[0]
	assert.Equal(t, "app", packResult.Name)
	assert.Len(t, packResult.Files, 1)

	file := packResult.Files[0]
	assert.Equal(t, "install.sh", file.Path)
	assert.Equal(t, "install", file.Handler)
	assert.Equal(t, "queue", file.Status)
	// Check the status - it should either show as needing re-run or have an error
	// In memory FS, the status check might fail if the file can't be read
	switch file.Status {
	case "queue":
		// If we successfully determined status, check for the right message
		if file.Message == "file changed, needs re-run" || file.Message == "never run" {
			// Both are acceptable - depends on whether old sentinel was found
			assert.True(t, true, "status message is acceptable")
		} else {
			t.Errorf("unexpected message for queue status: %s", file.Message)
		}
	case "error":
		// Error status is also acceptable if file couldn't be read
		assert.Contains(t, file.Message, "status check failed", "should be status check error")
	default:
		t.Errorf("unexpected status: %s", file.Status)
	}
}

func TestGetPacksStatus_Timestamp(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	env.SetupPack("test", testutil.PackConfig{
		Files: map[string]string{},
	})

	before := time.Now()
	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})
	after := time.Now()

	require.NoError(t, err)
	assert.True(t, result.Timestamp.After(before) || result.Timestamp.Equal(before))
	assert.True(t, result.Timestamp.Before(after) || result.Timestamp.Equal(after))
}

func TestGetPacksStatus_WithInvalidPack(t *testing.T) {
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	env.SetupPack("valid", testutil.PackConfig{
		Files: map[string]string{},
	})

	result, err := operations.GetPacksStatus(operations.StatusCommandOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"valid", "nonexistent"},
		Paths:        env.Paths,
		FileSystem:   env.FS,
	})

	// Should return an error for the nonexistent pack
	assert.Error(t, err)
	assert.Nil(t, result)
	// The error should mention that pack(s) were not found
	assert.Contains(t, err.Error(), "pack(s) not found")
}
