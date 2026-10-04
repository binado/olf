//! Condense a TeX `.log` into `file:line: message` diagnostics.
//!
//! Errors come from `-file-line-error` lines (`./sec.tex:12: msg`) and classic
//! `! msg` lines; files for the latter (and for warnings) are tracked through
//! the `(./file.tex ... )` nesting TeX writes as it opens and closes inputs.
//! The engine runs with a large `max_print_line`, so lines aren't wrapped.

use regex::Regex;
use serde::Serialize;
use std::fmt;
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
    /// The offending source line as TeX echoes it (`l.12 \foo`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.file, self.line) {
            (Some(file), Some(line)) => write!(f, "{file}:{line}: ")?,
            (Some(file), None) => write!(f, "{file}: ")?,
            (None, Some(line)) => write!(f, "line {line}: ")?,
            (None, None) => {}
        }
        f.write_str(&self.message)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

static FILE_LINE_ERROR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^((?:\.{1,2})?/[^:]*|[^:\s/][^:\s]*\.[A-Za-z]+):(\d+): (.*)$")
        .expect("valid regex")
});
static CONTEXT_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^l\.(\d+) (.*)$").expect("valid regex"));
static WARNING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(LaTeX Warning|(?:Package|Class) (\S+) Warning): (.*)$").expect("valid regex")
});
static CONTINUATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\(([^)\s]+)\)\s+(.*)$").expect("valid regex"));
static INPUT_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*on input line (\d+)\.?$").expect("valid regex"));
static OVERFULL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(Overfull \\[hv]box \([^)]*\)).*?(?:at lines? (\d+))?(?:--\d+)?$")
        .expect("valid regex")
});

/// Messages that only restate an earlier error.
fn is_followup(message: &str) -> bool {
    message == "Emergency stop." || message.starts_with("==> Fatal error occurred")
}

pub fn parse(log: &str) -> Report {
    let lines: Vec<&str> = log.lines().collect();
    let mut files = FileStack::default();
    let mut report = Report::default();
    let mut followups = Vec::new();
    let mut skip_parens = 0;

    for (i, line) in lines.iter().enumerate() {
        let file = files.current();
        if let Some(c) = FILE_LINE_ERROR.captures(line) {
            let message = c[3].trim().to_owned();
            let diagnostic = Diagnostic {
                file: Some(normalize(&c[1])),
                line: c[2].parse().ok(),
                context: context_after(&lines, i).map(|(_, ctx)| ctx),
                message,
            };
            if is_followup(&diagnostic.message) {
                followups.push(diagnostic);
            } else {
                report.errors.push(diagnostic);
            }
        } else if let Some(message) = line.strip_prefix("! ") {
            let context = context_after(&lines, i);
            let diagnostic = Diagnostic {
                file,
                line: context.as_ref().map(|(n, _)| *n),
                context: context.map(|(_, ctx)| ctx),
                message: message.trim().to_owned(),
            };
            if is_followup(&diagnostic.message) {
                followups.push(diagnostic);
            } else {
                report.errors.push(diagnostic);
            }
        } else if let Some(c) = WARNING.captures(line) {
            let mut message = c[3].trim().to_owned();
            for next in &lines[i + 1..] {
                match CONTINUATION.captures(next) {
                    Some(cont) => {
                        message.push(' ');
                        message.push_str(cont[2].trim());
                    }
                    None => break,
                }
            }
            let line = INPUT_LINE
                .captures(&message)
                .and_then(|m| m[1].parse().ok());
            let message = INPUT_LINE.replace(&message, "").into_owned();
            let message = match c.get(2) {
                Some(package) => format!("{}: {message}", package.as_str()),
                None => message,
            };
            push_unique(
                &mut report.warnings,
                Diagnostic {
                    file,
                    line,
                    message,
                    context: None,
                },
            );
        } else if let Some(c) = OVERFULL.captures(line) {
            push_unique(
                &mut report.warnings,
                Diagnostic {
                    file,
                    line: c.get(2).and_then(|m| m.as_str().parse().ok()),
                    message: c[1].to_owned(),
                    context: None,
                },
            );
        }

        // Source text echoed in error context may hold unbalanced parens.
        if CONTEXT_LINE.is_match(line) {
            skip_parens = 2;
        }
        if skip_parens > 0 {
            skip_parens -= 1;
        } else {
            files.scan(line);
        }
    }

    if report.errors.is_empty() {
        report.errors = followups;
    }
    report
}

fn push_unique(list: &mut Vec<Diagnostic>, diagnostic: Diagnostic) {
    if !list.contains(&diagnostic) {
        list.push(diagnostic);
    }
}

