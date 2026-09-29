//! Jupyter notebooks (`.ipynb`, nbformat 4 JSON) → one segment per markdown or code cell.
//!
//! - Locator `"cell N"`, where N is the 1-based position among *all* cells (raw and empty
//!   cells included), so it matches the cell's position when the notebook is opened.
//! - Markdown cells are kept as written. Code cells are wrapped in a Markdown code fence
//!   tagged with the notebook's language (e.g. ```` ```python ````) so readers can tell code
//!   from prose.
//! - Raw cells and all cell outputs are skipped (outputs are mostly images and logs).

use serde_json::Value;

use crate::util::failed;
use crate::{ExtractError, Segment};

pub(crate) fn extract(json: &str) -> Result<Vec<Segment>, ExtractError> {
    let notebook: Value = serde_json::from_str(json)
        .map_err(|e| failed(format!("notebook is not valid JSON: {e}")))?;
    let cells = notebook
        .get("cells")
        .and_then(Value::as_array)
        .ok_or_else(|| failed("notebook has no \"cells\" list (only nbformat 4 is supported)"))?;
    let language = notebook_language(&notebook);

    let mut segments = Vec::new();
    for (index, cell) in cells.iter().enumerate() {
        let source = cell_source(cell);
        if source.trim().is_empty() {
            continue;
        }
        let text = match cell.get("cell_type").and_then(Value::as_str) {
            Some("markdown") => source,
            Some("code") => fenced_code(&source, language),
            _ => continue, // "raw" or unknown cell types
        };
        segments.push(Segment {
            locator: Some(format!("cell {}", index + 1)),
            text,
        });
    }
    Ok(segments)
}

/// `cell.source` is either one string or a list of lines (each usually ending in `\n`).
fn cell_source(cell: &Value) -> String {
    match cell.get("source") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(lines)) => lines.iter().filter_map(Value::as_str).collect(),
        _ => String::new(),
    }
}

/// The notebook's programming language, e.g. "python" (empty if not recorded).
fn notebook_language(notebook: &Value) -> &str {
    let metadata = &notebook["metadata"];
    [
        &metadata["language_info"]["name"],
        &metadata["kernelspec"]["language"],
    ]
    .into_iter()
    .filter_map(Value::as_str)
    .filter_map(|name| name.split_whitespace().next())
    .next()
    .unwrap_or("")
}

/// Wrap code in a Markdown fence (at least three backticks) that is longer than any backtick
/// run inside the code.
fn fenced_code(code: &str, language: &str) -> String {
    // The pieces between non-backtick characters are backtick runs; a backtick is one byte,
    // so `len` counts backticks.
    let longest_run = code.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest_run.max(2) + 1);
    format!("{fence}{language}\n{}\n{fence}", code.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FailureKind;
    use crate::test_support::kind_of;

    #[test]
    fn markdown_and_code_cells_with_cell_locators() {
        let json = r##"{
          "nbformat": 4, "nbformat_minor": 5,
          "metadata": {"kernelspec": {"language": "python", "name": "python3"},
                       "language_info": {"name": "python"}},
          "cells": [
            {"cell_type": "markdown", "metadata": {}, "source": ["# DEMO101 Lab 1\n", "Intro to demo studies."]},
            {"cell_type": "raw", "metadata": {}, "source": "raw cell text"},
            {"cell_type": "code", "metadata": {}, "execution_count": 1,
             "source": "print('hello')\n",
             "outputs": [{"output_type": "stream", "name": "stdout", "text": ["OUTPUT_TEXT\n"]}]},
            {"cell_type": "code", "metadata": {}, "source": [], "outputs": []},
            {"cell_type": "markdown", "metadata": {}, "source": "Done."}
          ]
        }"##;
        let segments = extract(json).unwrap();
        assert_eq!(
            segments,
            vec![
                Segment {
                    locator: Some("cell 1".into()),
                    text: "# DEMO101 Lab 1\nIntro to demo studies.".into()
                },
                Segment {
                    locator: Some("cell 3".into()),
                    text: "```python\nprint('hello')\n```".into()
                },
                Segment {
                    locator: Some("cell 5".into()),
                    text: "Done.".into()
                },
            ]
        );
    }

    #[test]
    fn language_falls_back_to_kernelspec_or_nothing() {
        let with_kernelspec = r#"{"metadata": {"kernelspec": {"language": "R"}},
            "cells": [{"cell_type": "code", "source": "x <- 1"}]}"#;
        assert_eq!(
            extract(with_kernelspec).unwrap()[0].text,
            "```R\nx <- 1\n```"
        );
        let without = r#"{"cells": [{"cell_type": "code", "source": "x = 1"}]}"#;
        assert_eq!(extract(without).unwrap()[0].text, "```\nx = 1\n```");
    }

    #[test]
    fn code_containing_fences_gets_a_longer_fence() {
        assert_eq!(
            fenced_code("s = '```'", "python"),
            "````python\ns = '```'\n````"
        );
    }

    #[test]
    fn long_backtick_run_is_fenced_quickly() {
        // Regression: the fence used to grow one backtick per full scan of the code, which is
        // quadratic in the length of the run (minutes for a few hundred KB).
        let code = format!("s = '{}'", "`".repeat(200_000));
        let started = std::time::Instant::now();
        let fenced = fenced_code(&code, "python");
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        let fence = "`".repeat(200_001);
        assert!(fenced.starts_with(&format!("{fence}python\n")));
        assert!(fenced.ends_with(&format!("'\n{fence}")));
    }

    #[test]
    fn invalid_notebooks_are_failed() {
        for json in ["not json", "[]", r#"{"worksheets": []}"#, r#"{"cells": 3}"#] {
            let result = extract(json);
            assert!(matches!(result, Err(ExtractError::Failed(_))), "{json}");
            assert_eq!(kind_of(&result), Some(FailureKind::Malformed), "{json}");
        }
    }

    #[test]
    fn odd_cells_are_tolerated() {
        let json = r#"{"cells": [{"source": "no type"}, {"cell_type": "markdown"},
            {"cell_type": "markdown", "source": ["a", 5, "b"]}, 7]}"#;
        let segments = extract(json).unwrap();
        assert_eq!(
            segments,
            vec![Segment {
                locator: Some("cell 3".into()),
                text: "ab".into()
            }]
        );
    }
}
