package paths

import (
	"fmt"
	"path/filepath"
	"testing"
)

func BenchmarkPathsCreation(b *testing.B) {
	b.Run("New with explicit root", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			_, _ = New("/test/dotfiles")
		}
	})

	b.Run("New with env detection", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			_, _ = New("")
		}
	})
}

func BenchmarkPathOperations(b *testing.B) {
	p, _ := New("/test/dotfiles")

	b.Run("PackPath", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			_ = p.PackPath("vim")
		}
	})

	b.Run("StatePath", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			_ = p.StatePath("vim", "provision")
		}
	})

	b.Run("DeployedDir", func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			_ = p.DeployedDir()
		}
	})
}

func BenchmarkPathNormalization(b *testing.B) {
	p, _ := New("/test/dotfiles")

	testPaths := []string{
		"/simple/path",
		"~/dotfiles/vim/vimrc",
		"/path/../other/path",
		"relative/path/file.txt",
	}

	for _, path := range testPaths {
		b.Run(fmt.Sprintf("normalize_%s", filepath.Base(path)), func(b *testing.B) {
			for i := 0; i < b.N; i++ {
				_, _ = p.NormalizePath(path)
			}
		})
	}
}

func BenchmarkIsInDotfiles(b *testing.B) {
	p, _ := New("/test/dotfiles")

	testCases := []struct {
		name string
		path string
	}{
		{"inside", "/test/dotfiles/vim/vimrc"},
		{"outside", "/etc/passwd"},
		{"parent", "/test"},
		{"traversal", "/test/dotfiles/../outside/file"},
	}

	for _, tc := range testCases {
		b.Run(tc.name, func(b *testing.B) {
			for i := 0; i < b.N; i++ {
				_, _ = p.IsInDotfiles(tc.path)
			}
		})
	}
}

func BenchmarkExpandHome(b *testing.B) {
	testPaths := []string{
		"~",
		"~/dotfiles",
		"~/path/to/file",
		"/absolute/path",
		"relative/path",
		"~other/path",
	}

	for _, path := range testPaths {
		b.Run(path, func(b *testing.B) {
			for i := 0; i < b.N; i++ {
				_ = ExpandHome(path)
			}
		})
	}
}

func BenchmarkPathsAPI(b *testing.B) {
	// Benchmark the Paths API methods

	b.Run("Paths.DataDir", func(b *testing.B) {
		p, _ := New("")
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_ = p.DataDir()
		}
	})

	b.Run("Paths.DeployedDir", func(b *testing.B) {
		p, _ := New("")
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_ = p.DeployedDir()
		}
	})

	b.Run("Paths.HomebrewDir", func(b *testing.B) {
		p, _ := New("")
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_ = p.HomebrewDir()
		}
	})
}

func BenchmarkConcurrentPathAccess(b *testing.B) {
	// Test performance under concurrent access
	p, _ := New("/test/dotfiles")

	b.RunParallel(func(pb *testing.PB) {
		for pb.Next() {
			_ = p.PackPath("vim")
			_ = p.DataDir()
			_ = p.DeployedDir()
			_ = p.StatePath("vim", "provision")
		}
	})
}
