//! The bundled font pairs: an interface family and the code family that suits
//! it, with the face bytes that make both present on every platform.
//!
//! A pair is the unit the person chooses (Settings, Interface, Font). The
//! table here is the one place that names a pair's families, its faces and
//! the slot each face fills; [`crate::Theme::set_font_pair`] writes the
//! families into the theme, [`crate::register_ui_fonts`] registers the bytes.
//! Every family is SIL OFL 1.1; provenance and checksums are in
//! `rust/assets/fonts/README.md` and `THIRD_PARTY_NOTICES.md`.

/// What a face is drawn for: the interface family or the code family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FaceRole {
    Ui,
    Code,
}

/// One bundled font file and the slot it is declared to fill. The slot is
/// what the file really says (a test reads the file and compares), so the
/// table cannot drift from the bytes.
#[derive(Clone, Copy, Debug)]
pub struct Face {
    pub role: FaceRole,
    /// The `usWeightClass` of the file (Red Hat's SemiBold is 632).
    pub weight: u16,
    pub italic: bool,
    pub bytes: &'static [u8],
}

macro_rules! face {
    ($role:ident, $weight:expr, $italic:expr, $path:literal) => {
        Face {
            role: FaceRole::$role,
            weight: $weight,
            italic: $italic,
            bytes: include_bytes!(concat!("../../../assets/fonts/", $path)),
        }
    };
}
static ATKINSON_FACES: [Face; 7] = [
    face!(Ui, 400, false, "atkinson/AtkinsonHyperlegibleNext-Regular.ttf"),
    face!(Ui, 500, false, "atkinson/AtkinsonHyperlegibleNext-Medium.ttf"),
    face!(Ui, 600, false, "atkinson/AtkinsonHyperlegibleNext-SemiBold.ttf"),
    face!(Ui, 700, false, "atkinson/AtkinsonHyperlegibleNext-Bold.ttf"),
    face!(Ui, 400, true, "atkinson/AtkinsonHyperlegibleNext-Italic.ttf"),
    face!(Code, 400, false, "atkinson/AtkinsonHyperlegibleMono-Regular.ttf"),
    face!(Code, 700, false, "atkinson/AtkinsonHyperlegibleMono-Bold.ttf"),
];
static INTER_FACES: [Face; 7] = [
    face!(Ui, 400, false, "inter/Inter-Regular.ttf"),
    face!(Ui, 500, false, "inter/Inter-Medium.ttf"),
    face!(Ui, 600, false, "inter/Inter-SemiBold.ttf"),
    face!(Ui, 700, false, "inter/Inter-Bold.ttf"),
    face!(Ui, 400, true, "inter/Inter-Italic.ttf"),
    face!(Code, 400, false, "inter/JetBrainsMono-Regular.ttf"),
    face!(Code, 700, false, "inter/JetBrainsMono-Bold.ttf"),
];
static PLEX_FACES: [Face; 7] = [
    face!(Ui, 400, false, "plex/IBMPlexSans-Regular.ttf"),
    face!(Ui, 500, false, "plex/IBMPlexSans-Medium.ttf"),
    face!(Ui, 600, false, "plex/IBMPlexSans-SemiBold.ttf"),
    face!(Ui, 700, false, "plex/IBMPlexSans-Bold.ttf"),
    face!(Ui, 400, true, "plex/IBMPlexSans-Italic.ttf"),
    face!(Code, 400, false, "plex/IBMPlexMono-Regular.ttf"),
    face!(Code, 700, false, "plex/IBMPlexMono-Bold.ttf"),
];
static REDHAT_FACES: [Face; 5] = [
    face!(Ui, 400, false, "redhat/RedHatText-Regular.ttf"),
    face!(Ui, 632, false, "redhat/RedHatText-SemiBold.ttf"),
    face!(Ui, 700, false, "redhat/RedHatText-Bold.ttf"),
    face!(Code, 400, false, "redhat/RedHatMono-Regular.ttf"),
    face!(Code, 700, false, "redhat/RedHatMono-Bold.ttf"),
];
static RECURSIVE_FACES: [Face; 7] = [
    face!(Ui, 400, false, "recursive/RecursiveSansLnrSt-Regular.ttf"),
    face!(Ui, 500, false, "recursive/RecursiveSansLnrSt-Med.ttf"),
    face!(Ui, 600, false, "recursive/RecursiveSansLnrSt-SemiBold.ttf"),
    face!(Ui, 700, false, "recursive/RecursiveSansLnrSt-Bold.ttf"),
    face!(Ui, 400, true, "recursive/RecursiveSansLnrSt-Italic.ttf"),
    face!(Code, 400, false, "recursive/RecursiveMonoLnrSt-Regular.ttf"),
    face!(Code, 700, false, "recursive/RecursiveMonoLnrSt-Bold.ttf"),
];
static FIGTREE_FACES: [Face; 7] = [
    face!(Ui, 400, false, "figtree/Figtree-Regular.ttf"),
    face!(Ui, 500, false, "figtree/Figtree-Medium.ttf"),
    face!(Ui, 600, false, "figtree/Figtree-SemiBold.ttf"),
    face!(Ui, 700, false, "figtree/Figtree-Bold.ttf"),
    face!(Ui, 400, true, "figtree/Figtree-Italic.ttf"),
    face!(Code, 400, false, "figtree/FiraCode-Regular.ttf"),
    face!(Code, 700, false, "figtree/FiraCode-Bold.ttf"),
];
static GEIST_FACES: [Face; 5] = [
    face!(Ui, 400, false, "Geist.ttf"),
    face!(Ui, 500, false, "Geist-Medium.ttf"),
    face!(Ui, 600, false, "Geist-SemiBold.ttf"),
    face!(Ui, 700, false, "Geist-Bold.ttf"),
    face!(Code, 400, false, "GeistMono.ttf"),
];

