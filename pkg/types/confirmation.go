package types

// ConfirmationRequest represents a request for user confirmation before executing actions
type ConfirmationRequest struct {
	// ID is a unique identifier for this confirmation within the operation
	ID string

	// Pack is the name of the pack this confirmation relates to
	Pack string

	// Handler is the name of the handler requesting confirmation
	Handler string

	// Operation indicates whether this is for "provision" or "clear" operations
	Operation string

	// Title is a brief, user-friendly title describing what needs confirmation
	Title string

	// Description provides detailed information about what will happen
	Description string

	// Items lists specific items that will be affected (packages, files, etc.)
	// This allows for detailed display in confirmation dialogs
	Items []string

	// Default indicates the default response if user just presses enter
	// true = default to "yes", false = default to "no"
	Default bool
}

// ProcessingResult represents the result of handler processing, containing both
// actions to execute and confirmations that need user approval
type ProcessingResult struct {
	// Actions contains all the actions that should be executed
	Actions []Action

	// Confirmations contains requests for user confirmation
	// If empty, no confirmations are needed and actions can be executed directly
	Confirmations []ConfirmationRequest
}

// NewProcessingResult creates a ProcessingResult with just actions (no confirmations)
func NewProcessingResult(actions []Action) ProcessingResult {
	return ProcessingResult{
		Actions:       actions,
		Confirmations: []ConfirmationRequest{},
	}
}

// NewProcessingResultWithConfirmations creates a ProcessingResult with both actions and confirmations
func NewProcessingResultWithConfirmations(actions []Action, confirmations []ConfirmationRequest) ProcessingResult {
	return ProcessingResult{
		Actions:       actions,
		Confirmations: confirmations,
	}
}

// HasConfirmations returns true if this result contains confirmations that need user approval
func (pr ProcessingResult) HasConfirmations() bool {
	return len(pr.Confirmations) > 0
}

// ConfirmationResponse represents a user's response to confirmation requests
type ConfirmationResponse struct {
	// ID matches the ConfirmationRequest.ID
	ID string

	// Approved indicates whether the user approved this confirmation
	Approved bool
}

// ConfirmationContext holds all user responses to confirmation requests
// This is passed through to action execution if confirmations were approved
type ConfirmationContext struct {
	// Responses maps confirmation IDs to user responses
	Responses map[string]bool
}

// NewConfirmationContext creates a new ConfirmationContext from a list of responses
func NewConfirmationContext(responses []ConfirmationResponse) *ConfirmationContext {
	responseMap := make(map[string]bool)
	for _, resp := range responses {
		responseMap[resp.ID] = resp.Approved
	}
	return &ConfirmationContext{
		Responses: responseMap,
	}
}

// IsApproved returns true if the confirmation with the given ID was approved
func (cc *ConfirmationContext) IsApproved(confirmationID string) bool {
	if cc == nil || cc.Responses == nil {
		return false
	}
	return cc.Responses[confirmationID]
}

// AllApproved returns true if all confirmations were approved
func (cc *ConfirmationContext) AllApproved(confirmationIDs []string) bool {
	// Empty list means no confirmations to check, so return true
	if len(confirmationIDs) == 0 {
		return true
	}

	if cc == nil || cc.Responses == nil {
		return false
	}

	for _, id := range confirmationIDs {
		if !cc.Responses[id] {
			return false
		}
	}
	return true
}
