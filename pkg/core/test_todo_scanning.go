package core

// This file tests that only FIX, FIXME, and ISSUE create GitHub issues

// TODO: This TODO should NOT create an issue (testing TODO exclusion)
// FIXME: This FIXME SHOULD create an issue with auto-todo label
// FIX: This FIX SHOULD create an issue with auto-todo label
// ISSUE: This ISSUE SHOULD create an issue with auto-todo label

func TestTodoScanning() {
	// Mixed case test:
	// Todo: This lowercase Todo should NOT create an issue
	// todo: This all lowercase todo should NOT create an issue
	// fixme: This lowercase fixme - check if it creates issue
	// FIXME: This uppercase FIXME SHOULD definitely create an issue
}
