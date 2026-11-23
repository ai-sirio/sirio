//! A streamed chat reply, built part by part instead of whole on every chunk.
//!
//! [`StreamingDoc`] keeps the top-level blocks of the reply that cannot change any more as
//! frozen [`Segment`]s, and parses only the rest, the tail, on each chunk. The tree it returns
//! is the one [`build`] gives for the same text: Markdown reads some blocks by what stands
//! elsewhere in the reply, so a block is frozen only where that cannot differ, and the parts that
//! can are built again together (a rewind) or never frozen apart (a hold).
//!
//! What a block can depend on beyond itself, and what is done about it:
//!
//! - **A link or footnote definition.** The references it answers read it wherever it stands.
//!   Every part that writes `[label` for a defined label stays in one parse: a definition that
//!   arrives after a frozen reference rewinds to it, a reference after a frozen definition
//!   rewinds to the definition, and no cut falls between the first and the last mention.
//! - **Footnote numbers.** Ely numbers a footnote by its first reference in the reply, so a part
//!   would number from one: once a frozen segment holds a reference, a reference in the tail
//!   rewinds to it, and no cut falls between two references.
//! - **`$$` pairs.** Ely pairs every run of two dollars outside code with the next one, from the
//!   top of the reply, and a fence pair is a math block across the blocks between. The cut never
//!   falls inside a pair, nor passes a run that is still waiting for its partner.
//! - **Block HTML room.** It is the cap less the deepest Markdown of the whole reply. Each
//!   segment keeps the deepest it was built under, and is built again when that changes.
//! - **A block start that a line to come can undo.** A block that starts right under a line of
//!   text is told by the lines after it (a table needs its delimiter row, a definition goes on
//!   over its title), and what the text above is changes with them. After a blank line a block
//!   starts whatever follows, but the line it starts on may still change under it: `2` is a
//!   paragraph, `2.` the next item of the list above (`#` and then `#tag` too). So a cut falls
//!   only before a block that follows a blank line, and not on the line being written.
//! - **A byte order mark.** Ely drops one from the start of the text it reads and keeps any
//!   other as text, so no part starts at one except the first.
//! - **The nesting cap.** A part the guard refuses makes the whole reply one plain paragraph, as
//!   [`build`] does: from then on every chunk is a full build.
//!
//! `\(` pairs read inside one paragraph, heading or table cell, so they cross no block.

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use ely_gpui_component::documents::{MarkdownBlock, markdown_options};
use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};

use super::{
    MAX_NESTING, MarkdownBlocks, Parts, Policy, Within, as_plain_text, build, join, parts, read_of,
    rewrite,
};
use crate::markdown_preview::DiagramRequest;

/// Moves every source offset the tree keeps by `by`, wrapping: the first byte of the part it was
/// built from, in the reply it is a part of. `Code.from`, `Image.at` and the spans of an
/// inline's pieces count bytes of the source Ely read, and Ely read the part alone.
pub(super) fn shift(nodes: &mut [MarkdownBlock], by: usize) {
    if by == 0 {
        return;
    }
    rewrite::each_mut(nodes, &mut |node| match node {
        MarkdownBlock::Paragraph(inline) | MarkdownBlock::Heading(_, inline) => {
            for (_, span, _) in &mut inline.sources {
                *span = span.start.wrapping_add(by)..span.end.wrapping_add(by);
            }
        }
        MarkdownBlock::Code { from, .. } => *from = from.wrapping_add(by),
        MarkdownBlock::Image { at, .. } => *at = at.wrapping_add(by),
        _ => {}
    });
}

/// A definition's label, and how a reference writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Label {
    name: String,
    /// A footnote, written `[^name]`; else a link reference, `[name]`.
    note: bool,
}

/// What the brackets of a text write, found in one pass, so that asking whether a label is
/// mentioned is a lookup, and not a pass over the text for each label. A mention is the whole
/// label (`[10]` is no mention of label `1`), under the rules pulldown matches by: case does not
/// tell labels apart, runs of white space are one space, and none counts at the ends. What is not
/// ours to decide is taken for a mention, which only keeps parts together: the labels of other
/// scripts fold by rules we do not have, so any bracket may name one, and so may a label too long
/// to scan or broken over lines. A bracket that is not
/// closed yet (the text so far ends in it) is no reference to anything, whatever follows: it reads
/// as text until its `]` arrives, and the chunk it arrives in looks again.
#[derive(Clone, Debug, Default)]
struct Brackets {
    /// By label (see [`label_key`]): the first and the last `[` that write it as a link reference,
    /// `[label]`, and as a footnote reference, `[^label]`.
    links: HashMap<String, (usize, usize)>,
    notes: HashMap<String, (usize, usize)>,
    /// The first and the last `[` whose label is not ASCII, or not ours to read (see
    /// [`Written::Anything`]): it may be any ASCII label.
    foreign_links: Option<(usize, usize)>,
    foreign_notes: Option<(usize, usize)>,
    /// The first and the last `[` of the text, and of the `[^` ones: where a label that is not
    /// ASCII is mentioned, since any of them may name it.
    any_link: Option<(usize, usize)>,
    any_note: Option<(usize, usize)>,
}

