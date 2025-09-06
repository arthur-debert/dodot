package context

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
)

func TestManager_CreateContext(t *testing.T) {
	tests := []struct {
		name    string
		command string
		dryRun  bool
	}{
		{
			name:    "create context for on command",
			command: "up",
			dryRun:  false,
		},
		{
			name:    "create context for status command with dry run",
			command: "status",
			dryRun:  true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := NewManager()
			ctx := m.CreateContext(tt.command, tt.dryRun)

			assert.Equal(t, tt.command, ctx.Command)
			assert.Equal(t, tt.dryRun, ctx.DryRun)
			assert.NotZero(t, ctx.StartTime)
			assert.Zero(t, ctx.EndTime)
			assert.NotNil(t, ctx.PackResults)
			assert.Empty(t, ctx.PackResults)
		})
	}
}

func TestManager_AddPackResult(t *testing.T) {
	m := NewManager()
	ctx := m.CreateContext("up", false)

	// Create pack results with different stats
	packResult1 := &PackExecutionResult{
		TotalHandlers:     5,
		CompletedHandlers: 3,
		FailedHandlers:    1,
		SkippedHandlers:   1,
	}

	packResult2 := &PackExecutionResult{
		TotalHandlers:     3,
		CompletedHandlers: 2,
		FailedHandlers:    0,
		SkippedHandlers:   1,
	}

	// Add first pack result
	m.AddPackResult(ctx, "vim", packResult1)

	assert.Equal(t, 1, len(ctx.PackResults))
	assert.Equal(t, 5, ctx.TotalHandlers)
	assert.Equal(t, 3, ctx.CompletedHandlers)
	assert.Equal(t, 1, ctx.FailedHandlers)
	assert.Equal(t, 1, ctx.SkippedHandlers)

	// Add second pack result
	m.AddPackResult(ctx, "git", packResult2)

	assert.Equal(t, 2, len(ctx.PackResults))
	assert.Equal(t, 8, ctx.TotalHandlers)
	assert.Equal(t, 5, ctx.CompletedHandlers)
	assert.Equal(t, 1, ctx.FailedHandlers)
	assert.Equal(t, 2, ctx.SkippedHandlers)

	// Update first pack result
	packResult1.CompletedHandlers = 4
	packResult1.FailedHandlers = 0
	m.AddPackResult(ctx, "vim", packResult1)

	assert.Equal(t, 2, len(ctx.PackResults))
	assert.Equal(t, 8, ctx.TotalHandlers)
	assert.Equal(t, 6, ctx.CompletedHandlers)
	assert.Equal(t, 0, ctx.FailedHandlers)
	assert.Equal(t, 2, ctx.SkippedHandlers)
}

func TestManager_CompleteContext(t *testing.T) {
	m := NewManager()
	ctx := m.CreateContext("up", false)

	// Initially EndTime should be zero
	assert.Zero(t, ctx.EndTime)

	// Sleep briefly to ensure EndTime > StartTime
	time.Sleep(10 * time.Millisecond)

	m.CompleteContext(ctx)

	assert.NotZero(t, ctx.EndTime)
	assert.True(t, ctx.EndTime.After(ctx.StartTime))
}
