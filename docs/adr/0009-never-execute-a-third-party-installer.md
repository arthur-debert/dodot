# dodot never executes a third-party installer

When a package manager is missing, dodot names it and prints its canonical
project page. It does not install the manager, does not offer to, and does not
run an installer under any flag.

Both Homebrew and Nix ship pipe-to-shell as their official and only supported
install path. A dodot that ran one would be a program that downloads and
executes arbitrary remote code as an ordinary step of deploying dotfiles — which
would make dodot worth compromising as a way to intercept that step, on every
machine that has it installed. Nothing about the convenience of not typing one
command justifies acquiring that shape.

The project URL is a compile-time constant on the provisioner descriptor,
alongside the candidate paths (ADR-0004: configuration selects files, never
executables). This is the operative half of the rule. An attacker who can alter
that constant can already alter everything else dodot does, so holding it in the
binary costs nothing — while withholding it entirely only sends the user to a
search engine, which is a worse place to look up how to install a package
manager than the project's own page.

What dodot must never do is take an install instruction from anywhere else: not
from `.dodot.toml` at any layer, not from a pack, not from the network. A
descriptor field naming an installer command, a shell snippet, or a URL to fetch
is out of bounds by this rule whatever else recommends it, and so is a
`--install-missing` flag: the flag would only move the decision, not remove it.

Running the user's own `install.sh` is not the same act and stays what it is.
That is a file in a repository the user chose to deploy, which they can read
before it runs, and running it is the entire purpose of the `install` handler.
The line is authorship: dodot runs what the user's dotfiles contain, never what
a third party publishes at a URL.

Reasoning: `docs/proposals/provisioning-handlers.lex` §4.5.
