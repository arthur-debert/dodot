//! Tiny interactive-prompt helpers for CLI handlers.
//!
//! All input/output goes through stdin/stderr — stdout is reserved for
//! command output that downstream tools may pipe and parse.

use std::io::{self, BufRead, IsTerminal, Write};

/// Yes/No/Show response to a 3-way prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YesNoShow {
    /// User wants to proceed.
    Yes,
    /// User declined; caller should NOT mark the prompt dismissed
    /// (so it fires again next time).
    No,
    /// User wants to inspect the underlying config without committing.
    Show,
}

/// True if stdin is a TTY. Use to gate interactive prompts.
pub fn stdin_is_tty() -> bool {
    io::stdin().is_terminal()
}

/// Ask a Y/n/show question on stderr and return the parsed response.
///
/// `prompt_lines` are printed verbatim (one per line). The final
/// `[Y/n/show]` marker is appended automatically. Empty input maps to
/// `Yes` (the capital `Y` signals it as the default). Anything
/// unrecognised maps to `No` so a stray keypress doesn't accidentally
/// dismiss state.
pub fn prompt_yes_no_show(prompt_lines: &[&str]) -> io::Result<YesNoShow> {
    let mut stderr = io::stderr().lock();
    for line in prompt_lines {
        writeln!(stderr, "{line}")?;
    }
    write!(stderr, "[Y/n/show] ")?;
    stderr.flush()?;

    let mut buf = String::new();
    let stdin = io::stdin();
    stdin.lock().read_line(&mut buf)?;
    let answer = buf.trim().to_ascii_lowercase();

    Ok(match answer.as_str() {
        "" | "y" | "yes" => YesNoShow::Yes,
        "s" | "show" => YesNoShow::Show,
        _ => YesNoShow::No,
    })
}
