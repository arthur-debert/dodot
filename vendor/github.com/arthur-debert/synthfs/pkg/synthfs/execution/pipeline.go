package execution

import (
	"context"
	"fmt"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
	"github.com/gammazero/toposort"
)

// Pipeline defines the interface for operation pipeline management
type Pipeline interface {
	Add(ops ...interface{}) error
	Operations() []interface{}
	Resolve() error
	ResolvePrerequisites(resolver core.PrerequisiteResolver, fs interface{}) error
	Validate(ctx context.Context, fs interface{}) error
}

// memPipeline is an in-memory implementation of Pipeline
type memPipeline struct {
	ops      []OperationInterface
	idIndex  map[core.OperationID]int
	resolved bool
	logger   core.Logger
}

// NewMemPipeline creates a new in-memory pipeline
func NewMemPipeline(logger core.Logger) Pipeline {
	if logger == nil {
		logger = &noOpLogger{}
	}
	return &memPipeline{
		ops:      []OperationInterface{},
		idIndex:  make(map[core.OperationID]int),
		resolved: false,
		logger:   logger,
	}
}

// Add adds operations to the pipeline
func (mp *memPipeline) Add(ops ...interface{}) error {
	mp.logger.Info().
		Int("existing_operations", len(mp.ops)).
		Int("new_operations", len(ops)).
		Msg("adding operations to queue")

	for _, opInterface := range ops {
		if opInterface == nil {
			return fmt.Errorf("cannot add a nil operation to the pipeline")
		}

		op, ok := opInterface.(OperationInterface)
		if !ok {
			return fmt.Errorf("invalid operation type: expected OperationInterface")
		}

		// Check for duplicate IDs
		if _, exists := mp.idIndex[op.ID()]; exists {
			return fmt.Errorf("operation with ID '%s' already exists in the pipeline", op.ID())
		}

		mp.logger.Info().
			Str("op_id", string(op.ID())).
			Str("op_type", op.Describe().Type).
			Str("path", op.Describe().Path).
			Int("dependencies", len(op.Dependencies())).
			Msg("operation added to queue")

		// Add operation to queue
		index := len(mp.ops)
		mp.ops = append(mp.ops, op)
		mp.idIndex[op.ID()] = index

		// Mark as unresolved since we added new operations
		mp.resolved = false
	}

	mp.logger.Info().
		Int("total_operations", len(mp.ops)).
		Msg("operations added to queue successfully")

	return nil
}

// Operations returns all operations currently in the pipeline
func (mp *memPipeline) Operations() []interface{} {
	// Return a copy to prevent external modification
	result := make([]interface{}, len(mp.ops))
	for i, op := range mp.ops {
		result[i] = op
	}
	return result
}

