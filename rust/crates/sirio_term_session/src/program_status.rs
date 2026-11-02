//! Scans the bytes a PTY produces for OSC 7501 program status reports.
//!
//! A program reports its status with `ESC ] 7501 ; state=<s>[:kind=..][:app=..][:id=..]... ST`
//! (ST is `ESC \` or BEL), and some only report after the terminal answers their probe,
//! `ESC ] 7501 ; ? ST`. The scanner only reads: it never alters or removes bytes, a report
//! may arrive split across any number of reads, and it keeps at most [`MAX_KEPT_BODY`] bytes
//! of a body however long it runs. Fields the scanner does not know are ignored.
//!
//! The scanner is stricter than the emulator on purpose: a body interrupted by a bare ESC
//! (CAN and SUB abort it too) is dropped, and a field holding other control bytes never
//! matches a known value, so it can miss a report but never invent one.

// Nothing consumes the scanner until the session engine wires it in; drop this then.
#![allow(dead_code)]

/// The longest body kept; the scanner still finds the terminator of a longer one.
pub(crate) const MAX_KEPT_BODY: usize = 4096;

/// What a program says it is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportState {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
}

/// One status report from the program on the pane itself (a root record).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusRecord {
    pub state: ReportState,
    /// Only `permission`, `question` or `auth`.
    pub kind: Option<String>,
    /// Only 1-64 bytes of `[A-Za-z0-9._-]`.
    pub app: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Scanned {
    /// A probe. `end` is the offset just past its terminator, counted from the start of the
    /// chunk passed to the `scan` call that completed it (the engine splits the chunk there);
    /// `bel` is true when BEL ended it, so the answer can use the same terminator.
    Probe { end: usize, bel: bool },
    /// A root report; `None` is `state=clear`.
    Root(Option<StatusRecord>),
}

/// What the terminal answers to a probe. Sirio's answer lists the states and kinds it reads.
pub(crate) const PROBE_ANSWER_ST: &[u8] =
    b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x1b\\";
pub(crate) const PROBE_ANSWER_BEL: &[u8] =
    b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x07";

/// The answer ended by the terminator the probe used.
pub(crate) fn probe_answer(bel: bool) -> &'static [u8] {
    if bel {
        PROBE_ANSWER_BEL
    } else {
        PROBE_ANSWER_ST
    }
}

const PREFIX: &[u8] = b"\x1b]7501;";
const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

enum State {
    /// Outside any sequence of interest.
    Ground,
    /// Saw ESC.
    Esc,
    /// Matched this many bytes of `PREFIX`.
    Prefix(usize),
    /// Inside the body of a 7501 sequence.
    Body,
    /// Inside the body, saw ESC: `\` ends it, anything else abandons it.
    BodyEsc,
}

/// A byte state machine; it carries a partial sequence from one `scan` call to the next.
pub(crate) struct OscScanner {
    state: State,
    /// The first `MAX_KEPT_BODY` bytes of the current body.
    body: Vec<u8>,
    /// The first byte the limit discarded, when it did.
    first_dropped: Option<u8>,
}

impl OscScanner {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Ground,
            body: Vec::new(),
            first_dropped: None,
        }
    }

    /// Feed one read of PTY output; completed reports and probes are appended to `out`.
    /// A probe's `end` counts from the start of `bytes`.
    pub(crate) fn scan(&mut self, bytes: &[u8], out: &mut Vec<Scanned>) {
        for (index, &byte) in bytes.iter().enumerate() {
            self.step(byte, index + 1, out);
        }
    }

    /// `after` is the offset just past `byte` in the current chunk.
    fn step(&mut self, byte: u8, after: usize, out: &mut Vec<Scanned>) {
        match self.state {
            State::Ground => self.ground(byte),
            State::Esc => {
                if byte == b']' {
                    self.state = State::Prefix(2);
                } else {
                    self.ground(byte);
                }
            }
            State::Prefix(matched) => {
                if byte == PREFIX[matched] {
                    if matched + 1 == PREFIX.len() {
                        self.body.clear();
                        self.first_dropped = None;
                        self.state = State::Body;
                    } else {
                        self.state = State::Prefix(matched + 1);
                    }
                } else {
                    self.ground(byte);
                }
            }
            State::Body => match byte {
                BEL => self.finish(after, true, out),
                ESC => self.state = State::BodyEsc,
                CAN | SUB => self.abandon(),
                _ => self.keep(byte),
            },
            State::BodyEsc => {
                if byte == b'\\' {
                    self.finish(after, false, out);
                } else {
                    // The ESC began a new sequence: this byte is its second byte.
                    self.abandon();
                    self.state = State::Esc;
                    self.step(byte, after, out);
                }
            }
        }
    }

    fn ground(&mut self, byte: u8) {
        self.state = if byte == ESC {
            State::Esc
        } else {
            State::Ground
        };
    }

    fn keep(&mut self, byte: u8) {
        if self.body.len() < MAX_KEPT_BODY {
            self.body.push(byte);
        } else if self.first_dropped.is_none() {
            self.first_dropped = Some(byte);
        }
    }

    fn abandon(&mut self) {
        self.body.clear();
        self.first_dropped = None;
        self.state = State::Ground;
    }

    fn finish(&mut self, end: usize, bel: bool, out: &mut Vec<Scanned>) {
        self.state = State::Ground;
        if self.body == b"?" {
            out.push(Scanned::Probe { end, bel });
        } else if let Some(report) = parse_body(&self.body, self.first_dropped) {
            out.push(Scanned::Root(report));
        }
        self.body.clear();
        self.first_dropped = None;
    }
}