/// The bundled font pairs, in the order Settings lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontPair {
    Atkinson,
    Inter,
    Plex,
    RedHat,
    Recursive,
    Figtree,
    Geist,
}

impl FontPair {
    /// Every pair, in the order Settings lists them.
    pub const ALL: [FontPair; 7] = [
        FontPair::Atkinson,
        FontPair::Inter,
        FontPair::Plex,
        FontPair::RedHat,
        FontPair::Recursive,
        FontPair::Figtree,
        FontPair::Geist,
    ];

    /// The pair a person who never chose gets.
    pub const DEFAULT: FontPair = FontPair::Atkinson;

    /// The persisted id (`appearance.fontPair`).
    pub fn id(self) -> &'static str {
        match self {
            FontPair::Atkinson => "atkinson",
            FontPair::Inter => "inter",
            FontPair::Plex => "plex",
            FontPair::RedHat => "redhat",
            FontPair::Recursive => "recursive",
            FontPair::Figtree => "figtree",
            FontPair::Geist => "geist",
        }
    }

    /// The pair with this id, trimmed; an empty or unknown id is
    /// [`FontPair::DEFAULT`], so a garbage value never blocks settings loading.
    pub fn from_id(id: &str) -> FontPair {
        let id = id.trim();
        Self::ALL
            .into_iter()
            .find(|pair| pair.id() == id)
            .unwrap_or(Self::DEFAULT)
    }

    /// The name the Settings list shows.
    pub fn title(self) -> &'static str {
        match self {
            FontPair::Atkinson => "Atkinson Hyperlegible",
            FontPair::Inter => "Inter + JetBrains Mono",
            FontPair::Plex => "IBM Plex",
            FontPair::RedHat => "Red Hat",
            FontPair::Recursive => "Recursive",
            FontPair::Figtree => "Figtree + Fira Code",
            FontPair::Geist => "Geist",
        }
    }

    /// The interface family: the typographic family name in its files.
    pub fn ui_family(self) -> &'static str {
        match self {
            FontPair::Atkinson => "Atkinson Hyperlegible Next",
            FontPair::Inter => "Inter",
            FontPair::Plex => "IBM Plex Sans",
            FontPair::RedHat => "Red Hat Text",
            FontPair::Recursive => "Recursive Sans Linear Static",
            FontPair::Figtree => "Figtree",
            FontPair::Geist => "Geist",
        }
    }

    /// The code family: the typographic family name in its files.
    pub fn code_family(self) -> &'static str {
        match self {
            FontPair::Atkinson => "Atkinson Hyperlegible Mono",
            FontPair::Inter => "JetBrains Mono",
            FontPair::Plex => "IBM Plex Mono",
            FontPair::RedHat => "Red Hat Mono",
            FontPair::Recursive => "Recursive Mono Linear Static",
            FontPair::Figtree => "Fira Code",
            FontPair::Geist => "Geist Mono",
        }
    }

    /// Whether the pair bundles an interface italic that bezel markdown can
    /// use for emphasis. Geist and Red Hat do not (Red Hat's italic file is
    /// italic only by its slant angle, so it is left out).
    pub fn has_ui_italic(self) -> bool {
        self.faces().iter().any(|face| face.role == FaceRole::Ui && face.italic)
    }

    /// The bundled faces, interface first, in registration order.
    pub fn faces(self) -> &'static [Face] {
        match self {
            FontPair::Atkinson => &ATKINSON_FACES,
            FontPair::Inter => &INTER_FACES,
            FontPair::Plex => &PLEX_FACES,
            FontPair::RedHat => &REDHAT_FACES,
            FontPair::Recursive => &RECURSIVE_FACES,
            FontPair::Figtree => &FIGTREE_FACES,
            FontPair::Geist => &GEIST_FACES,
        }
    }

    /// The pair whose interface family is `family`, if any.
    pub fn for_ui_family(family: &str) -> Option<FontPair> {
        Self::ALL.into_iter().find(|pair| pair.ui_family() == family)
    }
}
