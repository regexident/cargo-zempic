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

use crate::Resolution;
use crate::finding::{Finding, Magnitude, Severity, Tier};
use crate::profile::ResolvedProfile;
use crate::report::{BOLD, RESET, Report, severity_color};

/// Render `report` as the human-readable form.
///
/// `color` decides whether ANSI escape sequences are emitted, and `width` is
/// the column budget the header and wrapped prose are laid out to.
pub fn render(report: &Report, color: bool, width: usize) -> String {
    let mut output = render_header(&report.resolution);

    if report.is_empty() {
        output.push_str("No findings.\n");
        output.push_str(&render_legend(width));

        return output;
    }

    output.push_str(&render_tiers(report, color, width));
    output.push_str(&render_conflicts(report));
    output.push_str(&render_summary(report));
    output.push_str(&render_legend(width));

    output
}

/// Render the report header naming where the analysis ran.
///
/// The header states the workspace and its member counts, the profile with its
/// `inherits` chain, the selected dimensions and the target triple. It carries
/// no size figure: the counts are facts about the report itself.
fn render_header(resolution: &Resolution) -> String {
    let mut output = String::new();

    output.push_str(&format!("cargo-zempic {}\n", env!("CARGO_PKG_VERSION")));

    output.push_str(&format!(
        "{:>14} {}\n",
        "workspace:",
        format_args!(
            "{}  ({} members, {} selected)",
            resolution.workspace_root,
            resolution.member_count,
            resolution.selected.len()
        )
    ));

    output.push_str(&format!(
        "{:>14} {}\n",
        "profile:",
        format_args!(
            "{}  (inherits: {})",
            resolution.profile.name,
            render_inherits(&resolution.profile)
        )
    ));

    let dimensions: Vec<&str> = resolution
        .dimensions
        .iter()
        .map(|dimension| dimension.as_str())
        .collect();

    output.push_str(&format!(
        "{:>14} {}\n",
        "optimizing for:",
        dimensions.join(", ")
    ));

    output.push_str(&format!("{:>14} {}\n", "target:", resolution.target_triple));

    output
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
            width,
        ));

        for finding in findings {
            output.push('\n');
            output.push_str(&render_finding(finding, report, color, width));
        }
    }

    output
}

/// Render a tier heading with the finding count right-aligned at `width`.
fn render_tier_heading(number: usize, tier: Tier, count: usize, width: usize) -> String {
    let label = format!("TIER {number}  {}", tier.title().to_uppercase());
    let counted = if count == 1 {
        "1 finding".to_owned()
    } else {
        format!("{count} findings")
    };

    let padding = width.saturating_sub(label.chars().count() + counted.chars().count());

    format!("{label}{}{counted}\n", " ".repeat(padding))
}

/// Render one finding's block.
///
/// The first line carries the severity, magnitude, rule identifier and a
/// right-hand column; the second carries effort, runtime cost and fixability;
/// the detail and the tradeoff follow wrapped to `width`. When `color` is true
/// the severity label and the rule identifier are wrapped in their escape
/// sequences; nothing else is colored.
fn render_finding(finding: &Finding, report: &Report, color: bool, width: usize) -> String {
    let mut output = String::new();

    let severity = finding.severity.as_str().to_uppercase();
    let severity = if color {
        format!("{}{severity}{RESET}", severity_color(finding.severity))
    } else {
        severity
    };

    let rule = finding.rule.to_string();
    let rule = if color {
        format!("{BOLD}{rule}{RESET}")
    } else {
        rule
    };

    let right = right_column(finding, &report.resolution);

    output.push_str(&format!(
        "  {severity:<6} {:<6} {rule:<32} {right}\n",
        finding.magnitude.as_str(),
    ));

    output.push_str(&format!(
        "        effort: {}   runtime cost: {}   fix: {}\n",
        finding.effort.as_str(),
        finding.runtime_cost.as_str(),
        finding.fixability.as_str(),
    ));

    output.push_str(&wrap(&finding.detail, 8, width));
    output.push_str(&format!(
        "        Tradeoff: {}",
        wrap_inline(&finding.tradeoff, 8, width)
    ));

    output
}

/// The right-hand column of a finding's first line.
///
/// A finding that recommends a setting change shows `<current> -> <recommended>`;
/// otherwise it shows the location's file relative to the workspace root, or is
/// empty when the finding has no location.
fn right_column(finding: &Finding, resolution: &Resolution) -> String {
    if let Some(change) = &finding.change {
        return format!("{} -> {}", change.current, change.recommended);
    }

    match &finding.location {
        Some(location) => location
            .span
            .file
            .strip_prefix(&resolution.workspace_root)
            .map(camino::Utf8Path::to_owned)
            .unwrap_or_else(|_| location.span.file.clone())
            .into_string(),
        None => String::new(),
    }
}

/// Render the conflict blocks that follow the tier sections.
fn render_conflicts(report: &Report) -> String {
    let mut output = String::new();

    for conflict in &report.conflicts {
        output.push_str(&format!("\nCONFLICT  {}\n", conflict.rule_path));

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
fn render_summary(report: &Report) -> String {
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
    first.push_str(&counted_list(&sev));
    first.push('.');

    if let Some(by_magnitude) = counted_clause("By magnitude: ", &mag) {
        first.push_str("  ");
        first.push_str(&by_magnitude);
    }

    let mut output = format!("\nSUMMARY\n{first}\n");

    if summary.fixable > 0 {
        output.push_str(&format!("  {} auto-fixable.\n", summary.fixable));
    }

    output
}

/// Render the magnitude legend, wrapped to `width` at two spaces of indent.
fn render_legend(width: usize) -> String {
    format!("\n{}", wrap(crate::report::MAGNITUDE_LEGEND, 2, width))
}

/// Join the non-zero counts of `entries` as `<n> <name>`, comma-separated.
fn counted_list(entries: &[(usize, &str)]) -> String {
    entries
        .iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, name)| format!("{count} {name}"))
        .collect::<Vec<String>>()
        .join(", ")
}

/// Render `<prefix><count> <name>, ...` from the non-zero counts of `entries`,
/// or [`None`] when every count is zero.
fn counted_clause(prefix: &str, entries: &[(usize, &str)]) -> Option<String> {
    let body = counted_list(entries);

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

/// Wrap `text` to `width` at `indent` spaces, without a trailing newline.
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
