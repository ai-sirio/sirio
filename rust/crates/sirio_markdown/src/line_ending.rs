/// The line ending a text file was written with.
///
/// An editor's buffer holds LF only. The ending is kept beside it, so a file
/// is read as LF text and written back with the ending it was read with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    Crlf,
}

impl LineEnding {
    /// The ending of `raw`: CRLF when more lines end in `\r\n` than in a bare
    /// `\n`, LF otherwise. Ties and text with no line break are LF.
    pub fn detect(raw: &str) -> LineEnding {
        let crlf = raw.matches("\r\n").count();
        let bare = raw.matches('\n').count() - crlf;
        if crlf > bare {
            LineEnding::Crlf
        } else {
            LineEnding::Lf
        }
    }

    /// `raw` with every `\r\n` replaced by `\n`. A lone `\r` is text and stays.
    pub fn normalize(raw: &str) -> String {
        raw.replace("\r\n", "\n")
    }

    /// Whether `raw` is `lf_text` written with `ending`. Text with no line
    /// break carries no ending on disk, so it matches either one.
    pub fn matches(raw: &str, lf_text: &str, ending: LineEnding) -> bool {
        Self::normalize(raw) == lf_text && (!lf_text.contains('\n') || Self::detect(raw) == ending)
    }

    /// `lf_text` written with this ending.
    pub fn apply(self, lf_text: &str) -> String {
        match self {
            LineEnding::Lf => lf_text.to_owned(),
            LineEnding::Crlf => lf_text.replace('\n', "\r\n"),
        }
    }
}