/// `range` taking in the place `at`.
fn widen(range: &mut Option<(usize, usize)>, at: usize) {
    *range = Some(range.map_or((at, at), |(first, _)| (first, at)));
}

/// The two ranges as one.
fn merged(a: Option<(usize, usize)>, b: Option<(usize, usize)>) -> Option<(usize, usize)> {
    match (a, b) {
        (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.max(b.1))),
        (a, b) => a.or(b),
    }
}

/// A label as labels are told apart: its words in lower case, one space between them.
fn label_key(label: &str) -> String {
    let words: Vec<String> = label.split_whitespace().map(str::to_ascii_lowercase).collect();
    words.join(" ")
}

/// A panic a test makes the stream's own passes over a text raise: the scan of what Ely read
/// ([`Scan::of`]) and the pass that finds where the `$$` runs may pair ([`not_prose`]). Both run
/// pulldown-cmark on text that no guard has parsed yet, so a panic in it unwinds from there the
/// way one left in the vendored parser would. Compiled into tests only.
#[cfg(test)]
pub(crate) mod scan_fault {
    use std::cell::Cell;

    thread_local!(static ARMED: Cell<Option<usize>> = const { Cell::new(None) });
    thread_local!(static STRIKES: Cell<usize> = const { Cell::new(0) });

    /// While it lives, the passes over a text on this thread panic, from the one after the first
    /// `skip` of them on, and their count begins again from zero.
    pub(crate) struct ScanFault(());

    /// The first `skip` passes go through; every one after them panics.
    pub(crate) fn on_after(skip: usize) -> ScanFault {
        ARMED.with(|armed| armed.set(Some(skip)));
        STRIKES.with(|strikes| strikes.set(0));
        ScanFault(())
    }

    /// How many passes were made since the fault was set: the ones that went through, the one
    /// that panicked, and every one made after it.
    pub(crate) fn strikes() -> usize {
        STRIKES.with(Cell::get)
    }

    impl Drop for ScanFault {
        fn drop(&mut self) {
            ARMED.with(|armed| armed.set(None));
        }
    }

    pub(super) fn strike() {
        let Some(skip) = ARMED.with(Cell::get) else {
            return;
        };
        let made = STRIKES.with(|strikes| {
            strikes.set(strikes.get() + 1);
            strikes.get()
        });
        if made > skip {
            panic!("a panic injected into a pass over a streamed text");
        }
    }
}

impl Brackets {
    fn of(text: &str) -> Self {
        let mut found = Self::default();
        for (at, _) in text.match_indices('[') {
            let rest = &text[at + 1..];
            widen(&mut found.any_link, at);
            found.write(rest, at, false);
            if let Some(rest) = rest.strip_prefix('^') {
                widen(&mut found.any_note, at);
                found.write(rest, at, true);
            }
        }
        found
    }

    /// The label that `rest`, what follows the `[` at `at`, writes as a link or as a footnote.
    fn write(&mut self, rest: &str, at: usize, note: bool) {
        let (labels, foreign) = match note {
            true => (&mut self.notes, &mut self.foreign_notes),
            false => (&mut self.links, &mut self.foreign_links),
        };
        let written = match label_written(rest) {
            Written::Nothing => return,
            Written::Anything => {
                widen(foreign, at);
                return;
            }
            Written::Label(written) => written,
        };
        // Case folding writes some ASCII letters in other scripts (`ß` is `ss`).
        if !written.is_ascii() {
            widen(foreign, at);
            return;
        }
        let mut keys = vec![label_key(written)];
        // A table reads `\|` as `|` before it reads a label.
        if written.contains("\\|") {
            keys.push(label_key(&written.replace("\\|", "|")));
        }
        for key in keys {
            let range = labels.entry(key).or_insert((at, at));
            range.1 = at;
        }
    }

