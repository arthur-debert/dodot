package synthfs

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"sync/atomic"
	"time"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// IDGenerator defines the interface for generating operation IDs
type IDGenerator func(opType, path string) core.OperationID

var (
	// sequenceCounter for SequenceIDGenerator
	sequenceCounter atomic.Uint64
)

// HashIDGenerator generates IDs based on operation type and path hash
func HashIDGenerator(opType, path string) core.OperationID {
	h := sha256.New()
	h.Write([]byte(opType))
	h.Write([]byte(path))
	_, _ = fmt.Fprintf(h, "%d", time.Now().UnixNano())
	hash := hex.EncodeToString(h.Sum(nil))[:8]
	return core.OperationID(fmt.Sprintf("%s-%s", opType, hash))
}

// SequenceIDGenerator generates sequential IDs (useful for testing)
func SequenceIDGenerator(opType, path string) core.OperationID {
	seq := sequenceCounter.Add(1)
	return core.OperationID(fmt.Sprintf("%s-%d", opType, seq))
}

// TimestampIDGenerator generates IDs based on timestamp
func TimestampIDGenerator(opType, path string) core.OperationID {
	ts := time.Now().UnixNano()
	return core.OperationID(fmt.Sprintf("%s-%d", opType, ts))
}

// ResetSequenceCounter resets the sequence counter (for testing)
func ResetSequenceCounter() {
	sequenceCounter.Store(0)
}
