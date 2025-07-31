package execution

import (
	"fmt"
	"io/fs"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// FileSystemInterface defines minimal filesystem operations needed by state tracker
type FileSystemInterface interface {
	Stat(path string) (fs.FileInfo, error)
}

// ItemInterface defines minimal interface for items
type ItemInterface interface {
	Path() string
}

// UnarchiveItemInterface defines interface for unarchive items
type UnarchiveItemInterface interface {
	ItemInterface
	ExtractPath() string
}

// PathState holds the projected state of a single path after all operations in a batch are applied
type PathState struct {
	Path         string
	WillExist    bool
	WillBeType   core.PathStateType
	CreatedBy    core.OperationID
	DeletedBy    core.OperationID
	ModifiedBy   []core.OperationID
	InitialState fs.FileInfo
}

// PathStateTracker manages the projected state of all paths affected by a batch of operations
type PathStateTracker struct {
	states map[string]*PathState
	fs     FileSystemInterface
}

// NewPathStateTracker creates a new tracker
func NewPathStateTracker(fs FileSystemInterface) *PathStateTracker {
	return &PathStateTracker{
		states: make(map[string]*PathState),
		fs:     fs,
	}
}

// GetState returns the projected state of a path
func (pst *PathStateTracker) GetState(path string) (*PathState, error) {
	if state, exists := pst.states[path]; exists {
		return state, nil
	}

	// If not tracked, check the real filesystem to create an initial state
	info, err := pst.fs.Stat(path)
	if err != nil {
		// An error here means it doesn't exist. This is a valid state.
		initialState := &PathState{
			Path:      path,
			WillExist: false,
		}
		pst.states[path] = initialState
		return initialState, nil
	}

	// The path exists on the filesystem
	var initialType core.PathStateType
	if info.IsDir() {
		initialType = core.PathStateDir
	} else if info.Mode()&fs.ModeSymlink != 0 {
		initialType = core.PathStateSymlink
	} else {
		initialType = core.PathStateFile
	}

	initialState := &PathState{
		Path:         path,
		WillExist:    true,
		WillBeType:   initialType,
		InitialState: info,
	}
	pst.states[path] = initialState
	return initialState, nil
}

// UpdateState applies the effect of an operation to the projected state of a path
func (pst *PathStateTracker) UpdateState(op OperationInterface) error {
	desc := op.Describe()
	opID := op.ID()

	switch desc.Type {
	case "create_file":
		return pst.updateStateForCreate(opID, desc.Path, core.PathStateFile)
	case "create_directory":
		return pst.updateStateForCreate(opID, desc.Path, core.PathStateDir)
	case "create_symlink":
		return pst.updateStateForCreate(opID, desc.Path, core.PathStateSymlink)
	case "create_archive":
		return pst.updateStateForCreate(opID, desc.Path, core.PathStateFile) // Archives are files

	case "delete":
		state, err := pst.GetState(desc.Path)
		if err != nil {
			return err
		}
		// In Phase II, it's a conflict to delete something that isn't projected to exist
		if !state.WillExist {
			return fmt.Errorf("validation conflict for %s: path %s to be deleted is not projected to exist", opID, desc.Path)
		}
		// It's also a conflict to delete something that's already been deleted
		if state.DeletedBy != "" {
			return fmt.Errorf("operation %s conflicts with %s: cannot delete %s, already scheduled for deletion", opID, state.DeletedBy, desc.Path)
		}
		// It is also a conflict to delete a path that was just created in this batch
		if state.CreatedBy != "" {
			return fmt.Errorf("operation %s conflicts with %s: cannot delete %s, it was created in the same batch", opID, state.CreatedBy, desc.Path)
		}
		state.WillExist = false
		state.DeletedBy = opID

	case "copy":
		// Get source and destination paths from details
		srcPath, _ := desc.Details["src"].(string)
		dstPath, _ := desc.Details["dst"].(string)

		if srcPath == "" || dstPath == "" {
			// Try to get from operation method if available
			if pathGetter, ok := op.(interface{ GetSrcPath() string }); ok {
				srcPath = pathGetter.GetSrcPath()
			}
			if pathGetter, ok := op.(interface{ GetDstPath() string }); ok {
				dstPath = pathGetter.GetDstPath()
			}
		}

		if srcPath == "" || dstPath == "" {
			return fmt.Errorf("copy operation %s missing source or destination path", opID)
		}

		// Validate source
		srcState, err := pst.GetState(srcPath)
		if err != nil {
			return err
		}
		if !srcState.WillExist {
			return fmt.Errorf("copy source %s does not exist", srcPath)
		}

		// Update destination
		return pst.updateStateForCreate(opID, dstPath, srcState.WillBeType)

	case "move":
		// Get source and destination paths from details
		srcPath, _ := desc.Details["src"].(string)
		dstPath, _ := desc.Details["dst"].(string)

		if srcPath == "" || dstPath == "" {
			// Try to get from operation method if available
			if pathGetter, ok := op.(interface{ GetSrcPath() string }); ok {
				srcPath = pathGetter.GetSrcPath()
			}
			if pathGetter, ok := op.(interface{ GetDstPath() string }); ok {
				dstPath = pathGetter.GetDstPath()
			}
		}

		if srcPath == "" || dstPath == "" {
			return fmt.Errorf("move operation %s missing source or destination path", opID)
		}

		// Validate source
		srcState, err := pst.GetState(srcPath)
		if err != nil {
			return err
		}
		if !srcState.WillExist {
			return fmt.Errorf("validation conflict for %s: move source %s does not exist", opID, srcPath)
		}
		if srcState.DeletedBy != "" {
			return fmt.Errorf("operation %s conflicts with %s: cannot move %s, already scheduled for deletion", opID, srcState.DeletedBy, srcPath)
		}

		// Update source to be deleted
		srcState.WillExist = false
		srcState.DeletedBy = opID

		// Update destination to be created
		return pst.updateStateForCreate(opID, dstPath, srcState.WillBeType)

	case "unarchive":
		// This is more complex as it affects an unknown number of paths
		// For now, we'll just check the source archive exists and treat the destination as modified
		state, err := pst.GetState(desc.Path)
		if err != nil {
			return err
		}
		if !state.WillExist {
			return fmt.Errorf("validation conflict for %s: source archive %s does not exist", opID, desc.Path)
		}

		// Mark destination directory as modified
		if item := op.GetItem(); item != nil {
			if unarchiveItem, ok := item.(UnarchiveItemInterface); ok {
				destPath := unarchiveItem.ExtractPath()
				destState, err := pst.GetState(destPath)
				if err != nil {
					return err
				}
				destState.ModifiedBy = append(destState.ModifiedBy, opID)
			}
		}
	}

	return nil
}

// IsDeleted returns true if the path is scheduled for deletion by any operation
func (pst *PathStateTracker) IsDeleted(path string) bool {
	state, err := pst.GetState(path)
	if err != nil {
		return false
	}
	return state.DeletedBy != ""
}

func (pst *PathStateTracker) updateStateForCreate(opID core.OperationID, path string, createType core.PathStateType) error {
	state, err := pst.GetState(path)
	if err != nil {
		return err
	}

	if state.WillExist {
		return fmt.Errorf("operation %s conflicts with existing state: cannot create %s because it is projected to already exist", opID, path)
	}
	if state.DeletedBy != "" {
		// A file was deleted, now we are creating it again. This should probably be a "modify" operation
		// For now, let's treat it as a conflict to keep it simple
		return fmt.Errorf("operation %s conflicts with %s: cannot create %s, path was scheduled for deletion", opID, state.DeletedBy, path)
	}

	state.WillExist = true
	state.WillBeType = createType
	state.CreatedBy = opID
	state.DeletedBy = "" // Reset deleted status
	return nil
}