    /// The first and the last place the text writes `[` and `label`, if it does.
    fn mentions(&self, label: &Label) -> Option<(usize, usize)> {
        let (labels, foreign, any) = match label.note {
            true => (&self.notes, self.foreign_notes, self.any_note),
            false => (&self.links, self.foreign_links, self.any_link),
        };
        if !label.name.is_ascii() {
            return any;
        }
        merged(labels.get(&label_key(&label.name)).copied(), foreign)
    }
}

/// What the bytes after a `[` write.
enum Written<'a> {
    /// A label, as far as the text shows it: up to the first `]` that is not backslash-escaped.
    Label(&'a str),
    /// No label: a `[` that is not escaped comes first (a label holds none), or no `]` does yet.
    Nothing,
    /// A label that is not ours to read, so any label may be it: one too long to scan (pulldown
    /// limits only what counts white space, escapes and bytes that are not ASCII against its
    /// thousand, so it takes an ASCII label of any length), or one that goes over a line end (the
    /// `>` of a block quote, and what else a container writes at the start of a line, is dropped
    /// from a label by pulldown, which then reads "one two" where the text has "one > two").
    Anything,
}

/// What `rest`, the text after a `[`, writes.
fn label_written(rest: &str) -> Written<'_> {
    const SCANNED: usize = 4096;
    let bytes = rest.as_bytes();
    let mut at = 0;
    while at < bytes.len() && at < SCANNED {
        match bytes[at] {
            b']' => {
                let written = &rest[..at];
                return match written.contains(['\n', '\r']) {
                    true => Written::Anything,
                    false => Written::Label(written),
                };
            }
            b'[' => return Written::Nothing,
            b'\\' if bytes.get(at + 1).is_some_and(u8::is_ascii_punctuation) => at += 2,
            _ => at += 1,
        }
    }
    // Neither a closing bracket nor an opening one in all that was scanned, and more text to go.
    match at < bytes.len() {
        true => Written::Anything,
        false => Written::Nothing,
    }
}

/// What one pass over the text Ely read says of its blocks.
#[derive(Default)]
struct Scan {
    /// Where each top-level block starts, in bytes of that text.
    starts: Vec<usize>,
    /// The labels it defines, links and footnotes.
    labels: Vec<Label>,
    /// Where it writes a footnote reference.
    notes: Vec<usize>,
    /// How deep it nests: `markdown_depth` of the text, from the same events.
    deepest: usize,
    /// Whether it holds HTML, block or inline: what Ely hands to the host to rewrite.
    html: bool,
}

impl Scan {
    fn of(read: &str) -> Self {
        #[cfg(test)]
        scan_fault::strike();
        let mut found = Self::default();
        let mut depth = 0usize;
        let mut events = Parser::new_ext(read, markdown_options()).into_offset_iter();
        for (event, range) in events.by_ref() {
            match event {
                Event::Start(tag) => {
                    if depth == 0 {
                        found.starts.push(range.start);
                    }
                    if let Tag::FootnoteDefinition(name) = &tag {
                        found.labels.push(Label {
                            name: name.to_string(),
                            note: true,
                        });
                    }
                    depth += 1;
                    found.deepest = found.deepest.max(depth);
                }
                Event::End(_) => depth = depth.saturating_sub(1),
                Event::Rule if depth == 0 => found.starts.push(range.start),
                Event::FootnoteReference(_) => found.notes.push(range.start),
                Event::Html(_) | Event::InlineHtml(_) => found.html = true,
                _ => {}
            }
        }
        for (name, _) in events.reference_definitions().iter() {
            found.labels.push(Label {
                name: name.to_owned(),
                note: false,
            });
        }
        found
    }
}

/// Where the text Ely read stands in the source it read: a byte order mark is dropped, a lone CR
/// is a line end of the same width, and a `$$` fence is a run of tildes one longer. An offset
/// inside a fence, or after text that reads otherwise, maps nowhere.
struct Alignment {
    /// A stretch of the read text, and where it starts in the source.
    pieces: Vec<(Range<usize>, usize)>,
}

