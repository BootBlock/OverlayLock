//! Parses the colours given on the command line: `#RGB`, `#RRGGBB` (the `#` optional) or a CSS
//! Color 4 named colour.

use std::fmt;
use std::str::FromStr;

use windows::Win32::Foundation::COLORREF;

/// An opaque sRGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colour {
    /// The red channel.
    pub r: u8,
    /// The green channel.
    pub g: u8,
    /// The blue channel.
    pub b: u8,
}

impl Colour {
    /// Initialises a new instance of the [`Colour`] type from `0xRRGGBB`.
    pub const fn hex(rgb: u32) -> Self {
        let [_, r, g, b] = rgb.to_be_bytes();
        Self { r, g, b }
    }

    /// Gets the GDI `COLORREF` (`0x00BBGGRR`) for this colour.
    pub fn to_colorref(self) -> COLORREF {
        COLORREF(u32::from(self.r) | u32::from(self.g) << 8 | u32::from(self.b) << 16)
    }

    /// Gets each channel as a fraction from 0 to 1, in red, green, blue order.
    pub fn to_unit_rgb(self) -> [f32; 3] {
        [self.r, self.g, self.b].map(|channel| f32::from(channel) / 255.0)
    }
}

/// The reason a colour did not parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColourError(String);

impl fmt::Display for ColourError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "'{}' is not a colour: use #RGB, #RRGGBB or a CSS colour name such as navy",
            self.0
        )
    }
}

impl std::error::Error for ColourError {}

impl FromStr for Colour {
    type Err = ColourError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let error = || ColourError(text.to_owned());
        let digits = text.strip_prefix('#').unwrap_or(text);
        if digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            let value = u32::from_str_radix(digits, 16).map_err(|_| error())?;
            return match digits.len() {
                6 => Ok(Self::hex(value)),
                3 => {
                    // Each digit doubles: #ABC is #AABBCC.
                    let expand = |shift: u32| ((value >> shift) & 0xF) * 0x11;
                    Ok(Self::hex(expand(8) << 16 | expand(4) << 8 | expand(0)))
                }
                _ => Err(error()),
            };
        }
        if text.starts_with('#') {
            return Err(error());
        }
        let name = text.to_ascii_lowercase();
        NAMED
            .binary_search_by(|(candidate, _)| (*candidate).cmp(name.as_str()))
            .map(|index| NAMED[index].1)
            .map_err(|_| error())
    }
}