/// The `l.<n> <source>` line following an error, before the next error.
fn context_after(lines: &[&str], i: usize) -> Option<(u32, String)> {
    lines[i + 1..]
        .iter()
        .take(12)
        .take_while(|l| !starts_new_error(l))
        .find_map(|l| {
            let c = CONTEXT_LINE.captures(l)?;
            Some((
                c[1].parse().ok()?,
                format!("l.{} {}", &c[1], c[2].trim_end()),
            ))
        })
}

/// Whether `line` opens a new error (follow-ups belong to the previous one).
fn starts_new_error(line: &str) -> bool {
    if let Some(message) = line.strip_prefix("! ") {
        return !is_followup(message.trim());
    }
    FILE_LINE_ERROR
        .captures(line)
        .is_some_and(|c| !is_followup(c[3].trim()))
}

fn normalize(path: &str) -> String {
    path.strip_prefix("./").unwrap_or(path).to_owned()
}

/// Files TeX currently has open, from the parentheses in the log.
#[derive(Default)]
struct FileStack(Vec<Option<String>>);

impl FileStack {
    fn current(&self) -> Option<String> {
        self.0.iter().rev().find_map(Clone::clone)
    }

    fn scan(&mut self, line: &str) {
        let mut rest = line;
        while let Some(pos) = rest.find(['(', ')']) {
            if rest.as_bytes()[pos] == b')' {
                self.0.pop();
                rest = &rest[pos + 1..];
                continue;
            }
            let after = &rest[pos + 1..];
            let end = after
                .find(|c: char| c.is_whitespace() || c == '(' || c == ')')
                .unwrap_or(after.len());
            let token = &after[..end];
            let is_file = ["./", "../", "/"].iter().any(|p| token.starts_with(p));
            self.0.push(is_file.then(|| normalize(token)));
            rest = &after[end..];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/logs/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(path).unwrap()
    }

    fn diag(file: &str, line: u32, message: &str) -> Diagnostic {
        Diagnostic {
            file: Some(file.into()),
            line: Some(line),
            message: message.into(),
            context: None,
        }
    }

    #[test]
    fn file_line_errors_with_context() {
        let report = parse(&fixture("undefined-control-sequence.log"));
        assert_eq!(
            report.errors,
            [Diagnostic {
                context: Some("l.2 Some text \\undefinedmacro".into()),
                ..diag("sections/intro.tex", 2, "Undefined control sequence.")
            }]
        );
        assert_eq!(
            report.errors[0].to_string(),
            "sections/intro.tex:2: Undefined control sequence."
        );
        // The warning before the \input is attributed to main.tex.
        assert_eq!(
            report.warnings,
            [diag(
                "main.tex",
                4,
                "Reference `missing' on page 1 undefined"
            )]
        );
    }

    #[test]
    fn bang_errors_use_file_stack_and_context_line() {
        let report = parse(&fixture("no-file-line-error.log"));
        assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
        assert_eq!(
            report.errors[0].to_string(),
            "sections/intro.tex:2: Undefined control sequence."
        );
    }

    #[test]
    fn missing_package_drops_emergency_stop() {
        let report = parse(&fixture("missing-package.log"));
        assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
        assert_eq!(
            report.errors[0].to_string(),
            "main.tex:3: LaTeX Error: File `doesnotexist.sty' not found."
        );
    }

    #[test]
    fn warnings_with_files_lines_and_continuations() {
        let report = parse(&fixture("warnings.log"));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        let warnings: Vec<String> = report.warnings.iter().map(ToString::to_string).collect();
        assert_eq!(
            warnings,
            [
                "main.tex:4: Reference `nope' on page 1 undefined",
                "main.tex:4: Citation `knuth' on page 1 undefined",
                "main.tex:5: Overfull \\hbox (174.65878pt too wide)",
                "main.tex:6: hyperref: Token not allowed in a PDF string (Unicode): removing `math shift'",
                "main.tex:6: hyperref: Token not allowed in a PDF string (Unicode): removing `superscript'",
                "sub.tex:1: Reference `alsomissing' on page 1 undefined",
                "main.tex: There were undefined references.",
            ]
        );
    }

    #[test]
    fn followups_are_kept_when_alone() {
        let report =
            parse("./main.tex:3:  ==> Fatal error occurred, no output PDF file produced!\n");
        assert_eq!(report.errors.len(), 1);
    }

    #[test]
    fn file_stack_ignores_non_file_parens() {
        let mut stack = FileStack::default();
        stack.scan("(./main.tex (/usr/share/article.cls (Font) x) (e.g., y)");
        assert_eq!(stack.current().as_deref(), Some("main.tex"));
        stack.scan("(./sec/a.tex [1] )");
        assert_eq!(stack.current().as_deref(), Some("main.tex"));
        stack.scan(") ) )");
        assert_eq!(stack.current(), None);
    }
}
