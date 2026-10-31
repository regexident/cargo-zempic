//! The human-readable report.
//!
//! The renderer is a pure function of a [`Report`] and its display inputs: it
//! takes color and width as parameters rather than reading the environment or
//! terminal, so the same report renders identically wherever it is called from.
//!
//! Paths are printed relative to the workspace root, matching the JSON
//! document's `file` field; a path outside the root is printed as its absolute
//! form. The renderer emits no size figure: the only numbers it writes are the
//! tier number, the finding counts, the header's member and selected counts,
//! and values that come verbatim out of a finding's own fields.

use yansi::{Condition, Paint};

use crate::Resolution;
use crate::finding::{Finding, Magnitude, Severity, Tier};
use crate::profile::ResolvedProfile;
use crate::report::{Report, severity_style};

/// Render `report` as the human-readable form.
///
/// `color` decides whether ANSI escape sequences are emitted, and `width` is
/// the column budget the header and wrapped prose are laid out to.
///
/// `root` is the path printed in place of
/// [`resolution.workspace_root`](crate::Resolution::workspace_root). A finding
/// quotes a file's path, and that file lives under the root, so the root's text
/// reaches the wrapped prose. Taking it as a parameter lets a caller render
/// against a stand-in of its choosing, rather than substituting the root's text
/// after wrapping, where the change would land on whatever column the original
/// length happened to reach. Pass the workspace root itself to render the
/// report as the analysis recorded it.
pub fn render(report: &Report, color: bool, width: usize, root: &camino::Utf8Path) -> String {
    let mut output = render_header(&report.resolution, color, root);

    if report.is_empty() {
        output.push_str("No findings.\n");
        output.push_str(&render_legend(width, color));

        return output;
    }

    output.push_str(&render_tiers(report, color, width));
    output.push_str(&render_conflicts(report, color));
    output.push_str(&render_summary(report, color));
    output.push_str(&render_legend(width, color));

    output
}

/// Render `report` as one line per finding, with the summary and the legend.
///
/// The quiet form drops the header block, the tier headings and each finding's
/// detail, tradeoff and metadata line; the summary and the legend remain,
/// because the legend is mandatory in every renderer. Conflicts are rendered as
/// in the full form: a conflict is not a finding and the block is the only form
/// it has.
///
/// The rule identifiers share one column, wide enough for the longest among the
/// findings, so the columns after them line up down the listing.
pub fn render_quiet(report: &Report, color: bool, width: usize) -> String {
    let mut output = String::new();

    if report.is_empty() {
        output.push_str("No findings.\n");
        output.push_str(&render_legend(width, color));

        return output;
    }

    let rule_width = report
        .findings
        .iter()
        .map(|finding| finding.rule.to_string().chars().count())
        .max()
        .unwrap_or_default();

    for finding in &report.findings {
        output.push_str(&render_quiet_line(
            finding,
            &report.resolution,
            color,
            rule_width,
        ));
    }

    output.push_str(&render_conflicts(report, color));
    output.push_str(&render_summary(report, color));
    output.push_str(&render_legend(width, color));

    output
}

/// The columns a quiet line pads the severity word to.
const QUIET_SEVERITY_WIDTH: usize = 6;

/// The columns a quiet line pads the magnitude to.
const QUIET_MAGNITUDE_WIDTH: usize = 10;

/// The quiet line's spacing: one space after the bullet and two between every
/// two columns after it.
const QUIET_GAP: usize = 2;

/// The quiet line's columns for one finding.
///
/// The columns are held unpadded and unpainted: each is measured as it is,
/// padded on those measurements, and painted only once the line is laid out, so
/// the escape sequences of a styled word never count as columns.
struct QuietFields {
    severity: String,
    magnitude: String,
    rule: String,
    location: String,
    change: String,
}