/// `Some(None)` is `clear`, `Some(Some(_))` a root record, `None` something to ignore.
fn parse_body(body: &[u8], first_dropped: Option<u8>) -> Option<Option<StatusRecord>> {
    let mut fields: Vec<&[u8]> = body.split(|&b| b == b':').collect();
    // The limit cut the last field unless it fell exactly on a field boundary.
    if first_dropped.is_some_and(|b| b != b':') {
        fields.pop();
    }
    let mut state: Option<&[u8]> = None;
    let mut kind = None;
    let mut app = None;
    let mut sub_record = false;
    for field in fields {
        let Some(eq) = field.iter().position(|&b| b == b'=') else {
            continue;
        };
        let (key, value) = (&field[..eq], &field[eq + 1..]);
        match key {
            b"state" => state = Some(value),
            b"id" => sub_record |= !value.is_empty(),
            b"kind" => {
                kind = [&b"permission"[..], b"question", b"auth"]
                    .contains(&value)
                    .then(|| String::from_utf8_lossy(value).into_owned());
            }
            b"app" => {
                let valid = (1..=64).contains(&value.len())
                    && value
                        .iter()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(b));
                app = valid.then(|| String::from_utf8_lossy(value).into_owned());
            }
            _ => {}
        }
    }
    if sub_record {
        return None;
    }
    let state = match state? {
        b"idle" => ReportState::Idle,
        b"working" => ReportState::Working,
        b"done" => ReportState::Done,
        b"blocked" => ReportState::Blocked,
        b"error" => ReportState::Error,
        b"clear" => return Some(None),
        _ => return None,
    };
    Some(Some(StatusRecord { state, kind, app }))
}

#[cfg(test)]
mod tests {
    //! Every way the scanner can fail, written before the scanner. One test per line.
    //!
    //! `ESC`, `BEL`, `CAN` and `ST` name control bytes; the spaces in the lists below are
    //! visual only, the real byte strings have none.
    //!
    //! - `split_across_reads`: `ESC]75` | `01;state=working ESC\` is one `Root(Some(Working))`.
    //! - `bel_terminator`: a report ended by BEL is read like one ended by ST.
    //! - `esc_backslash_split`: the ST cut between its ESC and its `\` still ends the report.
    //! - `probe_inside_output`: `abc ESC]7501;? ESC\ def` is `Probe { end: 13, bel: false }`.
    //! - `bel_probe`: `abc ESC]7501;? BEL def` is `Probe { end: 12, bel: true }`.
    //! - `probe_split_across_reads`: `end` is the offset in the chunk that completes the probe.
    //! - `two_probes`: two probes in one chunk, with increasing `end`.
    //! - `probe_needs_exact_body`: `?x` and an empty body are not probes.
    //! - `sub_record_dropped`: a report with a non-empty `id` yields nothing.
    //! - `clear_root`: `state=clear` is `Root(None)`.
    //! - `unknown_state_dropped`: an unknown state yields nothing.
    //! - `missing_state_dropped`: a body with no state yields nothing.
    //! - `fields_any_order_and_unknown_ignored`: order does not matter, unknown keys are skipped.
    //! - `kind_outside_set_dropped`: only permission, question and auth survive.
    //! - `app_with_bad_bytes_dropped`: an app outside 1-64 bytes of `[A-Za-z0-9._-]` is dropped.
    //! - `long_msg_keeps_state`: 8000 bytes of msg keep state and kind, and the terminator is
    //!   still found, so the next report parses.
    //! - `field_cut_by_limit_dropped`: a field cut by the limit is dropped, not truncated.
    //! - `cut_on_field_boundary_keeps_field`: a field the limit ends exactly on is kept.
    //! - `other_osc_untouched`: OSC 2 and OSC 7502 yield nothing.
    //! - `can_aborts`: CAN or SUB in a body drops it, and the next report parses.
    //! - `esc_inside_body_restarts`: an ESC that is not an ST abandons the body and starts over.
    //! - `invalid_utf8_value`: a field with a non-UTF-8 value is dropped, the record stands.

