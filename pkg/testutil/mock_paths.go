package testutil

// MockPathResolver provides mock path resolution for testing
type MockPathResolver struct {
	home      string
	xdgConfig string
	xdgData   string
	xdgCache  string
	xdgState  string
	dotfiles  string
}

// NewMockPathResolver creates a new mock path resolver
func NewMockPathResolver(home, xdgConfig, xdgData string) *MockPathResolver {
	return &MockPathResolver{
		home:      home,
		xdgConfig: xdgConfig,
		xdgData:   xdgData,
		xdgCache:  home + "/.cache",
		xdgState:  home + "/.local/state",
		dotfiles:  home + "/dotfiles",
	}
}

// Home returns the home directory path
func (m *MockPathResolver) Home() string {
	return m.home
}

// DotfilesRoot returns the dotfiles root directory
func (m *MockPathResolver) DotfilesRoot() string {
	return m.dotfiles
}

// DataDir returns the XDG data directory
func (m *MockPathResolver) DataDir() string {
	return m.xdgData
}

// ConfigDir returns the XDG config directory
func (m *MockPathResolver) ConfigDir() string {
	return m.xdgConfig
}

// CacheDir returns the XDG cache directory
func (m *MockPathResolver) CacheDir() string {
	return m.xdgCache
}

// StateDir returns the XDG state directory  
func (m *MockPathResolver) StateDir() string {
	return m.xdgState
}

// WithDotfilesRoot sets a custom dotfiles root
func (m *MockPathResolver) WithDotfilesRoot(path string) *MockPathResolver {
	m.dotfiles = path
	return m
}