impl QuietFields {
    /// The columns for `finding`, with the location written relative to the
    /// workspace root.
    fn new(finding: &Finding, resolution: &Resolution) -> Self {
        let location = finding
            .location
            .as_ref()
            .map_or_else(String::new, |location| {
                format!(
                    "{}:{}",
                    relativize(&location.span.file, resolution),
                    location.span.line
                )
            });

        let change = finding.change.as_ref().map_or_else(String::new, |change| {
            format!("{} -> {}", change.current, change.recommended)
        });

        Self {
            severity: finding.severity.as_str().to_uppercase(),
            magnitude: finding.magnitude.as_str().to_owned(),
            rule: finding.rule.to_string(),
            location,
            change,
        }
    }

    /// The line with the bullet, the severity padded to [`QUIET_SEVERITY_WIDTH`],
    /// the magnitude padded to [`QUIET_MAGNITUDE_WIDTH`], the rule padded to
    /// `rule_width`, and the location and change columns that are present, each
    /// separated by two spaces and with no trailing whitespace.
    ///
    /// The bullet and severity take `style` and the rule is bold; the padding
    /// and the other columns are plain. A rule shorter than the column has its
    /// padding dropped when it ends the line, so no line ends on a space. The
    /// location and change columns follow the rule rather than a fixed offset,
    /// because the location is as wide as the path it names.
    fn render(&self, rule_width: usize, style: yansi::Style, color: bool) -> String {
        let condition = Condition::cached(color);

        let bullet = "●".paint(style).whenever(condition).to_string();
        let severity = with_padding(&self.severity, QUIET_SEVERITY_WIDTH);
        let severity = severity.paint(style).whenever(condition).to_string();
        let magnitude = with_padding(&self.magnitude, QUIET_MAGNITUDE_WIDTH);
        let trailing = self.location.is_empty() && self.change.is_empty();
        let rule_width = if trailing {
            self.rule.chars().count()
        } else {
            rule_width
        };
        let rule = with_padding(&self.rule, rule_width);
        let rule = rule.bold().whenever(condition).to_string();

        let mut columns = vec![bullet, severity, magnitude, rule];

        if !self.location.is_empty() {
            columns.push(self.location.clone());
        }

        if !self.change.is_empty() {
            columns.push(self.change.clone());
        }

        let mut line = String::new();

        for (index, column) in columns.iter().enumerate() {
            if index > 0 {
                let gap = if index == 1 { 1 } else { QUIET_GAP };

                line.push_str(&" ".repeat(gap));
            }

            line.push_str(column);
        }

        line
    }
}

/// `text` padded with spaces to `width` visible columns.
fn with_padding(text: &str, width: usize) -> String {
    format!("{:<width$}", text)
}

/// Render one quiet line: the bullet, severity, magnitude, rule, location and
/// change columns, each separated by two spaces.
fn render_quiet_line(
    finding: &Finding,
    resolution: &Resolution,
    color: bool,
    rule_width: usize,
) -> String {
    let fields = QuietFields::new(finding, resolution);
    let line = fields.render(rule_width, severity_style(finding.severity), color);

    format!("{line}\n")
}

/// Render the report header naming where the analysis ran.
///
/// The header states the workspace and its member counts, the profile with its
/// `inherits` chain, the selected dimensions and the target triple. It carries
/// no size figure: the counts are facts about the report itself. `root` names
/// the workspace, as [`render`] documents.
fn render_header(resolution: &Resolution, color: bool, root: &camino::Utf8Path) -> String {
    let mut output = String::new();

    output.push_str(&format!("cargo-zempic {}\n", env!("CARGO_PKG_VERSION")));

    let dimensions: Vec<&str> = resolution
        .dimensions
        .iter()
        .map(|dimension| dimension.as_str())
        .collect();

    let lines = [
        (
            "workspace:",
            format!(
                "{}  ({} members, {} selected)",
                root,
                resolution.member_count,
                resolution.selected.len()
            ),
        ),
        (
            "profile:",
            format!(
                "{}  (inherits: {})",
                resolution.profile.name,
                render_inherits(&resolution.profile)
            ),
        ),
        ("optimizing for:", dimensions.join(", ")),
        ("target:", resolution.target_triple.clone()),
    ];

    for (label, value) in lines {
        output.push_str(&render_header_line(label, &value, color));
    }

    output
}