impl Alignment {
    fn of(read: &str, source: &str) -> Self {
        let bom = if source.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 };
        let (read, written) = (read.as_bytes(), &source.as_bytes()[bom..]);
        let (mut i, mut j) = (0, 0);
        let (mut from_i, mut from_j) = (0, 0);
        let mut pieces = Vec::new();
        loop {
            while i < read.len()
                && j < written.len()
                && (read[i] == written[j]
                    || (read[i] == b'\n'
                        && written[j] == b'\r'
                        && written.get(j + 1) != Some(&b'\n')))
            {
                i += 1;
                j += 1;
            }
            pieces.push((from_i..i, from_j + bom));
            let dollars = written[j..].iter().take_while(|byte| **byte == b'$').count();
            let tildes = read[i..].iter().take_while(|byte| **byte == b'~').count();
            if dollars < 2 || tildes != dollars + 1 {
                break;
            }
            (i, j) = (i + tildes, j + dollars);
            (from_i, from_j) = (i, j);
        }
        Self { pieces }
    }

    fn map(&self, at: usize) -> Option<usize> {
        (self.pieces.iter())
            .find(|(read, _)| read.start <= at && at <= read.end)
            .map(|(read, source)| source + at - read.start)
    }
}

/// Where the line holding byte `at` of `text` starts: a block never starts after the indent or
/// the marker of its own line, and a part that did would read an indented code block as a
/// paragraph.
fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind(['\n', '\r']).map_or(0, |end| end + 1)
}

/// Where the `$$` runs of a text pair. Ely pairs them from the top of the reply: every run of
/// two dollars or more outside code takes the next as long (Ely's `flow::paired`), whatever
/// stands between the two.
#[derive(Default)]
struct Dollars {
    /// Each pair, from the first byte of its opening run to the last of its closing one.
    pairs: Vec<Range<usize>>,
    /// The first run that has no partner yet.
    open: Option<usize>,
}

impl Dollars {
    fn of(text: &str) -> Self {
        let mut found = Self::default();
        if !text.contains("$$") {
            return found;
        }
        let text = lone_crs_as_lf(text);
        let held = not_prose(&text);
        let bytes = text.as_bytes();
        let mut at = 0;
        while let Some(open) = next_run(bytes, at, 2, &held) {
            match next_run(bytes, open.end, open.len(), &held) {
                Some(close) => {
                    found.pairs.push(open.start..close.end);
                    at = close.end;
                }
                None => {
                    found.open = Some(open.start);
                    break;
                }
            }
        }
        found
    }

    /// Whether a part may start at byte `at`. A pair is settled whole: a part that starts
    /// between its runs would pair them with others. A run that has no partner yet may take one
    /// that arrives, and everything after it with it.
    fn allows(&self, at: usize) -> bool {
        self.open.is_none_or(|open| at <= open)
            && !self.pairs.iter().any(|pair| pair.start < at && at < pair.end)
    }
}

/// Whether the line before the one that starts at byte `at` of `text` (a line start after the
/// first) is blank. A block that follows a blank line starts there whatever the lines after it
/// say; under a line of text it starts only if they say so.
fn after_blank_line(text: &str, at: usize) -> bool {
    let before = &text[..at];
    let before = (before.strip_suffix("\r\n"))
        .or_else(|| before.strip_suffix(['\n', '\r']))
        .unwrap_or(before);
    let line = &before[before.rfind(['\n', '\r']).map_or(0, |end| end + 1)..];
    line.bytes().all(|byte| matches!(byte, b' ' | b'\t'))
}

/// `text` with each lone CR as LF: what pulldown-cmark ends a line at, of the same width.
fn lone_crs_as_lf(text: &str) -> Cow<'_, str> {
    if !text.contains('\r') {
        return Cow::Borrowed(text);
    }
    let mut bytes = text.as_bytes().to_vec();
    for at in 0..bytes.len() {
        if bytes[at] == b'\r' && bytes.get(at + 1) != Some(&b'\n') {
            bytes[at] = b'\n';
        }
    }
    Cow::Owned(String::from_utf8(bytes).expect("LF for CR keeps UTF-8"))
}

/// All but prose, as the text reads without math: code, HTML, link targets (Ely's
/// `flow::not_prose`, which `$$` runs are not paired inside).
fn not_prose(text: &str) -> Vec<Range<usize>> {
    #[cfg(test)]
    scan_fault::strike();
    let mut held = Vec::new();
    let (mut at, mut quiet) = (0, 0usize);
    let mut links = Vec::new();
    let plain = markdown_options().difference(Options::ENABLE_MATH);
    for (event, range) in Parser::new_ext(text, plain).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_) | Tag::MetadataBlock(_) | Tag::Image { .. }) => {
                quiet += 1;
            }
            Event::End(TagEnd::CodeBlock | TagEnd::MetadataBlock(_) | TagEnd::Image) => {
                quiet = quiet.saturating_sub(1);
            }
            Event::Start(Tag::Link { link_type, .. }) => {
                let auto = matches!(link_type, LinkType::Autolink | LinkType::Email);
                quiet += usize::from(auto);
                links.push(auto);
            }
            Event::End(TagEnd::Link) => {
                quiet = quiet.saturating_sub(usize::from(links.pop() == Some(true)));
            }
            Event::Text(_) if quiet == 0 && range.start >= at => {
                held.push(at..range.start);
                at = range.end;
            }
            _ => {}
        }
    }
    held.push(at..text.len());
    held.retain(|gap| !gap.is_empty());
    held
}