// Resolve performs dependency resolution using topological sorting
func (mp *memPipeline) Resolve() error {
	mp.logger.Info().
		Int("operations", len(mp.ops)).
		Bool("already_resolved", mp.resolved).
		Msg("starting dependency resolution")

	if len(mp.ops) == 0 {
		mp.resolved = true
		mp.logger.Info().Msg("no operations to resolve")
		return nil
	}

	if mp.resolved {
		mp.logger.Info().Msg("dependencies already resolved")
		return nil
	}

	// Validate that all dependencies exist
	mp.logger.Info().Msg("validating dependency references")
	if err := mp.validateDependencies(); err != nil {
		mp.logger.Info().
			Err(err).
			Msg("dependency validation failed")
		return fmt.Errorf("dependency validation failed: %w", err)
	}
	mp.logger.Info().Msg("dependency references validated successfully")

	// Build dependency graph using topological sort library
	edges := make([]toposort.Edge, 0)

	for _, op := range mp.ops {
		for _, depID := range op.Dependencies() {
			// Edge is [2]interface{} where element 0 comes before element 1
			// So dependency -> operation (dependency must come first)
			edges = append(edges, toposort.Edge{string(depID), string(op.ID())})
		}
	}

	mp.logger.Info().
		Int("dependency_edges", len(edges)).
		Msg("performing topological sort")

	// Perform topological sort
	sortedIDs, err := toposort.Toposort(edges)
	if err != nil {
		mp.logger.Info().
			Err(err).
			Msg("topological sort failed - circular dependency detected")
		return fmt.Errorf("circular dependency detected: %w", err)
	}

	// Rebuild operations slice in topologically sorted order
	resolvedOps := make([]OperationInterface, 0, len(mp.ops))
	newIdIndex := make(map[core.OperationID]int)

	// Add operations in dependency order
	for _, idInterface := range sortedIDs {
		idStr, ok := idInterface.(string)
		if !ok {
			return fmt.Errorf("unexpected type in topological sort result: %T", idInterface)
		}
		opID := core.OperationID(idStr)
		if oldIndex, exists := mp.idIndex[opID]; exists {
			newIndex := len(resolvedOps)
			resolvedOps = append(resolvedOps, mp.ops[oldIndex])
			newIdIndex[opID] = newIndex
		}
	}

	// Add any operations that weren't in the dependency graph (no dependencies or dependents)
	for _, op := range mp.ops {
		if _, alreadyAdded := newIdIndex[op.ID()]; !alreadyAdded {
			newIndex := len(resolvedOps)
			resolvedOps = append(resolvedOps, op)
			newIdIndex[op.ID()] = newIndex
		}
	}

	mp.ops = resolvedOps
	mp.idIndex = newIdIndex
	mp.resolved = true

	mp.logger.Info().
		Int("resolved_operations", len(resolvedOps)).
		Msg("dependency resolution completed successfully")

	return nil
}

// ResolvePrerequisites resolves prerequisites for all operations in the pipeline
func (mp *memPipeline) ResolvePrerequisites(resolver core.PrerequisiteResolver, fs interface{}) error {
	mp.logger.Info().
		Int("operations", len(mp.ops)).
		Msg("starting prerequisite resolution")

	if len(mp.ops) == 0 {
		mp.logger.Info().Msg("no operations to resolve prerequisites for")
		return nil
	}

	// Track already resolved prerequisites to avoid duplicates
	resolvedPrereqs := make(map[string]bool)
	newOps := make([]OperationInterface, 0)

	for _, op := range mp.ops {
		// Get prerequisites for this operation
		prereqs := op.Prerequisites()

		mp.logger.Debug().
			Str("op_id", string(op.ID())).
			Int("prerequisites", len(prereqs)).
			Msg("processing operation prerequisites")

		// Process each prerequisite
		for _, prereq := range prereqs {
			prereqKey := fmt.Sprintf("%s:%s", prereq.Type(), prereq.Path())
			
			// Skip if already resolved
			if resolvedPrereqs[prereqKey] {
				mp.logger.Debug().
					Str("prereq_type", prereq.Type()).
					Str("prereq_path", prereq.Path()).
					Msg("prerequisite already resolved")
				continue
			}

			// Check if prerequisite is already satisfied
			if err := prereq.Validate(fs); err == nil {
				mp.logger.Debug().
					Str("prereq_type", prereq.Type()).
					Str("prereq_path", prereq.Path()).
					Msg("prerequisite already satisfied")
				resolvedPrereqs[prereqKey] = true
				continue
			}

			// Try to resolve the prerequisite
			if resolver.CanResolve(prereq) {
				mp.logger.Debug().
					Str("prereq_type", prereq.Type()).
					Str("prereq_path", prereq.Path()).
					Msg("resolving prerequisite")

				resolvedOps, err := resolver.Resolve(prereq)
				if err != nil {
					mp.logger.Debug().
						Str("prereq_type", prereq.Type()).
						Str("prereq_path", prereq.Path()).
						Err(err).
						Msg("failed to resolve prerequisite")
					return fmt.Errorf("failed to resolve prerequisite %s for path %s: %w", prereq.Type(), prereq.Path(), err)
				}

				// Add resolved operations to the pipeline
				for _, resolvedOp := range resolvedOps {
					if resolvedOpInterface, ok := resolvedOp.(OperationInterface); ok {
						newOps = append(newOps, resolvedOpInterface)
						
						// Add dependency from original operation to resolved operation
						if depAdder, ok := op.(interface{ AddDependency(core.OperationID) }); ok {
							depAdder.AddDependency(resolvedOpInterface.ID())
						}
						
						mp.logger.Debug().
							Str("resolved_op_id", string(resolvedOpInterface.ID())).
							Str("dependent_op_id", string(op.ID())).
							Msg("created prerequisite operation and dependency")
					}
				}

				resolvedPrereqs[prereqKey] = true
			} else {
				mp.logger.Debug().
					Str("prereq_type", prereq.Type()).
					Str("prereq_path", prereq.Path()).
					Msg("prerequisite not resolvable - will be validated later")
			}
		}
	}

	// Add new operations to the pipeline
	if len(newOps) > 0 {
		mp.logger.Info().
			Int("new_operations", len(newOps)).
			Msg("adding resolved prerequisite operations")

		for _, newOp := range newOps {
			// Check for duplicate IDs
			if _, exists := mp.idIndex[newOp.ID()]; exists {
				mp.logger.Warn().
					Str("op_id", string(newOp.ID())).
					Msg("prerequisite operation ID already exists - skipping")
				continue
			}

			// Add operation to pipeline
			index := len(mp.ops)
			mp.ops = append(mp.ops, newOp)
			mp.idIndex[newOp.ID()] = index

			mp.logger.Debug().
				Str("op_id", string(newOp.ID())).
				Str("op_type", newOp.Describe().Type).
				Str("path", newOp.Describe().Path).
				Msg("added prerequisite operation")
		}

		// Mark as unresolved since we added new operations
		mp.resolved = false
	}

	mp.logger.Info().
		Int("resolved_prerequisites", len(resolvedPrereqs)).
		Int("new_operations", len(newOps)).
		Msg("prerequisite resolution completed")

	return nil
}

