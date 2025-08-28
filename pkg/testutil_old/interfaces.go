package testutil

// TB is a subset of testing.TB interface that we use for our test helpers
// This allows us to use both *testing.T and mock implementations
type TB interface {
	Helper()
	Error(args ...interface{})
	Errorf(format string, args ...interface{})
	Fatal(args ...interface{})
	Fatalf(format string, args ...interface{})
	Skip(args ...interface{})
	Skipf(format string, args ...interface{})
	Log(args ...interface{})
	Logf(format string, args ...interface{})
	TempDir() string
}
