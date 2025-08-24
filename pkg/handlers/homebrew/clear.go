package homebrew

import (
	"bufio"
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// brewPackage represents a package listed in a Brewfile
type brewPackage struct {
	Name     string
	Type     string // "brew" or "cask"
	Brewfile string // which Brewfile it came from
}

// parseBrewfile reads a Brewfile and extracts package information
func parseBrewfile(fs types.FS, brewfilePath string) ([]brewPackage, error) {
	content, err := fs.ReadFile(brewfilePath)
	if err != nil {
		return nil, fmt.Errorf("failed to read Brewfile: %w", err)
	}

	var packages []brewPackage
	scanner := bufio.NewScanner(bytes.NewReader(content))

	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())

		// Skip empty lines and comments
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}

		// Parse brew "package" lines
		// Check for brew/cask followed by space or tab
		if strings.HasPrefix(line, "brew ") || strings.HasPrefix(line, "brew\t") {
			pkg := extractPackageName(line, "brew")
			if pkg != "" {
				packages = append(packages, brewPackage{
					Name:     pkg,
					Type:     "brew",
					Brewfile: filepath.Base(brewfilePath),
				})
			}
		} else if strings.HasPrefix(line, "cask ") || strings.HasPrefix(line, "cask\t") {
			pkg := extractPackageName(line, "cask")
			if pkg != "" {
				packages = append(packages, brewPackage{
					Name:     pkg,
					Type:     "cask",
					Brewfile: filepath.Base(brewfilePath),
				})
			}
		}
	}

	if err := scanner.Err(); err != nil {
		return nil, fmt.Errorf("error scanning Brewfile: %w", err)
	}

	// Sort packages by name for consistent ordering
	sort.Slice(packages, func(i, j int) bool {
		return packages[i].Name < packages[j].Name
	})

	return packages, nil
}

// extractPackageName extracts the package name from a brew/cask line
func extractPackageName(line, prefix string) string {
	// Remove the prefix
	line = strings.TrimPrefix(line, prefix)
	line = strings.TrimSpace(line)

	// Handle quoted package names
	if strings.HasPrefix(line, `"`) {
		endQuote := strings.Index(line[1:], `"`)
		if endQuote >= 0 { // >= 0 to handle empty quotes ""
			return line[1 : endQuote+1]
		}
	} else if strings.HasPrefix(line, `'`) {
		endQuote := strings.Index(line[1:], `'`)
		if endQuote >= 0 { // >= 0 to handle empty quotes ''
			return line[1 : endQuote+1]
		}
	}

	// Handle unquoted package names (take first word)
	parts := strings.Fields(line)
	if len(parts) > 0 {
		return parts[0]
	}

	return ""
}

// getInstalledPackages gets the list of installed brew packages
func getInstalledPackages() (map[string]bool, error) {
	installed := make(map[string]bool)

	// Get regular brew packages
	cmd := exec.Command("brew", "list", "--formula")
	output, err := cmd.Output()
	if err != nil {
		// If brew is not installed, return empty map
		if isCommandNotFound(err) {
			return installed, nil
		}
		return nil, fmt.Errorf("failed to list brew packages: %w", err)
	}

	scanner := bufio.NewScanner(bytes.NewReader(output))
	for scanner.Scan() {
		pkg := strings.TrimSpace(scanner.Text())
		if pkg != "" {
			installed[pkg] = true
		}
	}

	// Get cask packages
	cmd = exec.Command("brew", "list", "--cask")
	output, err = cmd.Output()
	if err != nil {
		// Casks might not be installed, that's OK
		return installed, nil
	}

	scanner = bufio.NewScanner(bytes.NewReader(output))
	for scanner.Scan() {
		pkg := strings.TrimSpace(scanner.Text())
		if pkg != "" {
			installed[pkg] = true
		}
	}

	return installed, nil
}

// isCommandNotFound checks if an error is due to command not found
func isCommandNotFound(err error) bool {
	if exitErr, ok := err.(*exec.ExitError); ok {
		return exitErr.ExitCode() == 127
	}
	return false
}

// promptForConfirmation asks the user to confirm package uninstallation
func promptForConfirmation(packages []brewPackage) (bool, error) {
	fmt.Println("\nThe following Homebrew packages will be uninstalled:")

	// Group by type
	var brews, casks []string
	for _, pkg := range packages {
		if pkg.Type == "brew" {
			brews = append(brews, pkg.Name)
		} else {
			casks = append(casks, pkg.Name)
		}
	}

	if len(brews) > 0 {
		fmt.Printf("  Formulae: %s\n", strings.Join(brews, ", "))
	}
	if len(casks) > 0 {
		fmt.Printf("  Casks: %s\n", strings.Join(casks, ", "))
	}

	fmt.Print("\nProceed with uninstallation? [y/N]: ")

	var response string
	_, err := fmt.Scanln(&response)
	if err != nil && err.Error() != "unexpected newline" {
		return false, err
	}

	response = strings.ToLower(strings.TrimSpace(response))
	return response == "y" || response == "yes", nil
}