// Validate checks if all operations in the pipeline are valid
func (mp *memPipeline) Validate(ctx context.Context, fs interface{}) error {
	mp.logger.Debug().
		Int("total_operations", len(mp.ops)).
		Bool("resolved", mp.resolved).
		Msg("starting comprehensive pipeline validation")

	// First validate dependencies exist
	mp.logger.Debug().Msg("validating operation dependencies")
	if err := mp.validateDependencies(); err != nil {
		mp.logger.Debug().
			Err(err).
			Msg("dependency validation failed")
		return err
	}
	mp.logger.Debug().Msg("dependency validation completed successfully")

	// Validate each operation individually
	mp.logger.Debug().Msg("validating individual operations")
	for i, op := range mp.ops {
		mp.logger.Debug().
			Int("operation_index", i+1).
			Int("total_operations", len(mp.ops)).
			Str("op_id", string(op.ID())).
			Str("op_type", op.Describe().Type).
			Str("path", op.Describe().Path).
			Msg("validating individual operation")

		if err := op.ValidateV2(ctx, &core.ExecutionContext{Logger: mp.logger}, fs); err != nil {
			mp.logger.Debug().
				Str("op_id", string(op.ID())).
				Str("op_type", op.Describe().Type).
				Str("path", op.Describe().Path).
				Err(err).
				Msg("individual operation validation failed")
			return fmt.Errorf("validation failed for operation %s: %w", op.ID(), err)
		}

		mp.logger.Debug().
			Str("op_id", string(op.ID())).
			Str("op_type", op.Describe().Type).
			Str("path", op.Describe().Path).
			Msg("individual operation validation passed")
	}
	mp.logger.Debug().
		Int("validated_operations", len(mp.ops)).
		Msg("individual operation validation completed successfully")

	// Check for conflicts
	mp.logger.Debug().Msg("validating operation conflicts")
	if err := mp.validateConflicts(); err != nil {
		mp.logger.Debug().
			Err(err).
			Msg("conflict validation failed")
		return err
	}
	mp.logger.Debug().Msg("conflict validation completed successfully")

	mp.logger.Debug().
		Int("total_operations", len(mp.ops)).
		Msg("comprehensive pipeline validation completed successfully")

	return nil
}