/// Render one header line: two leading spaces, the dim label padded to
/// [`HEADER_LABEL_WIDTH`] columns, and the value.
///
/// Padding is measured on the label alone and never below one space, so a label
/// wider than the column keeps its separation from the value and lets the line
/// overflow instead of running the two together.
fn render_header_line(label: &str, value: &str, color: bool) -> String {
    let padding = HEADER_LABEL_WIDTH
        .saturating_sub(label.chars().count())
        .max(1);
    let label = label.dim().whenever(Condition::cached(color)).to_string();

    let mut line = String::from("  ");
    line.push_str(&label);
    for _ in 0..padding {
        line.push(' ');
    }
    line.push_str(value);
    line.push('\n');
    line
}

/// Render a profile's `inherits` chain as `parent -> ... -> root`, or `none`
/// when the profile inherits from no other.
fn render_inherits(profile: &ResolvedProfile) -> String {
    if profile.inherits_chain.is_empty() {
        "none".to_owned()
    } else {
        profile.inherits_chain.join(" -> ")
    }
}

/// Render one section per non-empty tier, in [`Tier::ALL`] order.
fn render_tiers(report: &Report, color: bool, width: usize) -> String {
    let mut output = String::new();

    for (index, tier) in Tier::ALL.iter().enumerate() {
        let findings: Vec<&Finding> = report.findings_in(*tier).collect();

        if findings.is_empty() {
            continue;
        }

        output.push('\n');
        output.push_str(&render_tier_heading(
            index + 1,
            *tier,
            findings.len(),
            color,
            width,
        ));

        for finding in findings {
            output.push('\n');
            output.push_str(&render_finding(finding, report, color, width));
        }
    }

    output
}

/// Render a tier heading ruled out to `width` columns.
///
/// The heading is a dim rule segment, the bold tier label and name, a dim run of
/// `─` and the finding count. The run is measured on the unpainted text, so
/// color never changes the visible width, and it keeps at least
/// [`RULE_MIN_WIDTH`] characters; a label and count too long for the width then
/// overflow the line rather than collapse the rule.
fn render_tier_heading(
    number: usize,
    tier: Tier,
    count: usize,
    color: bool,
    width: usize,
) -> String {
    let label = format!("TIER {number} · {}", tier.title().to_uppercase());
    let counted = if count == 1 {
        "1 finding".to_owned()
    } else {
        format!("{count} findings")
    };

    let fixed = "── ".chars().count() + label.chars().count() + 1 + 1 + counted.chars().count();
    let rule_width = width.saturating_sub(fixed).max(RULE_MIN_WIDTH);

    let condition = Condition::cached(color);
    let lead = "── ".dim().whenever(condition).to_string();
    let label = label.bold().whenever(condition).to_string();
    let rule = "─".repeat(rule_width).dim().whenever(condition).to_string();

    format!("{lead}{label} {rule} {counted}\n")
}

/// The shortest run of `─` a tier heading draws, so the heading still reads as a
/// rule beside an overlong label.
const RULE_MIN_WIDTH: usize = 3;

/// Render one finding's block.
///
/// The block is a title line carrying the severity, the rule identifier and the
/// magnitude, then one labeled row per fact the finding states, behind a `│`
/// gutter; the last row closes the outline with `╰` beside its text. Every block
/// therefore opens with the severity's bullet, runs through `│` and ends with
/// `╰` on the line that holds its final words. A row that wraps repeats its
/// glyph on every continuation line, so the left border runs unbroken and never
/// reopens after it closes.
/// Rows wrap to `width`, and every visible length is measured on unpainted text,
/// so color never moves a column. When `color` is true the glyphs and the
/// severity take the severity's style, the rule identifier is bold, and the row
/// labels, the `at` row's key and the `cost` row's field names and separators
/// are dim.
fn render_finding(finding: &Finding, report: &Report, color: bool, width: usize) -> String {
    let style = severity_style(finding.severity);

    let severity = finding.severity.as_str().to_uppercase();
    let rule = finding.rule.to_string();

    let mut output = render_title(finding, &severity, &rule, style, color, width);

    let rows = finding_rows(finding, report);
    let last = rows.len().saturating_sub(1);

    for (index, row) in rows.iter().enumerate() {
        let last_row = index == last;
        let glyph = if last_row { "╰" } else { "│" };

        output.push_str(&render_row(glyph, last_row, row, style, color, width));
    }

    output
}

