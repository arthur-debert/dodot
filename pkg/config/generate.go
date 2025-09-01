package config

import (
	"strings"
)

// GenerateConfigContent generates the configuration file content with commented values
func GenerateConfigContent() string {
	// Get the user defaults content
	userDefaultsContent := GetUserDefaultsContent()

	// Comment out the configuration values
	return commentOutConfigValues(userDefaultsContent)
}

// commentOutConfigValues takes the TOML content and comments out all non-comment, non-blank lines
// that contain configuration values (assignments)
func commentOutConfigValues(content string) string {
	lines := strings.Split(content, "\n")
	var result []string

	for _, line := range lines {
		trimmed := strings.TrimSpace(line)

		// Keep blank lines as-is
		if trimmed == "" {
			result = append(result, line)
			continue
		}

		// Keep lines that are already comments
		if strings.HasPrefix(trimmed, "#") {
			result = append(result, line)
			continue
		}

		// Keep section headers (e.g., [pack], [symlink]) as-is
		if strings.HasPrefix(trimmed, "[") && strings.HasSuffix(trimmed, "]") {
			result = append(result, line)
			continue
		}

		// Comment out configuration value lines
		result = append(result, "# "+line)
	}

	return strings.Join(result, "\n")
}
