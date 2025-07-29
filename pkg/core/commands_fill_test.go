package core

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestFillPack(t *testing.T) {
	tests := []struct {
		name      string
		setup     func(t *testing.T) (string, string)
		packName  string
		validate  func(t *testing.T, result *types.FillResult, packPath string)
		wantErr   bool
	}{
		{
			name: "fill empty pack",
			setup: func(t *testing.T) (string, string) {
				root := testutil.TempDir(t, "fill-test")
				pack := testutil.CreateDir(t, root, "test-pack")
				return root, pack
			},
			packName: "test-pack",
			validate: func(t *testing.T, result *types.FillResult, packPath string) {
				testutil.AssertEqual(t, "test-pack", result.PackName)
				// Should create all 4 template files
				testutil.AssertEqual(t, 4, len(result.FilesCreated))
				
				// Check that all expected files were created
				expectedFiles := []string{"aliases.sh", "install.sh", "Brewfile", "path.sh"}
				for _, expected := range expectedFiles {
					found := false
					for _, created := range result.FilesCreated {
						if created == expected {
							found = true
							break
						}
					}
					testutil.AssertTrue(t, found, "expected file %s to be created", expected)
					
					// Verify file exists on disk
					filePath := filepath.Join(packPath, expected)
					info, err := os.Stat(filePath)
					testutil.AssertNoError(t, err)
					testutil.AssertNotNil(t, info)
					
					// Check shell scripts are executable
					if expected != "Brewfile" {
						testutil.AssertTrue(t, info.Mode()&0111 != 0, "%s should be executable", expected)
					}
				}
			},
		},
		{
			name: "fill pack with existing files",
			setup: func(t *testing.T) (string, string) {
				root := testutil.TempDir(t, "fill-test")
				pack := testutil.CreateDir(t, root, "test-pack")
				// Create some existing files
				testutil.CreateFile(t, pack, "aliases.sh", "existing content")
				testutil.CreateFile(t, pack, "Brewfile", "brew 'existing'")
				return root, pack
			},
			packName: "test-pack",
			validate: func(t *testing.T, result *types.FillResult, packPath string) {
				// Should only create files that don't exist
				testutil.AssertEqual(t, 2, len(result.FilesCreated))
				
				// Check that only missing files were created
				for _, created := range result.FilesCreated {
					testutil.AssertTrue(t, created == "install.sh" || created == "path.sh",
						"unexpected file created: %s", created)
				}
				
				// Verify existing files weren't overwritten
				aliasesContent, err := os.ReadFile(filepath.Join(packPath, "aliases.sh"))
				testutil.AssertNoError(t, err)
				testutil.AssertEqual(t, "existing content", string(aliasesContent))
			},
		},
		{
			name: "fill non-existent pack",
			setup: func(t *testing.T) (string, string) {
				root := testutil.TempDir(t, "fill-test")
				return root, ""
			},
			packName: "non-existent",
			wantErr:  true,
		},
		{
			name: "fill pack with all files existing",
			setup: func(t *testing.T) (string, string) {
				root := testutil.TempDir(t, "fill-test")
				pack := testutil.CreateDir(t, root, "complete-pack")
				// Create all template files
				testutil.CreateFile(t, pack, "aliases.sh", "#!/bin/sh")
				testutil.CreateFile(t, pack, "install.sh", "#!/bin/bash")
				testutil.CreateFile(t, pack, "Brewfile", "brew 'git'")
				testutil.CreateFile(t, pack, "path.sh", "#!/bin/sh")
				return root, pack
			},
			packName: "complete-pack",
			validate: func(t *testing.T, result *types.FillResult, packPath string) {
				// Should not create any files
				testutil.AssertEqual(t, 0, len(result.FilesCreated))
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			root, packPath := tt.setup(t)
			
			opts := FillPackOptions{
				DotfilesRoot: root,
				PackName:     tt.packName,
			}
			
			result, err := FillPack(opts)
			
			if tt.wantErr {
				testutil.AssertError(t, err)
				return
			}
			
			testutil.AssertNoError(t, err)
			testutil.AssertNotNil(t, result)
			
			if tt.validate != nil {
				tt.validate(t, result, packPath)
			}
		})
	}
}

func TestFillPackFileContents(t *testing.T) {
	// Test that generated files have appropriate content
	root := testutil.TempDir(t, "fill-content-test")
	pack := testutil.CreateDir(t, root, "content-test-pack")
	
	opts := FillPackOptions{
		DotfilesRoot: root,
		PackName:     "content-test-pack",
	}
	
	result, err := FillPack(opts)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 4, len(result.FilesCreated))
	
	// Check aliases.sh content
	aliasesContent, err := os.ReadFile(filepath.Join(pack, "aliases.sh"))
	testutil.AssertNoError(t, err)
	testutil.AssertContains(t, string(aliasesContent), "#!/usr/bin/env sh")
	testutil.AssertContains(t, string(aliasesContent), "content-test-pack")
	testutil.AssertContains(t, string(aliasesContent), "alias ll='ls -la'")
	
	// Check install.sh content
	installContent, err := os.ReadFile(filepath.Join(pack, "install.sh"))
	testutil.AssertNoError(t, err)
	testutil.AssertContains(t, string(installContent), "#!/usr/bin/env bash")
	testutil.AssertContains(t, string(installContent), "set -euo pipefail")
	testutil.AssertContains(t, string(installContent), "Installing content-test-pack pack")
	
	// Check Brewfile content
	brewContent, err := os.ReadFile(filepath.Join(pack, "Brewfile"))
	testutil.AssertNoError(t, err)
	testutil.AssertContains(t, string(brewContent), "Homebrew dependencies")
	testutil.AssertContains(t, string(brewContent), "content-test-pack")
	testutil.AssertContains(t, string(brewContent), "brew 'git'")
	
	// Check path.sh content
	pathContent, err := os.ReadFile(filepath.Join(pack, "path.sh"))
	testutil.AssertNoError(t, err)
	testutil.AssertContains(t, string(pathContent), "#!/usr/bin/env sh")
	testutil.AssertContains(t, string(pathContent), "PATH additions")
	testutil.AssertContains(t, string(pathContent), "export PATH=")
}