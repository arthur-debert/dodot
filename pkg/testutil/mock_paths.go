package testutil

// MockPaths is a test implementation of the Pather interface
type MockPaths struct {
	DotfilesRootPath string
	DataDirPath      string
	ConfigDirPath    string
	CacheDirPath     string
	StateDirPath     string
}

// DotfilesRoot returns the mock dotfiles root path
func (m *MockPaths) DotfilesRoot() string {
	if m.DotfilesRootPath == "" {
		return "/test/dotfiles"
	}
	return m.DotfilesRootPath
}

// DataDir returns the mock data directory path
func (m *MockPaths) DataDir() string {
	if m.DataDirPath == "" {
		return "/test/data"
	}
	return m.DataDirPath
}

// ConfigDir returns the mock config directory path
func (m *MockPaths) ConfigDir() string {
	if m.ConfigDirPath == "" {
		return "/test/config"
	}
	return m.ConfigDirPath
}

// CacheDir returns the mock cache directory path
func (m *MockPaths) CacheDir() string {
	if m.CacheDirPath == "" {
		return "/test/cache"
	}
	return m.CacheDirPath
}

// StateDir returns the mock state directory path
func (m *MockPaths) StateDir() string {
	if m.StateDirPath == "" {
		return "/test/state"
	}
	return m.StateDirPath
}

// DeployedSymlink returns the deployed symlink directory
func (m *MockPaths) DeployedSymlink() string {
	return m.DataDir() + "/deployed/symlink"
}

// DeployedPath returns the deployed path directory
func (m *MockPaths) DeployedPath() string {
	return m.DataDir() + "/deployed/path"
}

// DeployedShellProfile returns the deployed shell profile directory
func (m *MockPaths) DeployedShellProfile() string {
	return m.DataDir() + "/deployed/shell"
}

// InitScript returns the path to the init script
func (m *MockPaths) InitScript() string {
	return m.DataDir() + "/shell/dodot-init.sh"
}

// LogFile returns the path to the log file
func (m *MockPaths) LogFile() string {
	return m.DataDir() + "/dodot.log"
}
