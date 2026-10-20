//! The Markdown report document.
//!
//! The document carries the same content as the human renderer, laid out for a
//! pull request or issue: one tiered section per non-empty tier with a findings
//! table, a conflicts section, a summary and the magnitude legend. It takes no
//! width and no color: Markdown is not wrapped to a terminal and carries no
//! ANSI sequence, and the legend is retained verbatim rather than rewrapped.

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

/// Render one `## Tier` section per non-empty tier, each with its findings
/// table.
fn render_tiers(report: &Report) -> String {
    let mut output = String::new();

    for (index, tier) in Tier::ALL.iter().enumerate() {
        let findings: Vec<&Finding> = report.findings_in(*tier).collect();

        if findings.is_empty() {
            continue;
        }

        output.push_str(&format!(
            "\n## Tier {}: {}\n\n",
            index + 1,
            title_case(tier.title()),
        ));

        output.push_str(
            "| Rule | Severity | Magnitude | Change | Location | Effort | Runtime cost | Fix | Detail | Tradeoff |\n",
        );
        output.push_str("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n");

        for finding in findings {
            output.push_str(&render_row(finding, &report.resolution));
        }
    }

    output
}

/// Render one findings-table row.
fn render_row(finding: &Finding, resolution: &Resolution) -> String {
    let change = finding.change.as_ref().map_or_else(String::new, |change| {
        format!("{} -> {}", change.current, change.recommended)
    });
    let location = finding
        .location
        .as_ref()
        .map_or_else(String::new, |location| {
            let file = location
                .span
                .file
                .strip_prefix(&resolution.workspace_root)
                .map(camino::Utf8Path::to_owned)
                .unwrap_or_else(|_| location.span.file.clone());

            format!("{}:{}", file, location.span.line)
        });

    format!(
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
        cell(&format!("`{}`", finding.rule)),
        cell(finding.severity.as_str()),
        cell(finding.magnitude.as_str()),
        cell(&change),
        cell(&location),
        cell(finding.effort.as_str()),
        cell(finding.runtime_cost.as_str()),
        cell(finding.fixability.as_str()),
        cell(&detail_with_advice(finding)),
        cell(&finding.tradeoff),
    )
}

/// The Detail cell's text: the detail alone, or the detail with the advice
/// appended behind an `Advice:` label when the finding carries one.
fn detail_with_advice(finding: &Finding) -> String {
    match &finding.advice {
        Some(advice) => format!("{} Advice: {advice}", finding.detail),
        None => finding.detail.clone(),
    }
}

/// Escape `text` for a Markdown table cell.
///
/// A pipe would otherwise add a column and shift every cell after it; a newline
/// cannot appear in a cell at all, so it becomes a space. The escape is applied
/// to every cell, including constants, so a future value cannot break the table.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace(['\n', '\r'], " ")
}

/// Render a tier name in title case.
///
/// The tier vocabulary is stored uppercase for the human heading; a Markdown
/// heading reads better in title case, so only the first character is kept
/// uppercase and the rest lowercased.
fn title_case(name: &str) -> String {
    let mut characters = name.chars();

    match characters.next() {
        Some(first) => {
            first.to_uppercase().collect::<String>() + &characters.as_str().to_lowercase()
        }
        None => String::new(),
    }
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
