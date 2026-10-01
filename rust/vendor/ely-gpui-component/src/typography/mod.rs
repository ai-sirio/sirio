mod copy;
mod effects;
mod fit;
pub mod format;
mod formatted;
mod inline;
pub mod keys;
mod link;
mod select;
mod text;

pub use copy::CopyableText;
pub use effects::{GradientText, ShimmerText, Typewriter};
pub use fit::{Ellipsis, EllipsisTooltip, MiddleEllipsis};
pub(crate) use fit::{LEADING, text_width};
pub use format::DurationStyle;
pub(crate) use formatted::fresh;
pub use formatted::{
    CurrencyText, DateTimeText, DurationText, FileSizeText, NumberText, PercentText, PluralText,
    RelativeTime,
};
pub use inline::{Blockquote, Code, Highlight, Kbd, KbdCombo};
pub use link::{ExternalLink, Link};
pub use select::SelectableText;
pub use text::{Caption, Heading, Label, Overline, Paragraph, Subtitle, Title, literal, tabular};

mod rolling;
pub use rolling::AnimatedNumber;