    use super::*;

    fn scan_chunks(chunks: &[&[u8]]) -> Vec<Scanned> {
        let mut scanner = OscScanner::new();
        let mut out = Vec::new();
        for chunk in chunks {
            scanner.scan(chunk, &mut out);
        }
        out
    }

    fn root(state: ReportState, kind: Option<&str>, app: Option<&str>) -> Scanned {
        Scanned::Root(Some(StatusRecord {
            state,
            kind: kind.map(str::to_owned),
            app: app.map(str::to_owned),
        }))
    }

    fn plain(state: ReportState) -> Scanned {
        root(state, None, None)
    }

    #[test]
    fn split_across_reads() {
        let got = scan_chunks(&[b"\x1b]75", b"01;state=working\x1b\\"]);
        assert_eq!(got, [plain(ReportState::Working)]);
    }

    #[test]
    fn bel_terminator() {
        let got = scan_chunks(&[b"\x1b]7501;state=done\x07"]);
        assert_eq!(got, [plain(ReportState::Done)]);
    }

    #[test]
    fn esc_backslash_split() {
        let got = scan_chunks(&[b"\x1b]7501;state=idle\x1b", b"\\"]);
        assert_eq!(got, [plain(ReportState::Idle)]);
    }

    #[test]
    fn probe_inside_output() {
        let got = scan_chunks(&[b"abc\x1b]7501;?\x1b\\def"]);
        assert_eq!(
            got,
            [Scanned::Probe {
                end: 13,
                bel: false
            }]
        );
    }

    #[test]
    fn bel_probe() {
        let got = scan_chunks(&[b"abc\x1b]7501;?\x07def"]);
        assert_eq!(got, [Scanned::Probe { end: 12, bel: true }]);
    }

    #[test]
    fn probe_split_across_reads() {
        let got = scan_chunks(&[b"abc\x1b]7501", b";?\x1b\\def"]);
        assert_eq!(got, [Scanned::Probe { end: 4, bel: false }]);
    }

    #[test]
    fn two_probes() {
        let got = scan_chunks(&[b"\x1b]7501;?\x1b\\x\x1b]7501;?\x1b\\"]);
        assert_eq!(
            got,
            [
                Scanned::Probe {
                    end: 10,
                    bel: false
                },
                Scanned::Probe {
                    end: 21,
                    bel: false
                },
            ]
        );
    }

    #[test]
    fn probe_needs_exact_body() {
        assert_eq!(scan_chunks(&[b"\x1b]7501;?x\x1b\\"]), []);
        assert_eq!(scan_chunks(&[b"\x1b]7501;\x1b\\"]), []);
    }

    #[test]
    fn sub_record_dropped() {
        let got = scan_chunks(&[b"\x1b]7501;state=working:id=sub1\x1b\\"]);
        assert_eq!(got, []);
        let got = scan_chunks(&[b"\x1b]7501;id=sub1:state=working\x1b\\"]);
        assert_eq!(got, []);
    }

    #[test]
    fn clear_root() {
        let got = scan_chunks(&[b"\x1b]7501;state=clear\x1b\\"]);
        assert_eq!(got, [Scanned::Root(None)]);
    }

    #[test]
    fn unknown_state_dropped() {
        assert_eq!(scan_chunks(&[b"\x1b]7501;state=sleeping\x1b\\"]), []);
    }

    #[test]
    fn missing_state_dropped() {
        assert_eq!(
            scan_chunks(&[b"\x1b]7501;app=codex:kind=question\x1b\\"]),
            []
        );
    }

    #[test]
    fn fields_any_order_and_unknown_ignored() {
        let got = scan_chunks(&[b"\x1b]7501;app=codex:progress=40:state=working:x=1\x1b\\"]);
        assert_eq!(got, [root(ReportState::Working, None, Some("codex"))]);
    }

    #[test]
    fn kind_outside_set_dropped() {
        let got = scan_chunks(&[
            b"\x1b]7501;state=blocked:kind=permission\x1b\\",
            b"\x1b]7501;state=blocked:kind=question\x1b\\",
            b"\x1b]7501;state=blocked:kind=auth\x1b\\",
            b"\x1b]7501;state=blocked:kind=rage\x1b\\",
        ]);
        assert_eq!(
            got,
            [
                root(ReportState::Blocked, Some("permission"), None),
                root(ReportState::Blocked, Some("question"), None),
                root(ReportState::Blocked, Some("auth"), None),
                plain(ReportState::Blocked),
            ]
        );
    }

