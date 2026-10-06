use cracker_desktop::CategoryColor;
use gpui_kit::{Hsla, IntoElement, Rgba, div, prelude::*, px, rgb};

struct PaletteEntry {
    name: &'static str,
    light: u32,
    dark: u32,
}

// Stable slots are persisted in SQLite; do not reorder these entries.
const PALETTE: [PaletteEntry; 16] = [
    PaletteEntry {
        name: "Red",
        light: 0xb91c1c,
        dark: 0xf87171,
    },
    PaletteEntry {
        name: "Orange",
        light: 0xc2410c,
        dark: 0xfb923c,
    },
    PaletteEntry {
        name: "Gold",
        light: 0x927000,
        dark: 0xfacc15,
    },
    PaletteEntry {
        name: "Olive",
        light: 0x697000,
        dark: 0xb4c04a,
    },
    PaletteEntry {
        name: "Green",
        light: 0x15803d,
        dark: 0x4ade80,
    },
    PaletteEntry {
        name: "Mint",
        light: 0x287c59,
        dark: 0x9fe3bb,
    },
    PaletteEntry {
        name: "Teal",
        light: 0x0f766e,
        dark: 0x2dd4bf,
    },
    PaletteEntry {
        name: "Cyan",
        light: 0x0e7490,
        dark: 0x22d3ee,
    },
    PaletteEntry {
        name: "Blue",
        light: 0x1d4ed8,
        dark: 0x60a5fa,
    },
    PaletteEntry {
        name: "Indigo",
        light: 0x4338ca,
        dark: 0xa5b4fc,
    },
    PaletteEntry {
        name: "Violet",
        light: 0x7e22ce,
        dark: 0xc084fc,
    },
    PaletteEntry {
        name: "Magenta",
        light: 0xa21caf,
        dark: 0xe879f9,
    },
    PaletteEntry {
        name: "Rose",
        light: 0xbe185d,
        dark: 0xfb7185,
    },
    PaletteEntry {
        name: "Brown",
        light: 0x854d0e,
        dark: 0xd2a679,
    },
    PaletteEntry {
        name: "Slate",
        light: 0x475569,
        dark: 0x94a3b8,
    },
    PaletteEntry {
        name: "Gray",
        light: 0x525252,
        dark: 0xd4d4d4,
    },
];

pub(super) fn name(color: CategoryColor) -> &'static str {
    PALETTE[color.index()].name
}

fn luminance(color: Rgba) -> f32 {
    fn linear(channel: f32) -> f32 {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

fn contrast(a: Rgba, b: Rgba) -> f32 {
    let a = luminance(a);
    let b = luminance(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn fill(color: CategoryColor, dark: bool, background: Rgba) -> Rgba {
    let entry = &PALETTE[color.index()];
    let base = rgb(if dark { entry.dark } else { entry.light });
    let target = if luminance(background) < 0.179 {
        1.0
    } else {
        0.0
    };
    // Keep the hue where possible, but preserve non-text contrast on custom themes too.
    for step in 0..=100 {
        let mix = step as f32 / 100.0;
        let adjusted = Rgba {
            r: base.r + (target - base.r) * mix,
            g: base.g + (target - base.g) * mix,
            b: base.b + (target - base.b) * mix,
            a: 1.0,
        };
        if contrast(adjusted, background) >= 3.0 {
            return adjusted;
        }
    }
    rgb(if target == 1.0 { 0xffffff } else { 0x000000 })
}

pub(super) fn swatch(
    color: CategoryColor,
    dark: bool,
    background: Hsla,
    border: Hsla,
) -> impl IntoElement {
    div()
        .size(px(16.0))
        .flex_shrink_0()
        .rounded_full()
        .bg(fill(color, dark, background.into()))
        .border_1()
        .border_color(border)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_has_sixteen_unique_named_slots_with_theme_contrast() {
        assert_eq!(CategoryColor::ALL.len(), PALETTE.len());
        for (index, entry) in PALETTE.iter().enumerate() {
            for other in &PALETTE[..index] {
                assert_ne!(entry.name, other.name);
                assert_ne!(entry.light, other.light);
                assert_ne!(entry.dark, other.dark);
            }
            let color = CategoryColor::ALL[index];
            for (dark, background) in [
                (false, 0xffffff),
                (false, 0xfafafa),
                (true, 0x18181b),
                (true, 0x27272a),
                (false, 0x777777),
            ] {
                let background = rgb(background);
                assert!(
                    contrast(fill(color, dark, background), background) >= 3.0,
                    "{}",
                    entry.name
                );
            }
        }
    }
}
