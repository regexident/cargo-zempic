//! The Markdown report document.
//!
//! The document carries the same content as the human renderer, laid out for a
//! pull request or issue: one tiered section per non-empty tier, opened by a
//! one-line index of its findings and followed by a numbered subsection per
//! finding, then a conflicts section, a summary and the magnitude legend. It
//! takes no width and no color: Markdown is not wrapped to a terminal and
//! carries no ANSI sequence, and the legend is retained verbatim rather than
//! rewrapped.
//!
//! A finding has no natural home in a wide table: nine columns force every cell
//! to a fragment of a sentence, and the prose fields, which are the substance
//! of a finding, get the same width as a severity word. A subsection per
//! finding instead gives each field its own line or paragraph, and the index
//! keeps the per-tier overview a table would have provided.

use crate::Resolution;
use crate::finding::{Finding, Tier};
use crate::report::{MAGNITUDE_LEGEND, Report};

/// Render `report` as a Markdown document.
pub fn render(report: &Report) -> String {
    let mut output = format!("# cargo-zempic {}\n", env!("CARGO_PKG_VERSION"));

    output.push_str(&render_facts(&report.resolution));

    if report.is_empty() {
        output.push_str("\nNo findings.\n");
        output.push_str(&format!("\n{MAGNITUDE_LEGEND}\n"));

        return output;
    }

    output.push_str(&render_tiers(report));
    output.push_str(&render_conflicts(report));
    output.push_str(&render_summary(report));
    output.push_str(&format!("\n{MAGNITUDE_LEGEND}\n"));

    output
}

/// Render the list of facts the human header states.
fn render_facts(resolution: &Resolution) -> String {
    let dimensions: Vec<&str> = resolution
        .dimensions
        .iter()
        .map(|dimension| dimension.as_str())
        .collect();

    let inherits = if resolution.profile.inherits_chain.is_empty() {
        "none".to_owned()
    } else {
        resolution.profile.inherits_chain.join(" -> ")
    };

    let mut output = String::new();

    output.push_str(&format!("\n- Workspace: `{}`\n", resolution.workspace_root));
    output.push_str(&format!(
        "- Members: {} ({} selected)\n",
        resolution.member_count,
        resolution.selected.len()
    ));
    output.push_str(&format!(
        "- Profile: `{}` (inherits: {})\n",
        resolution.profile.name, inherits
    ));
    output.push_str(&format!("- Optimizing for: {}\n", dimensions.join(", ")));
    output.push_str(&format!("- Target: `{}`\n", resolution.target_triple));

    output
}

/// Render one `## Tier` section per non-empty tier, each with an index of its
/// findings followed by their subsections.
fn render_tiers(report: &Report) -> String {
    let mut output = String::new();

    for (index, tier) in Tier::ALL.iter().enumerate() {
        let findings: Vec<&Finding> = report.findings_in(*tier).collect();

        if findings.is_empty() {
            continue;
        }

        output.push_str(&format!("\n## Tier {}: {}\n\n", index + 1, tier.heading(),));

        output.push_str(&render_index(&findings));

        for (position, finding) in findings.iter().enumerate() {
            let separator = if position == 0 { "" } else { "\n" };

            output.push_str(&format!(
                "{separator}{}",
                render_finding(position + 1, finding, &report.resolution)
            ));
        }
    }

    output
}

/// Render the numbered index that opens a tier.
///
/// Each entry states the finding's rule, headline, severity and magnitude on
/// one line, so a reader sees what the tier holds without opening a subsection.
fn render_index(findings: &[&Finding]) -> String {
    let mut output = String::new();

    for (position, finding) in findings.iter().enumerate() {
        output.push_str(&format!(
            "{}. **`{}`** — {} ({}, {})\n",
            position + 1,
            finding.rule,
            inline(&finding.title),
            finding.severity.as_str(),
            finding.magnitude.as_str(),
        ));
    }

    output.push('\n');

    output
}

