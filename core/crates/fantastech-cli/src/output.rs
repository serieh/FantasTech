//! Standardized, cross-platform terminal message formatting.
//!
//! Provides clean, professional CLI status prefixes (`[INFO]`, `[OK]`, `[WARN]`, `[ERROR]`)
//! with ANSI color highlighting on interactive terminals (TTY) and automatic plain-text
//! fallback when piped or running in non-interactive environments.

use std::io::IsTerminal;

/// Checks if standard output is an interactive terminal.
#[inline]
pub fn is_stdout_tty() -> bool {
    std::io::stdout().is_terminal()
}

/// Checks if standard error is an interactive terminal.
#[inline]
pub fn is_stderr_tty() -> bool {
    std::io::stderr().is_terminal()
}

/// Returns the formatted `[INFO]` tag.
#[inline]
pub fn tag_info() -> &'static str {
    if is_stdout_tty() {
        "\x1b[1;36m[INFO]\x1b[0m"
    } else {
        "[INFO]"
    }
}

/// Returns the formatted `[OK]` tag.
#[inline]
pub fn tag_ok() -> &'static str {
    if is_stdout_tty() {
        "\x1b[1;32m[OK]\x1b[0m"
    } else {
        "[OK]"
    }
}

/// Returns the formatted `[WARN]` tag.
#[inline]
pub fn tag_warn() -> &'static str {
    if is_stderr_tty() {
        "\x1b[1;33m[WARN]\x1b[0m"
    } else {
        "[WARN]"
    }
}

/// Returns the formatted `[ERROR]` tag.
#[inline]
pub fn tag_error() -> &'static str {
    if is_stderr_tty() {
        "\x1b[1;31m[ERROR]\x1b[0m"
    } else {
        "[ERROR]"
    }
}

/// Prints an informational message to stdout with standard `[INFO]` prefix.
#[macro_export]
macro_rules! cli_info {
    ($($arg:tt)*) => {
        println!("{} {}", $crate::output::tag_info(), format_args!($($arg)*))
    };
}

/// Prints a success message to stdout with standard `[OK]` prefix.
#[macro_export]
macro_rules! cli_ok {
    ($($arg:tt)*) => {
        println!("{} {}", $crate::output::tag_ok(), format_args!($($arg)*))
    };
}

/// Prints a warning message to stderr with standard `[WARN]` prefix.
#[macro_export]
macro_rules! cli_warn {
    ($($arg:tt)*) => {
        eprintln!("{} {}", $crate::output::tag_warn(), format_args!($($arg)*))
    };
}

/// Prints an error message to stderr with standard `[ERROR]` prefix.
#[macro_export]
macro_rules! cli_error {
    ($($arg:tt)*) => {
        eprintln!("{} {}", $crate::output::tag_error(), format_args!($($arg)*))
    };
}