/// The visible columns before a row's value: two leading spaces, the gutter
/// glyph, a space, a ten-column label and two separating spaces.
///
/// Public so a caller can state the wrap budget a given width produces without
/// duplicating the layout arithmetic.
pub const VALUE_COLUMN: usize = 14;

/// The columns a row label occupies, excluding the spaces after it.
const LABEL_WIDTH: usize = 10;

/// The visible columns a continuation line spends before its value: two leading
/// spaces, the repeated gutter glyph and the space after it. A continuation
/// mirrors the first line's gutter and blanks the label and its separator, so
/// both reach the value column.
const GUTTER_COLUMN: usize = 4;

/// The columns a header label occupies, excluding the space after it.
///
/// Wide enough for `optimizing for:`, the longest label, so a value begins in
/// the same column whatever label precedes it; the longest label leaves one
/// separating space.
const HEADER_LABEL_WIDTH: usize = 16;

/// One labeled row of a finding's block.
///
/// `value` is the row's text as laid out, and `dim` the character ranges of it
/// painted dim. Keeping the two apart lets wrapping and padding measure the
/// value alone, whatever parts of it carry a style; the `at` and `cost` rows
/// name their file, key, field names and separators by range.
struct Row {
    label: &'static str,
    value: String,
    dim: Vec<std::ops::Range<usize>>,
}

impl Row {
    /// A row whose whole value is plain.
    fn plain(label: &'static str, value: impl Into<String>) -> Self {
        Self {
            label,
            value: value.into(),
            dim: Vec::new(),
        }
    }

    /// A row whose value is built by concatenating `parts`, each paired with
    /// whether it is painted dim.
    fn styled(label: &'static str, parts: &[(String, bool)]) -> Self {
        let mut value = String::new();

        for (text, _) in parts {
            value.push_str(text);
        }

        Self {
            label,
            dim: dim_ranges(parts),
            value,
        }
    }
}

/// The character ranges of the concatenated `parts` that are painted dim.
///
/// Adjacent dim parts yield one range, so a style change is never opened and
/// closed around no text.
fn dim_ranges(parts: &[(String, bool)]) -> Vec<std::ops::Range<usize>> {
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut offset = 0;

    for (text, dim) in parts {
        let length = text.chars().count();

        if *dim && length > 0 {
            match ranges.last_mut() {
                Some(range) if range.end == offset => range.end += length,
                _ => ranges.push(offset..offset + length),
            }
        }

        offset += length;
    }

    ranges
}

/// Render `text` with the dim ranges of `dim` in the dim style.
///
/// Only the text inside a range carries escape sequences; every other column is
/// written through unchanged.
fn paint_dim(text: &str, dim: &[std::ops::Range<usize>]) -> String {
    let mut output = String::new();
    let mut position = 0;

    for range in dim {
        output.extend(text.chars().skip(position).take(range.start - position));

        let styled: String = text
            .chars()
            .skip(range.start)
            .take(range.end - range.start)
            .collect();

        output.push_str(&styled.dim().to_string());

        position = range.end;
    }

    output.extend(text.chars().skip(position));

    output
}

/// Render the title line: the glyph, the severity and the rule identifier on the
/// left, the magnitude right-aligned to `width`.
///
/// The head is measured on the unpainted severity and rule, so the gap to the
/// magnitude depends on the visible text alone. The gap is at least two spaces; a
/// head too long for that lets the line overflow, because dropping the magnitude
/// or shortening the rule identifier would hide what the finding is.
fn render_title(
    finding: &Finding,
    severity: &str,
    rule: &str,
    style: yansi::Style,
    color: bool,
    width: usize,
) -> String {
    let condition = Condition::cached(color);
    let glyph = "╭".paint(style).whenever(condition).to_string();
    let styled_severity = severity.paint(style).whenever(condition).to_string();
    let styled_rule = rule.bold().whenever(condition).to_string();

    let magnitude = finding.magnitude.as_str();
    let head_width = 2 + 1 + 1 + severity.chars().count() + 2 + rule.chars().count();
    let padding = width
        .saturating_sub(head_width + magnitude.chars().count())
        .max(2);

    format!(
        "  {glyph} {styled_severity}  {styled_rule}{}{magnitude}\n",
        " ".repeat(padding)
    )
}

