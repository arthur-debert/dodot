package status

import (
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// Checker defines the interface for checking the status of files
// managed by different PowerUps
type Checker interface {
	// CheckStatus examines the current state of a file and returns its status
	CheckStatus(op *types.Operation, fs filesystem.FullFileSystem) (*types.FileStatus, error)
}

// PowerUpChecker provides a registry of status checkers for different PowerUps
type PowerUpChecker struct {
	checkers map[string]Checker
	fs       filesystem.FullFileSystem
}

// NewPowerUpChecker creates a new PowerUpChecker with the given filesystem
func NewPowerUpChecker(fs filesystem.FullFileSystem) *PowerUpChecker {
	pc := &PowerUpChecker{
		checkers: make(map[string]Checker),
		fs:       fs,
	}

	// Register all powerup checkers
	pc.checkers["symlink"] = NewSymlinkChecker()
	pc.checkers["shell_profile"] = NewProfileChecker()
	pc.checkers["add_path"] = NewPathChecker()
	pc.checkers["homebrew"] = NewBrewChecker()

	return pc
}

// CheckOperationStatus checks the status of an operation based on its PowerUp type
func (pc *PowerUpChecker) CheckOperationStatus(op *types.Operation) (*types.FileStatus, error) {
	checker, exists := pc.checkers[op.PowerUp]
	if !exists {
		// For unknown powerups, return a basic status
		return &types.FileStatus{
			Path:     op.Source,
			PowerUp:  op.PowerUp,
			Status:   types.StatusUnknown,
			Message:  "No status checker available for this PowerUp",
			Metadata: make(map[string]interface{}),
		}, nil
	}

	return checker.CheckStatus(op, pc.fs)
}