/// The first run of at least `len` dollars from byte `from`, outside `held`.
fn next_run(
    bytes: &[u8],
    from: usize,
    len: usize,
    held: &[Range<usize>],
) -> Option<Range<usize>> {
    let mut at = from;
    // `held` is in order and its ranges do not overlap, so the only range that can hold `at` is
    // the first that ends after it: a pointer that only moves on, not a search from the top.
    let mut next = held.partition_point(|block| block.end <= at);
    while at < bytes.len() {
        while held.get(next).is_some_and(|block| block.end <= at) {
            next += 1;
        }
        if let Some(block) = held.get(next).filter(|block| block.start <= at) {
            at = block.end;
            continue;
        }
        let run = bytes[at..].iter().take_while(|byte| **byte == b'$').count();
        if run >= len {
            return Some(at..at + run);
        }
        at += run.max(1);
    }
    None
}

/// One frozen stretch of the reply and the nodes built for exactly that stretch.
#[derive(Clone, Debug)]
pub(crate) struct Segment {
    pub range: Range<usize>,
    pub nodes: Rc<[MarkdownBlock]>,
    /// Block HTML was read in it, so the Markdown depth of the reply decided its room.
    pub has_block_html: bool,
    missing: Rc<[DiagramRequest]>,
    /// The deepest nesting of its own Markdown.
    deepest: usize,
    /// The deepest of the whole reply when it was built: what its block HTML had room for.
    room: usize,
    /// The labels it defines.
    labels: Vec<Label>,
    /// Whether it writes a footnote reference.
    notes: bool,
    /// What its brackets write: which labels it mentions.
    brackets: Brackets,
}

/// A reply that streams in, built part by part. `text` only grows between two calls of
/// [`update`](Self::update); a text that does not start with the bytes frozen so far is no
/// continuation of the reply, and starts over.
#[derive(Clone, Debug, Default)]
pub(crate) struct StreamingDoc {
    /// Everything before it is frozen: the segments tile `text[..cut]`.
    cut: usize,
    segments: Vec<Segment>,
    /// The bytes frozen, `text[..cut]`: what a text must start with to continue the reply.
    frozen: String,
    /// The nesting guard refused a part: every build is a build of the whole reply.
    whole: bool,
    /// Ely's reader panicked on a part, or on the whole reply: the stream asks it no more, and
    /// answers with the text as plain text, marked (`whole` is set too).
    panicked: bool,
    /// Until the reply is this long the stream does not look at it again: a chunk is the build of
    /// its text (see [`HOLD_FROM`]).
    hold_until: usize,
}

/// A reply from this length on, whose stream looked at it and froze nothing with most of the
/// reply still in the tail, is not looked at again until it has grown by a quarter. Looking is
/// reading the tail, scanning it and matching its labels, on top of the parse of it: where nothing
/// can freeze (one long fence, a reply with no blank line, a reply whose labels are cited from
/// its first block to its last) that is paid on every chunk and gains nothing. Under it a build is
/// too short for that to matter.
const HOLD_FROM: usize = 4096;

impl StreamingDoc {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The frozen segments, which tile the text up to [`cut`](Self::cut). They can lag the text
    /// (the wait after a fruitless look, a deep rewind), and unless the stream is
    /// [whole](Self::is_whole) they are a true prefix of what [`build`] makes of it: the reveal
    /// draws them as they stand.
    pub(crate) fn segments(&self) -> &[Segment] {
        &self.segments
    }

    // Read by the tests.
    #[allow(dead_code)]
    pub(crate) fn cut(&self) -> usize {
        self.cut
    }

    /// The nesting guard refused a part: every build of this reply is a build of the whole of it,
    /// and the segments left standing are not a prefix of what [`build`] shows.
    pub(crate) fn is_whole(&self) -> bool {
        self.whole
    }