/// Render one row of a finding's block, wrapping its value to the columns right
/// of the value column.
///
/// The first line carries the gutter glyph and the label; continuation lines
/// hold spaces where those sit, so every line's value starts in one column.
/// They carry a `│` unless the row is the block's last, whose final continuation
/// line closes the outline with `╰`. The corner therefore never shares a line
/// with a `│`, and never opens the border again once it has closed it.
fn render_row(
    glyph: &str,
    closes: bool,
    row: &Row,
    style: yansi::Style,
    color: bool,
    width: usize,
) -> String {
    let condition = Condition::cached(color);
    let label = format!(
        "{}{}",
        row.label.dim().whenever(condition),
        " ".repeat(LABEL_WIDTH - row.label.chars().count())
    );

    let indent = VALUE_COLUMN;
    let budget = width.saturating_sub(indent).max(1);
    let lines = wrap_ranges(&row.value, budget);

    // Every glyph of a block is painted in the finding's severity style: the
    // row's own glyph, the `│` of its continuations and the `╰` that closes it.
    // A row that wraps opens with `│` whatever glyph it was given, so the corner
    // falls on its last line alone.
    let paint = |text: &str| text.paint(style).whenever(condition).to_string();
    let opens_with_corner = !closes || lines.len() <= 1;
    let first = if opens_with_corner {
        paint(glyph)
    } else {
        paint("│")
    };
    if lines.is_empty() {
        return format!("  {first} {label}\n");
    }

    let mut output = format!(
        "  {first} {label}{}\n",
        render_line(&row.value, &row.dim, &lines[0], color)
    );

    let continuation = paint("│");
    let corner = paint("╰");
    let padding = " ".repeat(indent + 1 - GUTTER_COLUMN);

    for range in lines.iter().skip(1) {
        let closing = closes && lines.last() == Some(range);
        let glyph = if closing { &corner } else { &continuation };

        output.push_str("  ");
        output.push_str(glyph);
        output.push_str(&padding);
        output.push_str(&render_line(&row.value, &row.dim, range, color));
        output.push('\n');
    }

    output
}

/// Render the characters of `text` in `range`, dim across the parts of `dim`
/// that the range holds.
///
/// A dim range cut by the line is painted over the part the line keeps, so every
/// line carries balanced escape sequences.
fn render_line(
    text: &str,
    dim: &[std::ops::Range<usize>],
    range: &std::ops::Range<usize>,
    color: bool,
) -> String {
    let line: String = text
        .chars()
        .skip(range.start)
        .take(range.end - range.start)
        .collect();

    if !color {
        return line;
    }

    let clipped: Vec<std::ops::Range<usize>> = dim
        .iter()
        .filter(|piece| piece.start < range.end && piece.end > range.start)
        .map(|piece| {
            piece.start.max(range.start) - range.start..piece.end.min(range.end) - range.start
        })
        .collect();

    paint_dim(&line, &clipped)
}

/// Split `text` into line ranges over its characters, wrapping on whitespace to
/// at most `budget` columns.
///
/// Each range covers a line's visible text without the whitespace that ended it;
/// a word longer than the budget occupies a line unbroken. Every character falls
/// in some range except the whitespace between two lines, so a caller slicing
/// the text reproduces it without the breaks.
fn wrap_ranges(text: &str, budget: usize) -> Vec<std::ops::Range<usize>> {
    let characters: Vec<char> = text.chars().collect();
    let mut lines: Vec<std::ops::Range<usize>> = Vec::new();
    let mut start: Option<usize> = None;
    let mut end = 0;
    let mut index = 0;

    while index < characters.len() {
        if characters[index].is_whitespace() {
            index += 1;

            continue;
        }

        let word_start = index;

        while index < characters.len() && !characters[index].is_whitespace() {
            index += 1;
        }

        match start {
            None => {
                start = Some(word_start);
                end = index;
            }
            Some(line_start) => {
                if index - line_start <= budget {
                    end = index;
                } else {
                    lines.push(line_start..end);
                    start = Some(word_start);
                    end = index;
                }
            }
        }
    }

    if let Some(line_start) = start {
        lines.push(line_start..end);
    }

    lines
}

