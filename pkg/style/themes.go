package style

import (
	"github.com/charmbracelet/lipgloss"
)

// Color definitions using AdaptiveColor for automatic light/dark mode switching
var (
	// Primary colors
	PrimaryColor = lipgloss.AdaptiveColor{
		Light: "#007ACC", // Blue
		Dark:  "#3D9EFF",
	}

	SecondaryColor = lipgloss.AdaptiveColor{
		Light: "#6C757D", // Gray
		Dark:  "#A0A8B0",
	}

	// Status colors
	SuccessColor = lipgloss.AdaptiveColor{
		Light: "#28A745", // Green
		Dark:  "#4CDD76",
	}

	ErrorColor = lipgloss.AdaptiveColor{
		Light: "#DC3545", // Red
		Dark:  "#FF6B7D",
	}

	WarningColor = lipgloss.AdaptiveColor{
		Light: "#FFC107", // Amber
		Dark:  "#FFD54F",
	}

	InfoColor = lipgloss.AdaptiveColor{
		Light: "#17A2B8", // Cyan
		Dark:  "#4DD0E1",
	}

	// Text colors
	HeadingColor = lipgloss.AdaptiveColor{
		Light: "#212529", // Almost black
		Dark:  "#F8F9FA", // Almost white
	}

	TextColor = lipgloss.AdaptiveColor{
		Light: "#495057", // Dark gray
		Dark:  "#E9ECEF", // Light gray
	}

	MutedColor = lipgloss.AdaptiveColor{
		Light: "#6C757D", // Medium gray
		Dark:  "#ADB5BD",
	}

	// Background colors
	BackgroundColor = lipgloss.AdaptiveColor{
		Light: "#FFFFFF", // White
		Dark:  "#1A1B26", // Dark blue-gray
	}

	SurfaceColor = lipgloss.AdaptiveColor{
		Light: "#F8F9FA", // Very light gray
		Dark:  "#24253A", // Slightly lighter than background
	}

	BorderColor = lipgloss.AdaptiveColor{
		Light: "#DEE2E6", // Light gray
		Dark:  "#3B3C4F",
	}
)

// PowerUp specific colors
var (
	SymlinkColor = lipgloss.AdaptiveColor{
		Light: "#0EA5E9", // Sky blue
		Dark:  "#38BDF8",
	}

	ProfileColor = lipgloss.AdaptiveColor{
		Light: "#8B5CF6", // Purple
		Dark:  "#A78BFA",
	}

	InstallScriptColor = lipgloss.AdaptiveColor{
		Light: "#F59E0B", // Orange
		Dark:  "#FBBF24",
	}

	HomebrewColor = lipgloss.AdaptiveColor{
		Light: "#10B981", // Emerald
		Dark:  "#34D399",
	}
)
