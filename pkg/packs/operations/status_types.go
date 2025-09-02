package operations

import (
	"time"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// StatusOptions contains options for getting pack status
type StatusOptions struct {
	Pack       types.Pack
	DataStore  datastore.DataStore
	FileSystem types.FS
	Paths      paths.Paths
}

// StatusState represents the state of a deployment
type StatusState string

const (
	// StatusStatePending indicates the action has not been executed yet
	StatusStatePending StatusState = "pending"
	// StatusStateReady indicates the action was executed and is ready
	StatusStateReady StatusState = "ready"
	// StatusStateSuccess indicates success
	StatusStateSuccess StatusState = "success"
	// StatusStateMissing indicates missing files
	StatusStateMissing StatusState = "missing"
	// StatusStateError indicates an error occurred
	StatusStateError StatusState = "error"
	// StatusStateIgnored indicates the file/pack is ignored
	StatusStateIgnored StatusState = "ignored"
	// StatusStateConfig indicates this is a config file
	StatusStateConfig StatusState = "config"
)

// Status represents the status of a single item
type Status struct {
	State     StatusState
	Message   string
	Timestamp *time.Time
}

// FileStatus represents the status of a single file
type FileStatus struct {
	Handler        string
	Path           string
	Status         Status
	AdditionalInfo string
}

// StatusResult represents the complete status of a pack
type StatusResult struct {
	Name      string
	Path      string
	HasConfig bool
	IsIgnored bool
	Status    string
	Files     []FileStatus
}