    /// The tree of `text`, the reply so far: what [`build`] returns for it.
    ///
    /// What the tail says about itself (the text Ely reads of it, where its blocks start, which
    /// labels it defines, how deep it nests) is read before anything of it is built into a tree.
    /// So a rewind costs a pass over the text, not a parse of it, and what freezes is parsed once,
    /// as a segment, and not again as part of the tail: after the cut moves a long way (a rewound
    /// stream freezing everything it held) only the new tail is parsed besides.
    ///
    /// `build(text)` is what this must equal, so it is always an answer, and the stream gives it
    /// where looking at the reply would cost more than it saves: a rewind that leaves most of the
    /// reply to parse, and the wait that follows an analysis that froze nothing (`hold_until`).
    /// The segments and the cut are left as they were then, which is a state the next look
    /// starts from: the segments tile the text up to the cut.
    ///
    /// Everything it looks at is inside one guard. Ely's reader is guarded where it parses, but
    /// the stream parses with pulldown-cmark itself, to scan the tail and to find where `$$`
    /// runs pair, and those are the first parses of a text that nothing has guarded yet: a panic
    /// in any of them (or in what slices by what they found) answers as one in the reader does.
    pub(crate) fn update(&mut self, text: &str, policy: &mut Policy<'_>) -> MarkdownBlocks {
        if !text.starts_with(&self.frozen) {
            *self = Self::new();
        }
        if self.panicked {
            return as_plain_text(text);
        }
        if self.whole || text.len() < self.hold_until {
            return self.build_whole(text, policy);
        }
        match catch_unwind(AssertUnwindSafe(|| self.update_in_parts(text, policy))) {
            Ok(document) => document,
            Err(_) => self.plain(text),
        }
    }

    /// [`update`](Self::update) once the stream has decided to look at the reply: the tail is read
    /// and scanned before it is built, and what can freeze does.
    fn update_in_parts(&mut self, text: &str, policy: &mut Policy<'_>) -> MarkdownBlocks {
        let (read, scan, brackets, depth, math) = loop {
            let tail = &text[self.cut..];
            let Some(read) = read_of(tail) else {
                return self.plain(text);
            };
            let scan = Scan::of(&read);
            let brackets = Brackets::of(tail);
            let depth = scan.deepest;
            // Ely reads `\(` and `\[` pairs in a second pass, over a text it swaps itself, and the
            // depth of that text, which block HTML takes its room from, only a parse of the tail
            // tells: a reply with both is read the way that parses the tail first. A tail nested
            // past the cap is refused there too.
            let math = tail.contains("\\(") || tail.contains("\\[");
            if depth > MAX_NESTING
                || (math && (scan.html || self.segments.iter().any(|s| s.has_block_html)))
            {
                return self.update_by_parsing_the_tail(text, policy);
            }
            match self.rewind_from(depth, &scan, &brackets) {
                Some(index) => {
                    self.truncate(index);
                    // A rewind that leaves most of the reply to parse costs no more as a build,
                    // and it skips the work of freezing what is held again: that is for the
                    // chunks after it.
                    if self.cut < text.len() - self.cut {
                        return self.build_whole(text, policy);
                    }
                }
                None => break (read, scan, brackets, depth, math),
            }
        };
        let deepest = self.deepest().max(depth);
        let cut_before = self.cut;
        self.freeze(text, &read, &scan, &brackets, deepest, policy);
        if self.panicked {
            return as_plain_text(text);
        }
        if self.whole {
            return self.build_whole(text, policy);
        }
        let tail = parts(
            &text[self.cut..],
            policy,
            Within {
                deepest: self.deepest(),
                offset: self.cut,
            },
            false,
        );
        if tail.panicked {
            return self.plain(text);
        }
        if tail.refused || tail.declined {
            self.whole = true;
            return self.build_whole(text, policy);
        }
        // That a reply with `\(` pairs has no block HTML was the scan's word: if the parse says
        // otherwise, the rooms were not right, and the reply is built whole from here on.
        if math && (tail.block_html || self.segments.iter().any(|s| s.has_block_html)) {
            self.whole = true;
            return self.build_whole(text, policy);
        }
        self.wait_if_nothing_froze(text.len(), cut_before);
        self.joined(tail)
    }

