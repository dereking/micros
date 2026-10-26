//! The Micro OS icon resource registry.
//!
//! Apps name an icon (`ui.icon("settings")`, or `app({ icon: "settings" })`)
//! and every host renders that one name with its native toolkit:
//!
//! | Host | Rendering |
//! | --- | --- |
//! | ESP32 / macOS (LVGL) | the Font Awesome glyph, painted by `micro_icons_*` |
//! | Web Player | an inline 24 × 24 stroke SVG built from `path` |
//!
//! Keeping the glyph and the SVG path next to each other means a name can
//! never exist on one host and silently disappear on another. Device builds
//! disable the `svg-icons` feature, which compiles `path` out of the table
//! (the Web-only geometry never reaches firmware flash). Names are
//! lowercase ASCII identifiers; an App that does not find a fitting name can
//! still use a single display glyph (for example `icon: "C"`), and a binding
//! that returns an unknown name degrades to `?` with a renderer diagnostic.
//!
//! The Web paths use a 24 × 24 viewBox, no fill, a 2 px `currentColor` stroke
//! and round caps/joins — the same geometry as the OS app bar's Wi-Fi glyph.

/// One icon available to Apps through `ui.icon` / `app({ icon })`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconResource {
    /// Stable lowercase name used in App source.
    pub name: &'static str,
    /// Font Awesome private-use glyph, carried by `micro_icons_*`.
    pub glyph: &'static str,
    /// SVG path data on a 24 × 24 viewBox, rendered as a stroke. Web builds
    /// enable `svg-icons`; device builds compile the table without paths so
    /// the Web-only geometry never reaches their flash.
    #[cfg(feature = "svg-icons")]
    pub path: &'static str,
}

/// Build one registry entry; the SVG path is compiled only for Web hosts.
macro_rules! icon {
    ($name:literal, $glyph:literal, $path:literal) => {
        IconResource {
            name: $name,
            glyph: $glyph,
            #[cfg(feature = "svg-icons")]
            path: $path,
        }
    };
}

/// Every icon the OS ships. Keep this table sorted by `name`.
pub const ICONS: &[IconResource] = &[
    icon!(
        "apps",
        "\u{f0c9}",
        "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z"
    ),
    icon!("battery", "\u{f240}", "M2 8h16v8H2zM20 10.5v3M5 12h7"),
    icon!(
        "bell",
        "\u{f0f3}",
        "M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9M10.3 21a2 2 0 0 0 3.4 0"
    ),
    icon!("bluetooth", "\u{f293}", "M7 7l10 10-5 4V3l5 4L7 17"),
    icon!("check", "\u{f00c}", "M4 12l5 5L20 6"),
    icon!("close", "\u{f00d}", "M6 6l12 12M18 6L6 18"),
    icon!("copy", "\u{f0c5}", "M9 9h11v11H9zM15 9V4H4v11h5"),
    icon!("down", "\u{f078}", "M5 9l7 7 7-7"),
    icon!("download", "\u{f019}", "M12 4v11M7 10l5 5 5-5M4 20h16"),
    icon!(
        "drive",
        "\u{f01c}",
        "M3 14h18v5H3zM5 14l2-8h10l2 8M7 17h.01M11 17h.01"
    ),
    icon!("edit", "\u{f304}", "M4 20h4L19 9l-4-4L4 16zM14 6l4 4"),
    icon!(
        "eye",
        "\u{f06e}",
        "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z"
    ),
    icon!("file", "\u{f15b}", "M6 3h8l4 4v14H6zM14 3v4h4"),
    icon!("folder", "\u{f07b}", "M3 6h6l2 2h10v10H3z"),
    icon!(
        "home",
        "\u{f015}",
        "M4 11.5 12 4l8 7.5M6.5 10.5V20h11v-9.5M10 20v-5h4v5"
    ),
    icon!(
        "image",
        "\u{f03e}",
        "M3 5h18v14H3zM3 15l5-5 4 4 3-3 6 6M8.5 8a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3z"
    ),
    icon!(
        "keyboard",
        "\u{f11c}",
        "M2 6h20v12H2zM6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10"
    ),
    icon!("left", "\u{f053}", "M15 5l-7 7 7 7"),
    icon!(
        "list",
        "\u{f00b}",
        "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01"
    ),
    icon!(
        "location",
        "\u{f124}",
        "M12 21s-7-6.2-7-11a7 7 0 0 1 14 0c0 4.8-7 11-7 11zM12 12a2 2 0 1 0 0-4 2 2 0 0 0 0 4z"
    ),
    icon!("mail", "\u{f0e0}", "M3 6h18v12H3zM3 7l9 6 9-6"),
    icon!("minus", "\u{f068}", "M4 12h16"),
    icon!(
        "music",
        "\u{f001}",
        "M9 18V5l10-2v13M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM19 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0z"
    ),
    icon!(
        "mute",
        "\u{f026}",
        "M4 9h4l5-4v14l-5-4H4zM16.5 9.5l5 5M21.5 9.5l-5 5"
    ),
    icon!("next", "\u{f051}", "M6 5l8 7-8 7M18 5v14"),
    icon!("pause", "\u{f04c}", "M9 5v14M15 5v14"),
    icon!("play", "\u{f04b}", "M8 5l11 7-11 7z"),
    icon!("plus", "\u{f067}", "M12 4v16M4 12h16"),
    icon!("power", "\u{f011}", "M12 3v9M6.4 6.4a8 8 0 1 0 11.2 0"),
    icon!("prev", "\u{f048}", "M18 5l-8 7 8 7M6 5v14"),
    icon!("refresh", "\u{f021}", "M20 12a8 8 0 1 1-2.3-5.6M20 4v5h-5"),
    icon!("right", "\u{f054}", "M9 5l7 7-7 7"),
    icon!(
        "save",
        "\u{f0c7}",
        "M4 4h13l3 3v13H4zM8 4v6h8V4M8 20v-6h8v6"
    ),
    icon!(
        "settings",
        "\u{f013}",
        "M12 8.5a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7zM12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M18.7 5.3l-1.4 1.4M6.7 17.3l-1.4 1.4"
    ),
    icon!("stop", "\u{f04d}", "M6 6h12v12H6z"),
    icon!(
        "trash",
        "\u{f2ed}",
        "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v6M14 11v6"
    ),
    icon!("up", "\u{f077}", "M5 15l7-7 7 7"),
    icon!("upload", "\u{f093}", "M12 20V9M7 14l5-5 5 5M4 4h16"),
    icon!("video", "\u{f008}", "M3 6h13v12H3zM16 10l5-3v10l-5-3"),
    icon!(
        "volume",
        "\u{f028}",
        "M4 9h4l5-4v14l-5-4H4zM16.5 8.5a5 5 0 0 1 0 7M19 6a8.5 8.5 0 0 1 0 12"
    ),
    icon!("warning", "\u{f071}", "M12 3l10 18H2zM12 9v5M12 17h.01"),
    icon!(
        "wifi",
        "\u{f1eb}",
        "M2.5 8.5a14 14 0 0 1 19 0M6 12a9 9 0 0 1 12 0M9.5 15.5a4.5 4.5 0 0 1 5 0M12 19.5h.01"
    ),
];

