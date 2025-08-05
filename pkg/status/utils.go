package status

import "io/fs"

// isNotExist checks if an error indicates a file doesn't exist
func isNotExist(err error) bool {
	if err == nil {
		return false
	}
	if err == fs.ErrNotExist {
		return true
	}
	pathErr, ok := err.(*fs.PathError)
	return ok && pathErr.Err == fs.ErrNotExist
}
