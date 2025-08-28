package rules

import (
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog"
)

// Scanner scans packs and applies rules to find matches
type Scanner struct {
	rules  []Rule
	logger zerolog.Logger
	fs     types.FS // optional filesystem implementation
}

// NewScanner creates a new scanner with the given rules
func NewScanner(rules []Rule) *Scanner {
	return &Scanner{
		rules:  rules,
		logger: logging.GetLogger("rules.scanner"),
	}
}

// NewScannerWithFS creates a new scanner with the given rules and filesystem
func NewScannerWithFS(rules []Rule, fs types.FS) *Scanner {
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

	// Separate exclusion rules from normal rules
	exclusions, rules := s.separateRules()

	// Sort rules by priority (higher first)
	sort.Slice(rules, func(i, j int) bool {
		return rules[i].Priority > rules[j].Priority
	})

	s.logger.Debug().
		Int("fileCount", len(files)).
		Int("ruleCount", len(rules)).
		Int("exclusionCount", len(exclusions)).
		Msg("Starting pack scan with files and rules")

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
		// Check exclusions first
		if s.isExcluded(file, exclusions) {
			s.logger.Debug().
				Str("file", file.Path).
				Msg("File excluded by rule")
			continue
		}

		// Try to match against each rule
		for _, rule := range rules {
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
				break // first match wins
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

// separateRules separates exclusion rules (starting with !) from normal rules
func (s *Scanner) separateRules() (exclusions []Rule, normal []Rule) {
	for _, rule := range s.rules {
		if strings.HasPrefix(rule.Pattern, "!") {
			exclusions = append(exclusions, rule)
		} else {
			normal = append(normal, rule)
		}
	}
	return
}

// isExcluded checks if a file matches any exclusion rule
func (s *Scanner) isExcluded(file FileInfo, exclusions []Rule) bool {
	for _, rule := range exclusions {
		// Remove the ! prefix for matching
		pattern := strings.TrimPrefix(rule.Pattern, "!")
		if s.matchesPattern(file, pattern) {
			return true
		}
	}
	return false
}

// matchesRule checks if a file matches a rule's pattern
func (s *Scanner) matchesRule(file FileInfo, rule Rule) bool {
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
