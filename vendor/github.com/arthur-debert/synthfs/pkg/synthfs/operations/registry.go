package operations

import (
	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// OperationRegistrar is used to register operation factories with the main package
type OperationRegistrar interface {
	RegisterFactory(factory core.OperationFactory)
}

// registrations holds the functions to be called during init
var registrations []func(OperationRegistrar)

// Register adds a registration function to be called during init
func Register(fn func(OperationRegistrar)) {
	registrations = append(registrations, fn)
}

// Initialize calls all registered functions with the provided registrar
func Initialize(registrar OperationRegistrar) {
	for _, fn := range registrations {
		fn(registrar)
	}
}