// uninstallPackages uninstalls the given packages
func uninstallPackages(packages []brewPackage, dryRun bool) ([]types.ClearedItem, error) {
	logger := logging.GetLogger("handlers.homebrew")
	var clearedItems []types.ClearedItem

	// Group packages by type for efficient uninstallation
	brews := make([]string, 0)
	casks := make([]string, 0)

	for _, pkg := range packages {
		if pkg.Type == "brew" {
			brews = append(brews, pkg.Name)
		} else {
			casks = append(casks, pkg.Name)
		}
	}

	// Uninstall formulae
	if len(brews) > 0 {
		if dryRun {
			for _, pkg := range brews {
				clearedItems = append(clearedItems, types.ClearedItem{
					Type:        "brew_formula",
					Path:        pkg,
					Description: fmt.Sprintf("Would uninstall formula: %s", pkg),
				})
			}
		} else {
			args := append([]string{"uninstall", "--force"}, brews...)
			cmd := exec.Command("brew", args...)

			logger.Debug().
				Strs("packages", brews).
				Msg("Uninstalling brew formulae")

			output, err := cmd.CombinedOutput()
			if err != nil {
				logger.Error().
					Err(err).
					Str("output", string(output)).
					Msg("Failed to uninstall some formulae")
				// Continue anyway - some might have succeeded
			}

			for _, pkg := range brews {
				clearedItems = append(clearedItems, types.ClearedItem{
					Type:        "brew_formula",
					Path:        pkg,
					Description: fmt.Sprintf("Uninstalled formula: %s", pkg),
				})
			}
		}
	}

	// Uninstall casks
	if len(casks) > 0 {
		if dryRun {
			for _, pkg := range casks {
				clearedItems = append(clearedItems, types.ClearedItem{
					Type:        "brew_cask",
					Path:        pkg,
					Description: fmt.Sprintf("Would uninstall cask: %s", pkg),
				})
			}
		} else {
			args := append([]string{"uninstall", "--cask", "--force"}, casks...)
			cmd := exec.Command("brew", args...)

			logger.Debug().
				Strs("packages", casks).
				Msg("Uninstalling brew casks")

			output, err := cmd.CombinedOutput()
			if err != nil {
				logger.Error().
					Err(err).
					Str("output", string(output)).
					Msg("Failed to uninstall some casks")
				// Continue anyway - some might have succeeded
			}

			for _, pkg := range casks {
				clearedItems = append(clearedItems, types.ClearedItem{
					Type:        "brew_cask",
					Path:        pkg,
					Description: fmt.Sprintf("Uninstalled cask: %s", pkg),
				})
			}
		}
	}

	return clearedItems, nil
}

// ClearWithUninstall performs the full clear operation including package uninstallation
func (h *HomebrewHandler) ClearWithUninstall(ctx types.ClearContext) ([]types.ClearedItem, error) {
	logger := logging.GetLogger("handlers.homebrew").With().
		Str("pack", ctx.Pack.Name).
		Bool("dryRun", ctx.DryRun).
		Logger()

	var allClearedItems []types.ClearedItem

	// Read state to understand what was installed
	stateDir := ctx.Paths.PackHandlerDir(ctx.Pack.Name, "homebrew")
	entries, err := ctx.FS.ReadDir(stateDir)
	if err != nil {
		if os.IsNotExist(err) {
			logger.Debug().Msg("No homebrew state directory")
			return allClearedItems, nil
		}
		return nil, fmt.Errorf("failed to read homebrew state: %w", err)
	}

	// Get installed packages list
	installedPackages, err := getInstalledPackages()
	if err != nil {
		logger.Warn().Err(err).Msg("Failed to get installed packages list")
		// Continue anyway - we'll just track state removal
	}

	// Find all packages from Brewfiles
	var packagesToUninstall []brewPackage

	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".sentinel") {
			continue
		}

		// Find the corresponding Brewfile
		brewfileName := strings.TrimSuffix(entry.Name(), ".sentinel")
		if idx := strings.Index(brewfileName, "_"); idx >= 0 {
			brewfileName = brewfileName[idx+1:]
		}

		brewfilePath := filepath.Join(ctx.Pack.Path, brewfileName)

		// Parse the Brewfile
		packages, err := parseBrewfile(ctx.FS, brewfilePath)
		if err != nil {
			logger.Warn().
				Err(err).
				Str("brewfile", brewfilePath).
				Msg("Failed to parse Brewfile")
			continue
		}

		// Filter to only installed packages
		for _, pkg := range packages {
			if installedPackages[pkg.Name] {
				packagesToUninstall = append(packagesToUninstall, pkg)
			}
		}
	}

	// Handle package uninstallation
	if len(packagesToUninstall) > 0 {
		if ctx.DryRun {
			// In dry run, just report what would be uninstalled
			items, _ := uninstallPackages(packagesToUninstall, true)
			allClearedItems = append(allClearedItems, items...)
		} else {
			// Ask for confirmation
			shouldUninstall, err := promptForConfirmation(packagesToUninstall)
			if err != nil {
				logger.Error().Err(err).Msg("Failed to get user confirmation")
			} else if shouldUninstall {
				items, err := uninstallPackages(packagesToUninstall, false)
				if err != nil {
					logger.Error().Err(err).Msg("Error during package uninstallation")
				}
				allClearedItems = append(allClearedItems, items...)
			} else {
				logger.Info().Msg("User declined package uninstallation")
			}
		}
	}

	// Add state removal items
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".sentinel") {
			continue
		}

		sentinelPath := filepath.Join(stateDir, entry.Name())
		if ctx.DryRun {
			allClearedItems = append(allClearedItems, types.ClearedItem{
				Type:        "homebrew_state",
				Path:        sentinelPath,
				Description: "Would remove Homebrew state file",
			})
		} else {
			allClearedItems = append(allClearedItems, types.ClearedItem{
				Type:        "homebrew_state",
				Path:        sentinelPath,
				Description: "Removed Homebrew state file",
			})
		}
	}

	return allClearedItems, nil
}