/// The 148 named colours of CSS Color 4 (W3C Candidate Recommendation, section 6.1, "Named
/// Colors"), sorted by name for binary search. Copied from the specification's table.
static NAMED: [(&str, Colour); 148] = [
    ("aliceblue", Colour::hex(0xF0_F8FF)),
    ("antiquewhite", Colour::hex(0xFA_EBD7)),
    ("aqua", Colour::hex(0x00_FFFF)),
    ("aquamarine", Colour::hex(0x7F_FFD4)),
    ("azure", Colour::hex(0xF0_FFFF)),
    ("beige", Colour::hex(0xF5_F5DC)),
    ("bisque", Colour::hex(0xFF_E4C4)),
    ("black", Colour::hex(0x00_0000)),
    ("blanchedalmond", Colour::hex(0xFF_EBCD)),
    ("blue", Colour::hex(0x00_00FF)),
    ("blueviolet", Colour::hex(0x8A_2BE2)),
    ("brown", Colour::hex(0xA5_2A2A)),
    ("burlywood", Colour::hex(0xDE_B887)),
    ("cadetblue", Colour::hex(0x5F_9EA0)),
    ("chartreuse", Colour::hex(0x7F_FF00)),
    ("chocolate", Colour::hex(0xD2_691E)),
    ("coral", Colour::hex(0xFF_7F50)),
    ("cornflowerblue", Colour::hex(0x64_95ED)),
    ("cornsilk", Colour::hex(0xFF_F8DC)),
    ("crimson", Colour::hex(0xDC_143C)),
    ("cyan", Colour::hex(0x00_FFFF)),
    ("darkblue", Colour::hex(0x00_008B)),
    ("darkcyan", Colour::hex(0x00_8B8B)),
    ("darkgoldenrod", Colour::hex(0xB8_860B)),
    ("darkgray", Colour::hex(0xA9_A9A9)),
    ("darkgreen", Colour::hex(0x00_6400)),
    ("darkgrey", Colour::hex(0xA9_A9A9)),
    ("darkkhaki", Colour::hex(0xBD_B76B)),
    ("darkmagenta", Colour::hex(0x8B_008B)),
    ("darkolivegreen", Colour::hex(0x55_6B2F)),
    ("darkorange", Colour::hex(0xFF_8C00)),
    ("darkorchid", Colour::hex(0x99_32CC)),
    ("darkred", Colour::hex(0x8B_0000)),
    ("darksalmon", Colour::hex(0xE9_967A)),
    ("darkseagreen", Colour::hex(0x8F_BC8F)),
    ("darkslateblue", Colour::hex(0x48_3D8B)),
    ("darkslategray", Colour::hex(0x2F_4F4F)),
    ("darkslategrey", Colour::hex(0x2F_4F4F)),
    ("darkturquoise", Colour::hex(0x00_CED1)),
    ("darkviolet", Colour::hex(0x94_00D3)),
    ("deeppink", Colour::hex(0xFF_1493)),
    ("deepskyblue", Colour::hex(0x00_BFFF)),
    ("dimgray", Colour::hex(0x69_6969)),
    ("dimgrey", Colour::hex(0x69_6969)),
    ("dodgerblue", Colour::hex(0x1E_90FF)),
    ("firebrick", Colour::hex(0xB2_2222)),
    ("floralwhite", Colour::hex(0xFF_FAF0)),
    ("forestgreen", Colour::hex(0x22_8B22)),
    ("fuchsia", Colour::hex(0xFF_00FF)),
    ("gainsboro", Colour::hex(0xDC_DCDC)),
    ("ghostwhite", Colour::hex(0xF8_F8FF)),
    ("gold", Colour::hex(0xFF_D700)),
    ("goldenrod", Colour::hex(0xDA_A520)),
    ("gray", Colour::hex(0x80_8080)),
    ("green", Colour::hex(0x00_8000)),
    ("greenyellow", Colour::hex(0xAD_FF2F)),
    ("grey", Colour::hex(0x80_8080)),
    ("honeydew", Colour::hex(0xF0_FFF0)),
    ("hotpink", Colour::hex(0xFF_69B4)),
    ("indianred", Colour::hex(0xCD_5C5C)),
    ("indigo", Colour::hex(0x4B_0082)),
    ("ivory", Colour::hex(0xFF_FFF0)),
    ("khaki", Colour::hex(0xF0_E68C)),
    ("lavender", Colour::hex(0xE6_E6FA)),
    ("lavenderblush", Colour::hex(0xFF_F0F5)),
    ("lawngreen", Colour::hex(0x7C_FC00)),
    ("lemonchiffon", Colour::hex(0xFF_FACD)),
    ("lightblue", Colour::hex(0xAD_D8E6)),
    ("lightcoral", Colour::hex(0xF0_8080)),
    ("lightcyan", Colour::hex(0xE0_FFFF)),
    ("lightgoldenrodyellow", Colour::hex(0xFA_FAD2)),
    ("lightgray", Colour::hex(0xD3_D3D3)),
    ("lightgreen", Colour::hex(0x90_EE90)),
    ("lightgrey", Colour::hex(0xD3_D3D3)),
    ("lightpink", Colour::hex(0xFF_B6C1)),
    ("lightsalmon", Colour::hex(0xFF_A07A)),
    ("lightseagreen", Colour::hex(0x20_B2AA)),
    ("lightskyblue", Colour::hex(0x87_CEFA)),
    ("lightslategray", Colour::hex(0x77_8899)),
    ("lightslategrey", Colour::hex(0x77_8899)),
    ("lightsteelblue", Colour::hex(0xB0_C4DE)),
    ("lightyellow", Colour::hex(0xFF_FFE0)),
    ("lime", Colour::hex(0x00_FF00)),
    ("limegreen", Colour::hex(0x32_CD32)),
    ("linen", Colour::hex(0xFA_F0E6)),
    ("magenta", Colour::hex(0xFF_00FF)),
    ("maroon", Colour::hex(0x80_0000)),
    ("mediumaquamarine", Colour::hex(0x66_CDAA)),
    ("mediumblue", Colour::hex(0x00_00CD)),
    ("mediumorchid", Colour::hex(0xBA_55D3)),
    ("mediumpurple", Colour::hex(0x93_70DB)),
    ("mediumseagreen", Colour::hex(0x3C_B371)),
    ("mediumslateblue", Colour::hex(0x7B_68EE)),
    ("mediumspringgreen", Colour::hex(0x00_FA9A)),
    ("mediumturquoise", Colour::hex(0x48_D1CC)),
    ("mediumvioletred", Colour::hex(0xC7_1585)),
    ("midnightblue", Colour::hex(0x19_1970)),
    ("mintcream", Colour::hex(0xF5_FFFA)),
    ("mistyrose", Colour::hex(0xFF_E4E1)),
    ("moccasin", Colour::hex(0xFF_E4B5)),
    ("navajowhite", Colour::hex(0xFF_DEAD)),
    ("navy", Colour::hex(0x00_0080)),
    ("oldlace", Colour::hex(0xFD_F5E6)),
    ("olive", Colour::hex(0x80_8000)),
    ("olivedrab", Colour::hex(0x6B_8E23)),
    ("orange", Colour::hex(0xFF_A500)),
    ("orangered", Colour::hex(0xFF_4500)),
    ("orchid", Colour::hex(0xDA_70D6)),
    ("palegoldenrod", Colour::hex(0xEE_E8AA)),
    ("palegreen", Colour::hex(0x98_FB98)),
    ("paleturquoise", Colour::hex(0xAF_EEEE)),
    ("palevioletred", Colour::hex(0xDB_7093)),
    ("papayawhip", Colour::hex(0xFF_EFD5)),
    ("peachpuff", Colour::hex(0xFF_DAB9)),
    ("peru", Colour::hex(0xCD_853F)),
    ("pink", Colour::hex(0xFF_C0CB)),
    ("plum", Colour::hex(0xDD_A0DD)),
    ("powderblue", Colour::hex(0xB0_E0E6)),
    ("purple", Colour::hex(0x80_0080)),
    ("rebeccapurple", Colour::hex(0x66_3399)),
    ("red", Colour::hex(0xFF_0000)),
    ("rosybrown", Colour::hex(0xBC_8F8F)),
    ("royalblue", Colour::hex(0x41_69E1)),
    ("saddlebrown", Colour::hex(0x8B_4513)),
    ("salmon", Colour::hex(0xFA_8072)),
    ("sandybrown", Colour::hex(0xF4_A460)),
    ("seagreen", Colour::hex(0x2E_8B57)),
    ("seashell", Colour::hex(0xFF_F5EE)),
    ("sienna", Colour::hex(0xA0_522D)),
    ("silver", Colour::hex(0xC0_C0C0)),
    ("skyblue", Colour::hex(0x87_CEEB)),
    ("slateblue", Colour::hex(0x6A_5ACD)),
    ("slategray", Colour::hex(0x70_8090)),
    ("slategrey", Colour::hex(0x70_8090)),
    ("snow", Colour::hex(0xFF_FAFA)),
    ("springgreen", Colour::hex(0x00_FF7F)),
    ("steelblue", Colour::hex(0x46_82B4)),
    ("tan", Colour::hex(0xD2_B48C)),
    ("teal", Colour::hex(0x00_8080)),
    ("thistle", Colour::hex(0xD8_BFD8)),
    ("tomato", Colour::hex(0xFF_6347)),
    ("turquoise", Colour::hex(0x40_E0D0)),
    ("violet", Colour::hex(0xEE_82EE)),
    ("wheat", Colour::hex(0xF5_DEB3)),
    ("white", Colour::hex(0xFF_FFFF)),
    ("whitesmoke", Colour::hex(0xF5_F5F5)),
    ("yellow", Colour::hex(0xFF_FF00)),
    ("yellowgreen", Colour::hex(0x9A_CD32)),
];