/// The labeled rows of a finding's block, in report order.
///
/// A row is absent when its finding field is: no package, no change, no
/// location and no advice each drop their row. The cost row and the tradeoff are
/// always present, because every finding carries them.
fn finding_rows(finding: &Finding, report: &Report) -> Vec<Row> {
    let mut rows = Vec::new();

    if let Some(package) = &finding.package {
        rows.push(Row::plain("package", package.as_str()));
    }

    if let Some(change) = &finding.change {
        rows.push(Row::plain(
            "change",
            format!("{} -> {}", change.current, change.recommended),
        ));
    }

    if let Some(location) = &finding.location {
        rows.push(Row::styled(
            "at",
            &[
                (
                    format!(
                        "{}:{}",
                        relativize(&location.span.file, &report.resolution),
                        location.span.line
                    ),
                    false,
                ),
                ("  ".to_owned(), false),
                (location.span.key.clone(), true),
            ],
        ));
    }

    let mut cost = vec![
        ("effort".to_owned(), true),
        (" ".to_owned(), false),
        (finding.effort.as_str().to_owned(), false),
        (" · ".to_owned(), true),
        ("runtime".to_owned(), true),
        (" ".to_owned(), false),
        (finding.runtime_cost.as_str().to_owned(), false),
    ];

    // A finding no automated fix exists for states nothing by naming its
    // fixability, so the field is left out rather than reported as `report-only`.
    if let Some(fixability) = finding.fixability.reported() {
        cost.push((" · ".to_owned(), true));
        cost.push(("fix".to_owned(), true));
        cost.push((" ".to_owned(), false));
        cost.push((fixability.to_owned(), false));
    }

    cost.push((" · ".to_owned(), true));
    cost.push(("confidence".to_owned(), true));
    cost.push((" ".to_owned(), false));
    cost.push((finding.confidence.as_str().to_owned(), false));

    rows.push(Row::styled("cost", &cost));
    rows.push(Row::plain("why", finding.detail.as_str()));

    if let Some(advice) = &finding.advice {
        rows.push(Row::plain("advice", advice.as_str()));
    }

    rows.push(Row::plain("tradeoff", finding.tradeoff.as_str()));

    rows
}

/// Render a finding's file relative to the workspace root.
///
/// Falls back to the absolute path when the file lies outside the root, which
/// is the case a path such as `$CARGO_HOME/config.toml` reaches.
fn relativize(file: &camino::Utf8Path, resolution: &Resolution) -> String {
    file.strip_prefix(&resolution.workspace_root)
        .map(camino::Utf8Path::to_owned)
        .unwrap_or_else(|_| file.to_owned())
        .into_string()
}

/// Render the conflict blocks that follow the tier sections.
fn render_conflicts(report: &Report, color: bool) -> String {
    let mut output = String::new();

    let heading = "CONFLICT"
        .bold()
        .whenever(Condition::cached(color))
        .to_string();

    for conflict in &report.conflicts {
        output.push_str(&format!("\n{heading}  {}\n", conflict.rule_path));

        for recommendation in &conflict.recommendations {
            output.push_str(&format!(
                "  {} recommends  {}\n",
                recommendation.dimension.as_str(),
                recommendation.setting
            ));
        }

        output.push_str("  No fix applied. Choose one dimension, or set a per-package override.\n");
    }

    output
}

