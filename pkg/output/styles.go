package output

import (
	"github.com/arthur-debert/dodot/pkg/output/styles"
)

// LoadStylesFromFile loads a custom styles configuration from the specified file path.
// This allows users to override the default styles at runtime.
func LoadStylesFromFile(path string) error {
	return styles.LoadStyles(path)
}