    #[test]
    fn app_with_bad_bytes_dropped() {
        let long = format!("\x1b]7501;state=idle:app={}\x1b\\", "a".repeat(65));
        let max = format!("\x1b]7501;state=idle:app={}\x1b\\", "a".repeat(64));
        let got = scan_chunks(&[
            b"\x1b]7501;state=idle:app=my.app_1-x\x1b\\",
            b"\x1b]7501;state=idle:app=bad app\x1b\\",
            b"\x1b]7501;state=idle:app=bad/app\x1b\\",
            b"\x1b]7501;state=idle:app=\x1b\\",
            long.as_bytes(),
            max.as_bytes(),
        ]);
        assert_eq!(
            got,
            [
                root(ReportState::Idle, None, Some("my.app_1-x")),
                plain(ReportState::Idle),
                plain(ReportState::Idle),
                plain(ReportState::Idle),
                plain(ReportState::Idle),
                root(ReportState::Idle, None, Some(&"a".repeat(64))),
            ]
        );
    }

    #[test]
    fn long_msg_keeps_state() {
        let mut report = b"\x1b]7501;state=blocked:kind=permission:msg=".to_vec();
        report.extend(std::iter::repeat_n(b'A', 8000));
        report.extend_from_slice(b"\x1b\\\x1b]7501;state=idle\x1b\\");
        // Fed in uneven pieces so the kept part and the discarded part span reads.
        let (a, b) = report.split_at(5000);
        let got = scan_chunks(&[a, b]);
        assert_eq!(
            got,
            [
                root(ReportState::Blocked, Some("permission"), None),
                plain(ReportState::Idle),
            ]
        );
    }

    #[test]
    fn field_cut_by_limit_dropped() {
        // "state=working:pad=" (18) + 4070 pad + ":app=cod" (8) is exactly MAX_KEPT_BODY,
        // so the body is cut in the middle of "app=codex".
        let mut report = b"\x1b]7501;state=working:pad=".to_vec();
        report.extend(std::iter::repeat_n(b'A', MAX_KEPT_BODY - 18 - 8));
        report.extend_from_slice(b":app=codex\x1b\\");
        let got = scan_chunks(&[&report]);
        assert_eq!(got, [plain(ReportState::Working)]);
    }

    #[test]
    fn cut_on_field_boundary_keeps_field() {
        // "pad=" + 4078 + ":state=working" is exactly MAX_KEPT_BODY and a ':' follows it.
        let mut report = b"\x1b]7501;pad=".to_vec();
        report.extend(std::iter::repeat_n(b'A', MAX_KEPT_BODY - 4 - 14));
        report.extend_from_slice(b":state=working:x=1\x1b\\");
        let got = scan_chunks(&[&report]);
        assert_eq!(got, [plain(ReportState::Working)]);
    }

    #[test]
    fn other_osc_untouched() {
        let got = scan_chunks(&[
            b"\x1b]2;t\x07",
            b"\x1b]7502;state=working\x07",
            b"\x1b]75010;state=working\x07",
            b"\x1b]7501x;state=working\x07",
            b"\x1b]750;state=working\x07",
        ]);
        assert_eq!(got, []);
    }

    #[test]
    fn can_aborts() {
        let got = scan_chunks(&[
            b"\x1b]7501;state=work\x18ing:state=done\x1b\\",
            b"\x1b]7501;state=work\x1aing:state=done\x1b\\",
            b"\x1b]7501;state=idle\x1b\\",
        ]);
        assert_eq!(got, [plain(ReportState::Idle)]);
    }

    #[test]
    fn esc_inside_body_restarts() {
        let got = scan_chunks(&[b"\x1b]7501;state=work\x1b]7501;state=idle\x07"]);
        assert_eq!(got, [plain(ReportState::Idle)]);
    }

    #[test]
    fn invalid_utf8_value() {
        let got = scan_chunks(&[
            b"\x1b]7501;state=blocked:kind=\xff\xfe:app=codex\x1b\\",
            b"\x1b]7501;state=blocked:kind=auth:app=\xc3\x28\x1b\\",
        ]);
        assert_eq!(
            got,
            [
                root(ReportState::Blocked, None, Some("codex")),
                root(ReportState::Blocked, Some("auth"), None),
            ]
        );
    }
}