#[cfg(test)]
mod tests {
    use super::{Colour, NAMED};

    fn parse(text: &str) -> Option<Colour> {
        text.parse().ok()
    }

    #[test]
    fn hex_in_four_forms() {
        let expected = Colour::hex(0x1A_2B3C);
        assert_eq!(parse("#1A2B3C"), Some(expected));
        assert_eq!(parse("1a2b3c"), Some(expected));
        assert_eq!(parse("#abc"), Some(Colour::hex(0xAA_BBCC)));
        assert_eq!(parse("ABC"), Some(Colour::hex(0xAA_BBCC)));
    }

    #[test]
    fn invalid_colours_are_rejected() {
        for text in [
            "",
            "#",
            "#12",
            "#1234",
            "#12345g",
            "1234567",
            "#navy",
            "nosuchcolour",
            "red ",
        ] {
            assert_eq!(parse(text), None, "{text:?} should not parse");
        }
    }

    /// Ten names checked against the values in the specification's table.
    #[test]
    fn named_colours_match_the_specification() {
        let samples = [
            ("aliceblue", 0xF0_F8FF),
            ("black", 0x00_0000),
            ("cornflowerblue", 0x64_95ED),
            ("darkgoldenrod", 0xB8_860B),
            ("gold", 0xFF_D700),
            ("lightgoldenrodyellow", 0xFA_FAD2),
            ("mediumspringgreen", 0x00_FA9A),
            ("rebeccapurple", 0x66_3399),
            ("slategrey", 0x70_8090),
            ("yellowgreen", 0x9A_CD32),
        ];
        for (name, rgb) in samples {
            assert_eq!(parse(name), Some(Colour::hex(rgb)), "{name}");
        }
    }

    #[test]
    fn names_are_case_insensitive() {
        assert_eq!(parse("RebeccaPurple"), Some(Colour::hex(0x66_3399)));
        assert_eq!(parse("NAVY"), Some(Colour::hex(0x00_0080)));
    }

    #[test]
    fn table_is_sorted_unique_and_lower_case() {
        // The array type fixes the count at the specification's 148 names at compile time.
        assert!(NAMED.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert!(
            NAMED
                .iter()
                .all(|(name, _)| name.bytes().all(|b| b.is_ascii_lowercase()))
        );
    }

    #[test]
    fn colorref_is_bgr() {
        assert_eq!(Colour::hex(0x11_2233).to_colorref().0, 0x0033_2211);
    }
}
