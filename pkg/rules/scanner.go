package rules

import (
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog"
)

// Scanner scans packs and applies rules to find matches
type Scanner struct {
	rules  []config.Rule
	logger zerolog.Logger
	fs     types.FS // optional filesystem implementation
}

// NewScanner creates a new scanner with the given rules
func NewScanner(rules []config.Rule) *Scanner {
	return &Scanner{
		rules:  rules,
		logger: logging.GetLogger("rules.scanner"),
	}
}

// NewScannerWithFS creates a new scanner with the given rules and filesystem
func NewScannerWithFS(rules []config.Rule, fs types.FS) *Scanner {
	return &Scanner{
		rules:  rules,
		logger: logging.GetLogger("rules.scanner"),
		fs:     fs,
	}
}

// ScanPack scans a single pack and returns all matches
func (s *Scanner) ScanPack(pack types.Pack) ([]Match, error) {
	s.logger.Debug().
		Str("pack", pack.Name).
		Str("path", pack.Path).
		Msg("Scanning pack")

	// Read all files in the pack
	files, err := s.readPackFiles(pack.Path)
	if err != nil {
		return nil, err
	}

	s.logger.Debug().
		Int("fileCount", len(files)).
		Int("ruleCount", len(s.rules)).
		Msg("Starting pack scan")

	// Log files found for debugging
	for _, f := range files {
		s.logger.Debug().
			Str("file", f.Path).
			Bool("isDir", f.IsDirectory).
			Msg("Found file in pack")
	}

	// Match files against rules
	var matches []Match
	for _, file := range files {
		// Check if file should be excluded
		if s.isExcluded(file) {
			continue
		}

		// Find the first matching rule
		for _, rule := range s.rules {
			// Skip exclusion rules (already handled above)
			if strings.HasPrefix(rule.Pattern, "!") {
				continue
			}

			if s.matchesRule(file, rule) {
				s.logger.Debug().
					Str("file", file.Path).
					Str("pattern", rule.Pattern).
					Str("handler", rule.Handler).
					Msg("File matched rule")
				matches = append(matches, Match{
					PackName:    pack.Name,
					FilePath:    file.Path,
					FileName:    file.Name,
					IsDirectory: file.IsDirectory,
					Handler:     rule.Handler,
					Options:     rule.Options,
				})
				break // First match wins
			}
		}
	}

	s.logger.Debug().
		Str("pack", pack.Name).
		Int("matches", len(matches)).
		Msg("Pack scan complete")

	return matches, nil
}

// readPackFiles reads all files in a pack directory (non-recursive)
func (s *Scanner) readPackFiles(packPath string) ([]FileInfo, error) {
	var entries []os.DirEntry
	var err error

	if s.fs != nil {
		entries, err = s.fs.ReadDir(packPath)
	} else {
		entries, err = os.ReadDir(packPath)
	}
	if err != nil {
		return nil, err
	}

	var files []FileInfo
	for _, entry := range entries {
		// Skip certain hidden files but allow dotfiles that should be linked
		name := entry.Name()
		if strings.HasPrefix(name, ".") {
			// Skip special dodot files and common temp/system files
			if name == ".dodot.toml" || name == ".dodotignore" ||
				name == ".DS_Store" || name == ".git" || name == ".gitignore" {
				continue
			}
			// Allow other dotfiles like .vimrc, .bashrc, etc.
		}

		files = append(files, FileInfo{
			Path:        entry.Name(),
			Name:        entry.Name(),
			IsDirectory: entry.IsDir(),
		})
	}

	return files, nil
}

// isExcluded checks if a file matches any exclusion rule
func (s *Scanner) isExcluded(file FileInfo) bool {
	for _, rule := range s.rules {
		if strings.HasPrefix(rule.Pattern, "!") {
			// Remove the ! prefix for matching
			pattern := strings.TrimPrefix(rule.Pattern, "!")
			if s.matchesPattern(file, pattern) {
				return true
			}
		}
	}
	return false
}

// matchesRule checks if a file matches a rule's pattern
func (s *Scanner) matchesRule(file FileInfo, rule config.Rule) bool {
	return s.matchesPattern(file, rule.Pattern)
}

// matchesPattern checks if a file matches a pattern with our conventions
func (s *Scanner) matchesPattern(file FileInfo, pattern string) bool {
	// Directory matching - pattern ends with /
	if strings.HasSuffix(pattern, "/") {
		if !file.IsDirectory {
			return false
		}
		dirPattern := strings.TrimSuffix(pattern, "/")
		matched, _ := filepath.Match(dirPattern, file.Name)
		return matched
	}

	// Don't match directories with non-directory patterns
	if file.IsDirectory {
		return false
	}

	// Path pattern - contains /
	if strings.Contains(pattern, "/") {
		matched, _ := filepath.Match(pattern, file.Path)
		return matched
	}

	// Simple filename pattern
	matched, _ := filepath.Match(pattern, file.Name)
	return matched
}
