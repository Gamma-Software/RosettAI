//! Reusable terminal rendering, reserved for future CLI integration.
//! Not declared in main.rs yet, so this module does not affect CLI output.
//! Spinner callers should call tick roughly every FRAME_INTERVAL.
use std::io::{self, Write};
use std::time::{Duration, Instant};

pub const ACCENT: &str = "38;2;0;190;218";
pub const FRAME_INTERVAL: Duration = Duration::from_millis(80);
const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn style(text: &str, code: &str, color: bool) -> String {
    if color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.into()
    }
}

/// Rows contain plain text and its ANSI style code.
/// Padding counts Unicode scalars; wide glyphs need display-width handling.
pub fn rectangle(out: &mut impl Write, rows: &[(&str, &str)], color: bool) -> io::Result<()> {
    let width = rows
        .iter()
        .map(|(text, _)| text.chars().count())
        .max()
        .unwrap_or(0)
        + 6;
    writeln!(
        out,
        "  {}",
        style(&format!("╭{}╮", "─".repeat(width)), ACCENT, color)
    )?;
    for &(text, code) in rows {
        writeln!(
            out,
            "  {}   {}{}{}",
            style("│", ACCENT, color),
            style(text, code, color),
            " ".repeat(width - 3 - text.chars().count()),
            style("│", ACCENT, color)
        )?;
    }
    writeln!(
        out,
        "  {}",
        style(&format!("╰{}╯", "─".repeat(width)), ACCENT, color)
    )?;
    out.flush()
}

pub struct Step {
    label: String,
    started: Instant,
    frame: usize,
    live: bool,
}

impl Step {
    /// `live` enables ANSI redraws and colors; NO_COLOR is intentionally ignored.
    pub fn new(label: impl Into<String>, live: bool) -> Self {
        Self {
            label: label.into(),
            started: Instant::now(),
            frame: 0,
            live,
        }
    }

    pub fn tick(&mut self, out: &mut impl Write) -> io::Result<()> {
        if !self.live {
            return Ok(());
        }
        write!(
            out,
            "\r\x1b[2K  {} {}  {}",
            style(&FRAMES[self.frame].to_string(), ACCENT, true),
            self.label,
            style(
                &format!("{:.1}s", self.started.elapsed().as_secs_f32()),
                "2",
                true
            )
        )?;
        self.frame = (self.frame + 1) % FRAMES.len();
        out.flush()
    }

    pub fn finish(self, out: &mut impl Write, success: bool) -> io::Result<()> {
        if self.live {
            write!(out, "\r\x1b[2K")?;
        }
        let (mark, code) = if success {
            ("✓", ACCENT)
        } else {
            ("✗", "31")
        };
        writeln!(
            out,
            "  {} {}  {}",
            style(mark, code, self.live),
            self.label,
            style(
                &format!("{:.1}s", self.started.elapsed().as_secs_f32()),
                "2",
                self.live
            )
        )?;
        out.flush()
    }
}
