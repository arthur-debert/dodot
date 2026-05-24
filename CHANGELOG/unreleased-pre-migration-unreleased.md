
### Added

- **Docs site infrastructure.** `mkdocs.yml` (Material theme + `mkdocs-lex-plugin`) renders the `.lex` files under `docs/` as a static site. `.github/workflows/docs.yml` deploys on every push to `main` that touches `docs/**` or `mkdocs.yml`, via the `mhausenblas/mkdocs-deploy-gh-pages` action which runs `mkdocs gh-deploy` in a container. Custom domain `dodot.sh` wired in via `docs/CNAME` (Google Cloud DNS zone, A records pointing at GitHub Pages + apex AAAA + `www` CNAME). `docs/requirements.txt` pins the build deps to exact versions for reproducibility. Content wiring is incremental — sample nav covers home + four user-guide pages + three reference pages; the rest of the existing `.lex` files build but aren't yet in the nav.


### Changed

- **Intra-pack handler execution order is now explicit.** Previously ordering was `category → alphabetical by handler name`, which happened to produce the right sequence (homebrew, install, path, shell, symlink) but was fragile — adding a handler with a name sorted earlier alphabetically would have silently reordered the pipeline. Handlers now declare an `ExecutionPhase` (`Provision` → `Setup` → `PathExport` → `ShellInit` → `Link`), and `rules::handler_execution_order` sorts on the enum's declared order. The observable order is unchanged; the contract is now encoded in the type system, and adding a handler requires a deliberate choice of phase. `HandlerCategory` (used by `--no-provision`) is derived from phase. Catchall-last is now enforced by `Link` being the final variant rather than by convention.