    /// [`update`](Self::update) for a tail that cannot be read before it is built: the tail is
    /// parsed, then the segments are looked at, then what can freeze does.
    fn update_by_parsing_the_tail(&mut self, text: &str, policy: &mut Policy<'_>) -> MarkdownBlocks {
        let (tail, read, scan, brackets) = loop {
            let mut tail = parts(
                &text[self.cut..],
                policy,
                Within {
                    deepest: self.deepest(),
                    offset: self.cut,
                },
                true,
            );
            if tail.panicked {
                return self.plain(text);
            }
            if tail.refused || tail.declined {
                self.whole = true;
                return self.build_whole(text, policy);
            }
            let read = tail.read.take().unwrap_or_default();
            let scan = Scan::of(&read);
            let brackets = Brackets::of(&text[self.cut..]);
            match self.rewind_from(tail.deepest, &scan, &brackets) {
                Some(index) => {
                    self.truncate(index);
                    // As in `update`: a rewind that leaves most of the reply to parse is a build.
                    if self.cut < text.len() - self.cut {
                        return self.build_whole(text, policy);
                    }
                }
                None => break (tail, read, scan, brackets),
            }
        };
        let deepest = self.deepest().max(tail.deepest);
        let document = self.joined(tail);
        let cut_before = self.cut;
        self.freeze(text, &read, &scan, &brackets, deepest, policy);
        if self.panicked {
            return as_plain_text(text);
        }
        self.wait_if_nothing_froze(text.len(), cut_before);
        document
    }

    /// The build of the whole of `text`, which is what a stream answers when looking at the reply
    /// in parts would cost more than it saves. The reader may panic in it too, and then the
    /// stream is done with it, as for a panic in a part.
    fn build_whole(&mut self, text: &str, policy: &mut Policy<'_>) -> MarkdownBlocks {
        let document = build(text, policy);
        if document.reader_panicked {
            self.panicked = true;
            self.whole = true;
        }
        document
    }

    /// The answer when the reader panicked on a part of `text`: the whole of it as plain text,
    /// marked. A build of it would ask the reader again, and it would panic again, in the same
    /// update. From here on the stream answers so without asking.
    fn plain(&mut self, text: &str) -> MarkdownBlocks {
        self.panicked = true;
        self.whole = true;
        as_plain_text(text)
    }

    /// After a look that froze nothing, with most of the reply still in the tail and the reply
    /// long enough for it to matter, the stream does not look again until the reply has grown by
    /// a quarter ([`HOLD_FROM`]).
    fn wait_if_nothing_froze(&mut self, length: usize, cut_before: usize) {
        if length >= HOLD_FROM && self.cut == cut_before && (length - self.cut) * 2 >= length {
            self.hold_until = length + length / 4;
        }
    }

    /// The tree of the frozen segments followed by the nodes of `tail`.
    fn joined(&self, tail: Parts) -> MarkdownBlocks {
        join_segments(&self.segments, tail)
    }

    /// The deepest nesting of the Markdown of the frozen segments.
    fn deepest(&self) -> usize {
        (self.segments.iter())
            .map(|segment| segment.deepest)
            .max()
            .unwrap_or(0)
    }

    /// Frozen segments back to `index` become tail again.
    fn truncate(&mut self, index: usize) {
        self.cut = self.segments[index].range.start;
        self.segments.truncate(index);
        self.frozen.truncate(self.cut);
    }

    /// The first frozen segment the tail makes wrong, if there is one.
    fn rewind_from(
        &self,
        tail_deepest: usize,
        scan: &Scan,
        brackets: &Brackets,
    ) -> Option<usize> {
        let mut first: Option<usize> = None;
        let mut note = |index: usize| first = Some(first.map_or(index, |known| known.min(index)));

        // Block HTML took its room from another depth than the reply has now.
        let reply = self.deepest().max(tail_deepest);
        if let Some(index) = (self.segments.iter())
            .position(|segment| segment.has_block_html && segment.room != reply)
        {
            note(index);
        }

        // A label written in the tail, and in a frozen part too: one parse, or the reference
        // and the definition may not meet.
        let frozen = self.segments.iter().flat_map(|segment| segment.labels.iter());
        for label in frozen.chain(scan.labels.iter()) {
            if brackets.mentions(label).is_none() {
                continue;
            }
            if let Some(index) = (self.segments.iter())
                .position(|segment| segment.brackets.mentions(label).is_some())
            {
                note(index);
            }
        }

        // Footnotes number from the first reference of the reply, not of the part.
        if !scan.notes.is_empty()
            && let Some(index) = self.segments.iter().position(|segment| segment.notes)
        {
            note(index);
        }
        first
    }