/// Render one finding as a numbered `###` subsection.
///
/// The facts a reader can scan are bulleted, one per line, so that a diff, a
/// search or a line-oriented reader can address each field on its own. The
/// prose fields follow as paragraphs, with the advice blockquoted so it reads
/// as an aside rather than as more detail.
fn render_finding(position: usize, finding: &Finding, resolution: &Resolution) -> String {
    let mut output = format!(
        "### {position}. {}: {}\n\n",
        finding.rule,
        inline(&finding.title),
    );

    if let Some(change) = &finding.change {
        output.push_str(&format!(
            "- **Change:** `{}` -> `{}`\n",
            paragraph(&change.current),
            paragraph(&change.recommended),
        ));
    }

    output.push_str(&format!("- **Severity:** {}\n", finding.severity.as_str()));
    output.push_str(&format!(
        "- **Magnitude:** {}\n",
        finding.magnitude.as_str()
    ));
    output.push_str(&format!(
        "- **Confidence:** {}\n",
        finding.confidence.as_str()
    ));
    output.push_str(&format!("- **Effort:** {}\n", finding.effort.as_str()));
    output.push_str(&format!(
        "- **Runtime cost:** {}\n",
        finding.runtime_cost.as_str()
    ));

    if let Some(fixability) = finding.fixability.reported() {
        output.push_str(&format!("- **Fix:** {fixability}\n"));
    }

    if let Some(location) = &finding.location {
        let file = location
            .span
            .file
            .strip_prefix(&resolution.workspace_root)
            .map(camino::Utf8Path::to_owned)
            .unwrap_or_else(|_| location.span.file.clone());

        output.push_str(&format!(
            "- **Location:** `{}:{}`\n",
            paragraph(file.as_str()),
            location.span.line,
        ));
    }

    if let Some(package) = &finding.package {
        output.push_str(&format!("- **Package:** `{}`\n", paragraph(package)));
    }

    output.push_str(&format!("\n{}\n", paragraph(&finding.detail)));

    if let Some(advice) = &finding.advice {
        output.push_str(&format!("\n> **Advice:** {}\n", paragraph(advice)));
    }

    output.push_str(&format!("\n*Tradeoff:* {}\n", paragraph(&finding.tradeoff)));

    if !finding.references.is_empty() {
        let references: Vec<String> = finding
            .references
            .iter()
            .map(|reference| inline(reference))
            .collect();

        output.push_str(&format!("\n*References:* {}\n", references.join(", ")));
    }

    output
}

/// Escape prose for a Markdown paragraph.
///
/// A newline would otherwise end the paragraph and turn the rest of the field
/// into stray text, so it becomes a space. A pipe is not escaped: outside a
/// table it is literal text, and escaping it would show a backslash.
fn paragraph(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}

/// Escape prose for a span that must stay on one line.
///
/// This is [`paragraph`] plus pipe escaping, for text spliced into a line that
/// already carries Markdown syntax, where a stray pipe is at best noise.
fn inline(text: &str) -> String {
    paragraph(text).replace('|', "\\|")
}

/// Render the conflicts section, when the report has any.
fn render_conflicts(report: &Report) -> String {
    if report.conflicts.is_empty() {
        return String::new();
    }

    let mut output = String::from("\n## Conflicts\n");

    for conflict in &report.conflicts {
        output.push_str(&format!("\n### {}\n\n", conflict.rule_path));

        for recommendation in &conflict.recommendations {
            output.push_str(&format!(
                "- {} recommends {}\n",
                recommendation.dimension.as_str(),
                recommendation.setting,
            ));
        }

        output.push_str("\nNo fix applied. Choose one dimension, or set a per-package override.\n");
    }

    output
}

/// Render the summary section, carrying the same non-zero categories the human
/// summary states.
fn render_summary(report: &Report) -> String {
    let summary = &report.summary;

    let finding_word = if summary.total == 1 {
        "finding"
    } else {
        "findings"
    };

    let mut output = format!("\n## Summary\n\n- {} {finding_word}\n", summary.total);

    let severities = [
        (summary.by_severity.high, "high"),
        (summary.by_severity.medium, "medium"),
        (summary.by_severity.low, "low"),
        (summary.by_severity.info, "info"),
    ];

    for (count, name) in severities {
        if count > 0 {
            output.push_str(&format!("- {count} {name}\n"));
        }
    }

    let magnitudes = [
        (summary.by_magnitude.large, "large"),
        (summary.by_magnitude.moderate, "moderate"),
        (summary.by_magnitude.small, "small"),
        (summary.by_magnitude.negligible, "negligible"),
        (summary.by_magnitude.none, "none"),
    ];
    let nonzero: Vec<String> = magnitudes
        .iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, name)| format!("{count} {name}"))
        .collect();

    if !nonzero.is_empty() {
        output.push_str(&format!("- By magnitude: {}\n", nonzero.join(", ")));
    }

    if summary.fixable > 0 {
        output.push_str(&format!("- {} auto-fixable\n", summary.fixable));
    }

    output
}
