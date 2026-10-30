//! Format-preserving reading of `Cargo.toml` manifests.
//!
//! Analysis needs two things from a manifest: the parsed structure, and the
//! location of the keys it reasons about so findings can point at them. A
//! [`toml_edit::Document`] supplies both, but a key's recorded location is a
//! byte range into the source text, while findings report one-based line and
//! column positions. Converting between the two requires the line structure of
//! the original text, so [`ManifestDocument`] retains the byte offset of every
//! newline alongside the parsed document.

use camino::{Utf8Path, Utf8PathBuf};

/// A one-based line and column position of a manifest key.
///
/// A span is present only for a document parsed from text and is lost after
/// programmatic mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeySpan {
    /// Manifest containing the key.
    pub file: Utf8PathBuf,
    /// Dotted key as requested, for example `profile.release.lto`.
    pub key: String,
    /// One-based line on which the key's first segment begins.
    pub line: usize,
    /// One-based column at which the key's first segment begins.
    pub column: usize,
}

/// A `Cargo.toml` parsed for analysis, retaining the line structure its spans
/// refer to.
#[derive(Debug)]
pub struct ManifestDocument {
    path: Utf8PathBuf,
    /// Ascending byte offsets of every `'\n'` in the parsed text.
    newlines: Vec<usize>,
    document: toml_edit::Document<String>,
}

impl ManifestDocument {
    /// Read and parse the manifest at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`ZempicError::ManifestRead`] if the file cannot be read and
    /// [`ZempicError::ManifestParse`] if its contents are not valid TOML:
    /// [`crate::ZempicError`].
    pub fn load(path: &Utf8Path) -> crate::Result<Self> {
        let text =
            std::fs::read_to_string(path).map_err(|source| crate::ZempicError::ManifestRead {
                path: path.to_owned(),
                source,
            })?;

        Self::parse(path, &text)
    }

    /// Parse `text` as the manifest at `path` without touching the filesystem.
    ///
    /// # Errors
    ///
    /// Returns [`ZempicError::ManifestParse`] if `text` is not valid TOML:
    /// [`crate::ZempicError`].
    pub fn parse(path: &Utf8Path, text: &str) -> crate::Result<Self> {
        let document = toml_edit::Document::<String>::parse(text.to_owned()).map_err(|source| {
            crate::ZempicError::ManifestParse {
                path: path.to_owned(),
                source,
            }
        })?;

        Ok(Self {
            path: path.to_owned(),
            newlines: text.match_indices('\n').map(|(index, _)| index).collect(),
            document,
        })
    }

    /// The manifest's path.
    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    /// The parsed TOML document, retaining the spans of the text it was parsed
    /// from.
    pub fn document(&self) -> &toml_edit::Document<String> {
        &self.document
    }

    /// Resolve `dotted_key`, such as `profile.release.lto`, to its source
    /// position.
    ///
    /// Returns `None` when any segment of the key is absent or when the parsed
    /// document carries no span for the key.
    pub fn key_span(&self, dotted_key: &str) -> Option<KeySpan> {
        let mut segments = dotted_key.split('.').peekable();
        let mut table = self.document.as_table();
        let key = loop {
            let segment = segments.next()?;
            let (key, value) = table.get_key_value(segment)?;

            if segments.peek().is_none() {
                break key;
            }

            table = value.as_table()?;
        };

        let span = key.span()?;
        let (line, column) = self.position_at(span.start);

        Some(KeySpan {
            file: self.path.clone(),
            key: dotted_key.to_owned(),
            line,
            column,
        })
    }

    /// Convert a byte offset in the original text to a one-based line and
    /// column.
    fn position_at(&self, offset: usize) -> (usize, usize) {
        let index = self.newlines.partition_point(|newline| *newline < offset);
        let column = match index {
            0 => offset + 1,
            _ => offset - self.newlines[index - 1],
        };

        (index + 1, column)
    }
}

#[cfg(test)]
mod tests {
    use super::ManifestDocument;
    use camino::Utf8Path;

    #[test]
    fn key_span_reports_positions_on_every_line() {
        let path = Utf8Path::new("Cargo.toml");
        let text = "[package]\nname = \"demo\"\n\n[profile.release]\n  lto = true\n";
        let document = ManifestDocument::parse(path, text).unwrap();

        let package = document.key_span("package").unwrap();
        let name = document.key_span("package.name").unwrap();
        let lto = document.key_span("profile.release.lto").unwrap();

        assert_eq!((package.line, package.column), (1, 2));
        assert_eq!((name.line, name.column), (2, 1));
        assert_eq!((lto.line, lto.column), (5, 3));
    }
}