/// Render the summary of a non-empty report.
///
/// The first line carries the finding count, the non-zero severity counts and
/// the non-zero magnitude counts; the second states the auto-fixable count, but
/// only when it is positive. A category whose count is zero is omitted, so the
/// summary never invites the reader to read meaning into a zero.
fn render_summary(report: &Report, color: bool) -> String {
    let summary = &report.summary;

    let sev = [
        (summary.by_severity.high, Severity::High.as_str()),
        (summary.by_severity.medium, Severity::Medium.as_str()),
        (summary.by_severity.low, Severity::Low.as_str()),
        (summary.by_severity.info, Severity::Info.as_str()),
    ];
    let mag = [
        (summary.by_magnitude.large, Magnitude::Large.as_str()),
        (summary.by_magnitude.moderate, Magnitude::Moderate.as_str()),
        (summary.by_magnitude.small, Magnitude::Small.as_str()),
        (
            summary.by_magnitude.negligible,
            Magnitude::Negligible.as_str(),
        ),
        (summary.by_magnitude.none, Magnitude::None.as_str()),
    ];

    let finding_word = if summary.total == 1 {
        "finding"
    } else {
        "findings"
    };

    let mut first = format!("  {} {finding_word}: ", summary.total);
    first.push_str(&counted_list(&sev, color));
    first.push('.');

    if let Some(by_magnitude) = counted_clause("By magnitude: ", &mag) {
        first.push_str("  ");
        first.push_str(&by_magnitude);
    }

    let heading = "SUMMARY"
        .bold()
        .whenever(Condition::cached(color))
        .to_string();

    let mut output = format!("\n{heading}\n{first}\n");

    if summary.fixable > 0 {
        output.push_str(&format!("  {} auto-fixable.\n", summary.fixable));
    }

    output
}

/// The columns of indentation the magnitude legend's wrapped lines carry.
const LEGEND_INDENT: usize = 2;

/// Render the magnitude legend at [`LEGEND_INDENT`] columns, wrapped to `width`.
/// The legend is dimmed: the text is mandatory but explains the report rather
/// than reporting on it. Each wrapped line is painted on its own, so no escape
/// sequence spans a line break and the indent stays outside the paint.
fn render_legend(width: usize, color: bool) -> String {
    let indent = " ".repeat(LEGEND_INDENT);
    let wrapped = wrap(crate::report::MAGNITUDE_LEGEND, LEGEND_INDENT, width);

    let mut output = String::from("\n");

    for line in wrapped.lines() {
        let text = line.strip_prefix(indent.as_str()).unwrap_or_default();

        output.push_str(&indent);
        output.push_str(&text.dim().whenever(Condition::cached(color)).to_string());
        output.push('\n');
    }

    output
}

/// Join the non-zero counts of `entries` as `<n> <name>`, comma-separated.
///
/// The names are severities; `color` decides whether each entry carries its
/// severity's style. The separators stay plain.
fn counted_list(entries: &[(usize, &str)], color: bool) -> String {
    let condition = Condition::cached(color);

    entries
        .iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, name)| {
            let entry = format!("{count} {name}");
            let style = Severity::from_name(name).map_or_else(yansi::Style::new, severity_style);

            entry.paint(style).whenever(condition).to_string()
        })
        .collect::<Vec<String>>()
        .join(", ")
}

/// Render `<prefix><count> <name>, ...` from the non-zero counts of `entries`,
/// or [`None`] when every count is zero.
fn counted_clause(prefix: &str, entries: &[(usize, &str)]) -> Option<String> {
    let body = counted_list(entries, false);

    if body.is_empty() {
        None
    } else {
        Some(format!("{prefix}{body}."))
    }
}

/// Wrap `text` to `width`, indenting every line with `indent` spaces.
///
/// Wrapping breaks on whitespace and never splits a word; a word longer than
/// the available budget occupies its own line unbroken.
fn wrap(text: &str, indent: usize, width: usize) -> String {
    format!(
        "{}{}\n",
        " ".repeat(indent),
        wrap_inline(text, indent, width)
    )
}

/// Wrap `text` to `width` at `indent` spaces, without a trailing newline, as the
/// first line and every continuation line see it.
fn wrap_inline(text: &str, indent: usize, width: usize) -> String {
    let budget = width.saturating_sub(indent).max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(word);
        } else if current.chars().count() + 1 + word.chars().count() <= budget {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    let separator = format!("\n{}", " ".repeat(indent));

    lines.join(&separator)
}
