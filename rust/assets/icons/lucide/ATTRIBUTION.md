# Lucide icons attribution

The SVG files in this directory were copied byte-for-byte from
[`lucide-icons/lucide`](https://github.com/lucide-icons/lucide) release tag
`1.66.0` (commit `a630cec77eac6568b989e87e4947a8bfe45caacd`), on 2026-11-01:

- `panel-left-close.svg` —
  https://github.com/lucide-icons/lucide/blob/1.66.0/icons/panel-left-close.svg
- `panel-left-open.svg` —
  https://github.com/lucide-icons/lucide/blob/1.66.0/icons/panel-left-open.svg

`LICENSE` is the repository's `LICENSE` file at the same tag, copied
unchanged. Neither icon is in its list of icons derived from Feather, so the
ISC part of it is the one that applies.

They are the Primary half's collapse and expand buttons (`Icon::PanelLeftClose`
and `Icon::PanelLeftOpen`). The Zed catalog pinned in `../zed/` has no panel
icon with a direction, only the `threads_sidebar_*` state pair, so this pair
comes from Lucide, the set Zed's own icons are licensed under. Keep the
geometry unchanged; update the tag, the commit and this list together when
refreshing or adding a file.

## `star-filled.svg`

Not a byte-for-byte copy: the `star` path of Lucide (the one vendored in
`rust/vendor/ely-gpui-component/assets/icons/star.svg`, lucide-static 1.48.0)
drawn with `fill="currentColor"` and no stroke. It is the Primary's filled star
(`Icon::StarFilled`); a text glyph drew it as a dot.
