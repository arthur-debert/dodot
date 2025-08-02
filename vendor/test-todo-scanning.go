package vendor

// This file tests that vendor directory is properly excluded from todo-to-issue scanning

// TODO: This TODO should NOT create an issue
// FIXME: This FIXME should NOT create an issue
// FIX: This FIX should NOT create an issue
// ISSUE: This ISSUE should NOT create an issue

func TestVendorExclusion() {
	// TODO: Another TODO that should be ignored
	// FIXME: Another FIXME that should be ignored
	// FIX: Another FIX that should be ignored
	// ISSUE: Another ISSUE that should be ignored
}