/// Look an icon up by its stable name.
pub fn icon(name: &str) -> Option<&'static IconResource> {
    ICONS.iter().find(|icon| icon.name == name)
}

/// Whether `name` is one of the OS icon resources.
pub fn is_icon_name(name: &str) -> bool {
    icon(name).is_some()
}

/// The glyph an LVGL host should paint for `name`.
///
/// A single-character name is passed through so legacy App manifests
/// (`icon: "C"`) keep rendering; unknown names return `None` and the renderer
/// substitutes `?` and reports a diagnostic.
pub fn glyph_for(name: &str) -> Option<&str> {
    if let Some(icon) = icon(name) {
        return Some(icon.glyph);
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(glyph), None) => name.get(..glyph.len_utf8()),
        _ => None,
    }
}

/// The SVG markup a Web host should paint for `name`, or `None` when `name`
/// is not a registered icon (a single glyph or an unknown name is rendered as
/// text instead). The 24 px default matches [`crate::TextStyle::DEFAULT_ICON`].
#[cfg(feature = "svg-icons")]
pub fn svg_for(name: &str) -> Option<String> {
    svg_for_size(name, 24)
}

/// Like [`svg_for`], but sets the intrinsic width/height so the icon matches
/// the glyph size the LVGL hosts paint (`textStyle.size`).
#[cfg(feature = "svg-icons")]
pub fn svg_for_size(name: &str, size_px: u8) -> Option<String> {
    let icon = icon(name)?;
    let size = size_px.max(12);
    Some(format!(
        "<svg viewBox=\"0 0 24 24\" width=\"{size}\" height=\"{size}\" \
         fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" \
         stroke-linecap=\"round\" stroke-linejoin=\"round\" \
         aria-hidden=\"true\" focusable=\"false\"><path d=\"{}\"/></svg>",
        icon.path
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_names_are_unique_sorted_and_well_formed() {
        let mut previous = "";
        for icon in ICONS {
            assert!(icon.name > previous, "{} is out of order", icon.name);
            previous = icon.name;
            assert!(
                icon.name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-'),
                "{} must be a lowercase identifier",
                icon.name
            );
            assert_eq!(icon.glyph.chars().count(), 1, "{} glyph", icon.name);
            #[cfg(feature = "svg-icons")]
            {
                assert!(!icon.path.is_empty(), "{} path", icon.name);
                assert!(!icon.path.contains('"'), "{} path quoting", icon.name);
            }
        }
        assert_eq!(ICONS.len(), 42);
    }

    #[test]
    fn lookup_maps_names_glyphs_and_svg() {
        assert!(is_icon_name("settings"));
        assert!(!is_icon_name("Settings"));
        assert_eq!(glyph_for("settings"), Some("\u{f013}"));
        assert_eq!(glyph_for("C"), Some("C"));
        assert_eq!(glyph_for("Counter"), None);
        #[cfg(feature = "svg-icons")]
        {
            let svg = svg_for("home").unwrap();
            assert!(svg.starts_with("<svg viewBox=\"0 0 24 24\""));
            assert!(svg.contains("M4 11.5 12 4l8 7.5"));
            assert!(svg_for("C").is_none());
        }
    }
}
