module github.com/arthur-debert/dodot

go 1.23

require (
	github.com/adrg/xdg v0.5.3
	github.com/arthur-debert/synthfs v0.0.0-00010101000000-000000000000
	github.com/pelletier/go-toml/v2 v2.2.4
	github.com/rs/zerolog v1.34.0
	github.com/spf13/cobra v1.8.0
	github.com/stretchr/testify v1.10.0
)

replace github.com/arthur-debert/synthfs => ./local/third-parties/go-synthfs

require (
	github.com/cpuguy83/go-md2man/v2 v2.0.3 // indirect
	github.com/davecgh/go-spew v1.1.1 // indirect
	github.com/gammazero/toposort v0.1.1 // indirect
	github.com/inconshreveable/mousetrap v1.1.0 // indirect
	github.com/mattn/go-colorable v0.1.13 // indirect
	github.com/mattn/go-isatty v0.0.19 // indirect
	github.com/pmezard/go-difflib v1.0.0 // indirect
	github.com/russross/blackfriday/v2 v2.1.0 // indirect
	github.com/spf13/pflag v1.0.5 // indirect
	golang.org/x/sys v0.26.0 // indirect
	gopkg.in/yaml.v3 v3.0.1 // indirect
)