// validateDependencies ensures all referenced dependencies exist in the pipeline
func (mp *memPipeline) validateDependencies() error {
	mp.logger.Debug().
		Int("operations_to_check", len(mp.ops)).
		Msg("checking dependency references")

	dependencyCount := 0
	for _, op := range mp.ops {
		deps := op.Dependencies()
		dependencyCount += len(deps)

		mp.logger.Debug().
			Str("op_id", string(op.ID())).
			Str("op_type", op.Describe().Type).
			Interface("dependencies", deps).
			Int("dependency_count", len(deps)).
			Msg("checking operation dependencies")

		for _, depID := range deps {
			if _, exists := mp.idIndex[depID]; !exists {
				mp.logger.Debug().
					Str("op_id", string(op.ID())).
					Str("missing_dependency", string(depID)).
					Interface("all_dependencies", deps).
					Msg("dependency reference validation failed - missing dependency")

				return fmt.Errorf("operation %s has missing dependency: %s", op.ID(), depID)
			} else {
				mp.logger.Debug().
					Str("op_id", string(op.ID())).
					Str("dependency", string(depID)).
					Msg("dependency reference found")
			}
		}
	}

	mp.logger.Debug().
		Int("total_dependencies", dependencyCount).
		Int("operations_checked", len(mp.ops)).
		Msg("dependency reference validation completed")

	return nil
}

// validateConflicts checks for operations that conflict with each other
func (mp *memPipeline) validateConflicts() error {
	mp.logger.Debug().
		Int("operations_to_check", len(mp.ops)).
		Msg("checking operation conflicts")

	conflictCount := 0
	for _, op := range mp.ops {
		conflicts := op.Conflicts()
		conflictCount += len(conflicts)

		if len(conflicts) > 0 {
			mp.logger.Debug().
				Str("op_id", string(op.ID())).
				Str("op_type", op.Describe().Type).
				Interface("conflicts", conflicts).
				Int("conflict_count", len(conflicts)).
				Msg("checking operation conflicts")
		}

		for _, conflictID := range conflicts {
			if _, exists := mp.idIndex[conflictID]; exists {
				mp.logger.Debug().
					Str("op_id", string(op.ID())).
					Str("conflicting_operation", string(conflictID)).
					Interface("all_conflicts", conflicts).
					Msg("conflict validation failed - conflicting operation found in queue")
				return fmt.Errorf("operation %s conflicts with operation %s", op.ID(), conflictID)
			} else {
				mp.logger.Debug().
					Str("op_id", string(op.ID())).
					Str("potential_conflict", string(conflictID)).
					Msg("potential conflict not in queue - no actual conflict")
			}
		}
	}

	mp.logger.Debug().
		Int("total_potential_conflicts", conflictCount).
		Int("operations_checked", len(mp.ops)).
		Msg("conflict validation completed - no conflicts found")

	return nil
}

// noOpLogger implements core.Logger for when no logger is provided
type noOpLogger struct{}

func (l *noOpLogger) Trace() core.LogEvent { return &noOpLogEvent{} }
func (l *noOpLogger) Debug() core.LogEvent { return &noOpLogEvent{} }
func (l *noOpLogger) Info() core.LogEvent  { return &noOpLogEvent{} }
func (l *noOpLogger) Warn() core.LogEvent  { return &noOpLogEvent{} }
func (l *noOpLogger) Error() core.LogEvent { return &noOpLogEvent{} }

// noOpLogEvent implements core.LogEvent with no-op methods
type noOpLogEvent struct{}

func (e *noOpLogEvent) Str(key, val string) core.LogEvent             { return e }
func (e *noOpLogEvent) Int(key string, val int) core.LogEvent         { return e }
func (e *noOpLogEvent) Bool(key string, val bool) core.LogEvent       { return e }
func (e *noOpLogEvent) Dur(key string, val interface{}) core.LogEvent { return e }
func (e *noOpLogEvent) Interface(key string, val interface{}) core.LogEvent { return e }
func (e *noOpLogEvent) Err(err error) core.LogEvent                   { return e }
func (e *noOpLogEvent) Float64(key string, val float64) core.LogEvent { return e }
func (e *noOpLogEvent) Msg(msg string)                                {}