    /// Freezes what can no longer change: every block before the last that can be left alone.
    fn freeze(
        &mut self,
        text: &str,
        read: &str,
        scan: &Scan,
        brackets: &Brackets,
        deepest: usize,
        policy: &mut Policy<'_>,
    ) {
        let tail = &text[self.cut..];
        let alignment = Alignment::of(read, tail);

        // Where a part may start: a line that starts a top-level block, mapped back to the source.
        let mut starts: Vec<usize> = (scan.starts.iter())
            .filter_map(|at| alignment.map(*at))
            .map(|at| line_start(tail, at))
            .filter(|at| *at > 0)
            .collect();
        starts.dedup();
        // Between the first and the last place a defined label is written, a cut would part a
        // reference from its definition. Between two footnote references it would number the
        // second from one. Inside a `$$` pair, or past a run that waits for a partner, it would
        // pair the runs after it elsewhere.
        let mut spans: Vec<(usize, usize)> =
            (scan.labels.iter()).filter_map(|label| brackets.mentions(label)).collect();
        let notes: Vec<usize> = (scan.notes.iter())
            .map(|at| alignment.map(*at).unwrap_or(usize::MAX))
            .collect();
        if let (Some(first), Some(last)) = (notes.iter().min(), notes.iter().max()) {
            spans.push((*first, *last));
        }
        let dollars = Dollars::of(tail);
        // The line being written: the block that starts on it may be none once it is done, or
        // taken up by the block above (`#` and `#tag`, `2` and `2.` after a list item).
        let unfinished = (!tail.ends_with(['\n', '\r'])).then(|| line_start(tail, tail.len()));
        starts.retain(|at| {
            after_blank_line(tail, *at)
                && Some(*at) != unfinished
                && !tail[*at..].starts_with('\u{feff}')
                && dollars.allows(*at)
                && !spans.iter().any(|(first, last)| first < at && at <= last)
        });

        let mut from = 0;
        for at in starts {
            let range = self.cut + from..self.cut + at;
            let made = parts(
                &text[range.clone()],
                policy,
                Within {
                    deepest,
                    offset: range.start,
                },
                true,
            );
            if made.panicked {
                self.panicked = true;
                self.whole = true;
                break;
            }
            if made.refused || made.declined {
                // The segments made so far stay: they tile the text up to the cut.
                self.whole = true;
                break;
            }
            let own = Scan::of(made.read.as_deref().unwrap_or_default());
            let brackets = Brackets::of(&text[range.clone()]);
            self.segments.push(Segment {
                range,
                nodes: Rc::from(made.nodes),
                has_block_html: made.block_html,
                missing: Rc::from(made.missing),
                deepest: made.deepest,
                room: deepest.max(made.deepest),
                labels: own.labels,
                notes: !own.notes.is_empty(),
                brackets,
            });
            from = at;
        }
        self.frozen.push_str(&text[self.cut..self.cut + from]);
        self.cut += from;
    }
}

/// The tree of `segments` followed by the nodes of `tail`.
fn join_segments(segments: &[Segment], mut tail: Parts) -> MarkdownBlocks {
    let mut missing: Vec<DiagramRequest> = Vec::new();
    let wanted = segments.iter().flat_map(|segment| segment.missing.iter());
    for request in wanted.chain(tail.missing.iter()) {
        if !missing.iter().any(|known| known.key == request.key) {
            missing.push(request.clone());
        }
    }
    let mut nodes = Vec::new();
    for segment in segments {
        nodes.extend(segment.nodes.iter().cloned());
    }
    nodes.append(&mut tail.nodes);
    join(nodes, missing, tail.panicked)
}

/// The tree of `segments` followed by `tail`, the text of the reply from where the last of them
/// ends: what the reveal draws of a reply that is still arriving. `tail` is built as a part of
/// that reply, as the stream builds its own (its source offsets counted from the reply, its block
/// HTML nested to what the deepest Markdown of the segments leaves), and the segments are taken as
/// they stand: they are not parsed again. They are cloned into the joined tree, and the join
/// (headings, tables of contents, counts, plain text) walks all of it, so a frame costs the tail's
/// parse and a walk of the whole drawn tree. The reveal keeps a frame's result while nothing it
/// depends on moves.
pub(crate) fn with_tail(segments: &[Segment], tail: &str, policy: &mut Policy<'_>) -> MarkdownBlocks {
    let offset = segments.last().map_or(0, |segment| segment.range.end);
    let deepest = segments.iter().map(|segment| segment.deepest).max().unwrap_or(0);
    join_segments(
        segments,
        parts(tail, policy, Within { deepest, offset }, false),
    )
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod stream_tests;
