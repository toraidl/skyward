use crate::color::{over, Rgba};
use crate::glow::{self, Glow};

/// The design language a theme is drawn in. The pages themselves say what
/// they hold; this says how it is set: where the title sits, how round the
/// cards are, and which inks the stock system would use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// Xiaomi's HyperOS, which the pages were first measured against.
    #[default]
    HyperOs,
    /// OPPO's ColorOS 17, measured on a Find X3 (PEDM00).
    ColorOs,
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Rgba,
    pub card: Rgba,
    pub text: Rgba,
    pub subtitle: Rgba,
    pub value: Rgba,
    pub section: Rgba,
    pub chevron: Rgba,
    pub accent: Rgba,
    pub track: Rgba,
    pub switch_off: Rgba,
    pub search: Rgba,
    pub search_icon: Rgba,
    pub placeholder: Rgba,
    pub badge: Rgba,
    pub avatar: Rgba,
    pub avatar_glyph: Rgba,
    pub status: Rgba,
    pub battery: BatteryInk,
    pub mono_icon: Rgba,
    pub danger: Rgba,
    pub danger_soft: Rgba,
    /// Titles and values on the parameters page and captions inside cards, a
    /// step softer than `text`.
    pub info: Rgba,
    /// Line icons on the parameters page.
    pub outline: Rgba,
    /// Captions under the About page's spec values.
    pub spec_label: Rgba,
    /// Laid over the card behind a row a finger holds down, under the
    /// row's words and icon.
    pub row_press: Rgba,
    /// Laid over the `<`, the search page's round button and the home
    /// page's search bar while a finger holds them down.
    pub button_press: Rgba,
    /// The round button at the end of a WLAN network's row, and the arrow
    /// in it.
    pub round: Rgba,
    pub round_mark: Rgba,
    /// The arrows up and down after the value of a row that opens a list of
    /// choices.
    pub updown: Rgba,
    /// A list of choices open over the page, and the shadow it casts where
    /// the shadow is whole.
    pub popup: Rgba,
    pub shadow: Rgba,
    /// The hairline between two rows of one card. HyperOS keeps its rows
    /// apart by nothing at all; ColorOS draws this line, inset to the words.
    pub divider: Rgba,
}

/// The status bar battery's inks, partly see-through as the stock status
/// bar's are, so they sit on whatever page is under them.
#[derive(Clone, Copy)]
pub struct BatteryInk {
    /// What the level fills while the cable is in, charging or full.
    pub charging: Rgba,
    /// What a low level fills with the cable out.
    pub low: Rgba,
    /// What any other level fills; the digits are cut out of it.
    pub level: Rgba,
    /// The empty part, cap included.
    pub empty: Rgba,
    pub bolt: Rgba,
}

/// The About page in a theme: the moving field behind it, the version
/// line under the wordmark, and the cards that scroll over the field.
#[derive(Clone, Copy)]
pub struct Hero {
    pub glow: &'static Glow,
    /// The version line: black or white, partly see-through.
    pub version: Rgba,
    pub card: Rgba,
    pub title: Rgba,
    pub value: Rgba,
    pub chevron: Rgba,
}

/// Layout metrics measured on a Xiaomi 17 Ultra's stock settings, in dp,
/// converted at the panel's density.
#[derive(Clone, Copy)]
pub struct Metrics {
    pub w: u32,
    pub h: u32,
    pub px_per_dp: f32,
    pub margin: f32,
    pub card_radius: f32,
    pub card_gap: f32,
    pub section_block: f32,
    pub section_label_top: f32,
    pub row_h: f32,
    pub row_pad_v: f32,
    pub inner: f32,
    pub icon: f32,
    pub icon_gap: f32,
    pub title_px: f32,
    pub title_x: f32,
    pub title_top: f32,
    /// Back chevron: tip x, centre y, width, height and stroke.
    pub back_x: f32,
    pub back_cy: f32,
    pub back_w: f32,
    pub back_h: f32,
    pub back_stroke: f32,
    pub stroke: f32,
    pub content_top: f32,
    pub search_h: f32,
    pub search_text_inset: f32,
    pub after_search: f32,
    pub row_title_px: f32,
    pub sub_px: f32,
    pub sub_gap: f32,
    pub sub_pitch: f32,
    pub label_px: f32,
    pub account_title_px: f32,
    pub account_row_h: f32,
    pub avatar: f32,
    pub avatar_inset: f32,
    pub badge_px: f32,
    pub badge_w: f32,
    pub badge_h: f32,
    pub chevron_w: f32,
    pub chevron_h: f32,
    pub chevron_inset: f32,
    pub chevron_stroke: f32,
    pub value_gap: f32,
    pub switch_w: f32,
    pub switch_h: f32,
    pub knob: f32,
    pub slider_h: f32,
    pub slider_knob: f32,
    pub status_x: f32,
    /// Where the battery ends, from the right edge; with the cable in, the
    /// bolt's box ends at `status_bolt_right` instead and the battery moves
    /// left.
    pub status_right: f32,
    pub status_bolt_right: f32,
    pub status_cy: f32,
    pub status_px: f32,
}

impl Metrics {
    pub fn dp(&self, v: f32) -> f32 {
        v * self.px_per_dp
    }
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub pal: Palette,
    pub hero: Hero,
    pub m: Metrics,
    pub dark: bool,
    pub style: Style,
}

impl Theme {
    /// `px_per_dp` is the panel density over 160, e.g. 3.0 at 480 dpi.
    pub fn light(w: u32, h: u32, px_per_dp: f32) -> Self {
        Theme { pal: LIGHT, hero: LIGHT_HERO, m: metrics(w, h, px_per_dp), dark: false, style: Style::HyperOs }
    }

    pub fn dark(w: u32, h: u32, px_per_dp: f32) -> Self {
        Theme { pal: DARK, hero: DARK_HERO, m: metrics(w, h, px_per_dp), dark: true, style: Style::HyperOs }
    }

    /// The ColorOS 17 look, sampled from a Find X3's stock settings.
    pub fn coloros_light(w: u32, h: u32, px_per_dp: f32) -> Self {
        Theme {
            pal: COLOROS_LIGHT,
            hero: COLOROS_LIGHT_HERO,
            m: coloros_metrics(w, h, px_per_dp),
            dark: false,
            style: Style::ColorOs,
        }
    }

    pub fn coloros_dark(w: u32, h: u32, px_per_dp: f32) -> Self {
        Theme {
            pal: COLOROS_DARK,
            hero: COLOROS_DARK_HERO,
            m: coloros_metrics(w, h, px_per_dp),
            dark: true,
            style: Style::ColorOs,
        }
    }

    /// The theme of `style` in the given mode.
    pub fn styled(style: Style, dark: bool, w: u32, h: u32, px_per_dp: f32) -> Self {
        match (style, dark) {
            (Style::HyperOs, false) => Theme::light(w, h, px_per_dp),
            (Style::HyperOs, true) => Theme::dark(w, h, px_per_dp),
            (Style::ColorOs, false) => Theme::coloros_light(w, h, px_per_dp),
            (Style::ColorOs, true) => Theme::coloros_dark(w, h, px_per_dp),
        }
    }

    /// This theme with every card as it shows behind a row held down.
    pub fn pressed(&self) -> Theme {
        let mut t = *self;
        t.pal.card = over(self.pal.row_press, self.pal.card);
        t.hero.card = over(self.pal.row_press, self.hero.card);
        t
    }
}

fn metrics(w: u32, h: u32, px_per_dp: f32) -> Metrics {
    let dp = |v: f32| v * px_per_dp;
    Metrics {
        w,
        h,
        px_per_dp,
        margin: dp(12.0),
        card_radius: dp(20.0),
        card_gap: dp(12.0),
        section_block: dp(45.67),
        section_label_top: dp(23.0),
        row_h: dp(55.67),
        row_pad_v: dp(17.0),
        inner: dp(16.0),
        icon: dp(28.0),
        icon_gap: dp(20.67),
        title_px: dp(32.0),
        title_x: dp(28.6),
        title_top: dp(117.7),
        back_x: dp(26.3),
        back_cy: dp(75.7),
        back_w: dp(11.7),
        back_h: dp(21.0),
        back_stroke: dp(1.8),
        stroke: dp(2.0),
        content_top: dp(167.7),
        search_h: dp(43.7),
        search_text_inset: dp(43.7),
        after_search: dp(12.7),
        row_title_px: dp(16.9),
        sub_px: dp(14.0),
        sub_gap: dp(6.7),
        sub_pitch: dp(18.7),
        label_px: dp(13.2),
        account_title_px: dp(17.1),
        account_row_h: dp(69.7),
        avatar: dp(34.0),
        avatar_inset: dp(12.8),
        badge_px: dp(14.5),
        badge_w: dp(72.9),
        badge_h: dp(28.9),
        chevron_w: dp(7.7),
        chevron_h: dp(13.3),
        chevron_inset: dp(17.3),
        chevron_stroke: dp(1.15),
        value_gap: dp(11.3),
        switch_w: dp(48.7),
        switch_h: dp(28.0),
        knob: dp(19.3),
        slider_h: dp(27.5),
        slider_knob: dp(18.7),
        status_x: dp(37.0),
        status_right: dp(40.0),
        status_bolt_right: dp(36.0),
        status_cy: dp(25.7),
        status_px: dp(14.1),
    }
}

// Sampled from the stock Settings on a Xiaomi 17 Ultra.
const LIGHT: Palette = Palette {
    bg: Rgba::rgb(243, 243, 243),
    card: Rgba::rgb(255, 255, 255),
    text: Rgba::rgb(0, 0, 0),
    subtitle: Rgba::rgb(102, 102, 102),
    value: Rgba::rgb(153, 153, 153),
    section: Rgba::rgb(143, 156, 174),
    chevron: Rgba::rgb(178, 178, 178),
    accent: Rgba::rgb(74, 128, 247),
    track: Rgba::rgb(240, 240, 240),
    switch_off: Rgba::rgb(229, 229, 229),
    search: Rgba::rgb(229, 229, 229),
    search_icon: Rgba::rgb(161, 161, 161),
    // Black at 30%: 160 over the search field, and darker blue where it
    // crosses the cursor.
    placeholder: Rgba::rgb(0, 0, 0).with_alpha(77),
    badge: Rgba::rgb(235, 242, 254),
    avatar: Rgba::rgb(221, 223, 226),
    avatar_glyph: Rgba::rgb(255, 255, 255),
    status: Rgba::rgb(61, 61, 61),
    // The stock status bar's dark icons, sampled over light pages.
    battery: BatteryInk {
        charging: Rgba::rgb(29, 205, 58),
        low: Rgba::rgb(250, 56, 46),
        level: Rgba::rgb(0, 0, 0).with_alpha(196),
        empty: Rgba::rgb(0, 0, 0).with_alpha(77),
        bolt: Rgba::rgb(0, 0, 0).with_alpha(172),
    },
    mono_icon: Rgba::rgb(179, 179, 179),
    danger: Rgba::rgb(230, 66, 60),
    danger_soft: Rgba::rgb(253, 236, 235),
    info: Rgba::rgb(51, 51, 51),
    outline: Rgba::rgb(102, 102, 102),
    spec_label: Rgba::rgb(127, 127, 127),
    // A white card held down goes to 225; the page under `<` goes from 243
    // to 219, the search field and its round button from 229 to 206.
    row_press: Rgba::rgb(0, 0, 0).with_alpha(30),
    button_press: Rgba::rgb(0, 0, 0).with_alpha(26),
    // Sampled from the stock WLAN page.
    round: Rgba::rgb(242, 242, 242),
    round_mark: Rgba::rgb(169, 169, 169),
    // Sampled from the stock page that adds a network; the shadow as the
    // grey it leaves on the page beside the list is fitted.
    updown: Rgba::rgb(214, 214, 214),
    popup: Rgba::rgb(255, 255, 255),
    shadow: Rgba::rgb(0, 0, 0).with_alpha(27),
    divider: Rgba::rgb(229, 229, 229),
};

const DARK: Palette = Palette {
    bg: Rgba::rgb(0, 0, 0),
    card: Rgba::rgb(28, 28, 28),
    text: Rgba::rgb(237, 237, 237),
    subtitle: Rgba::rgb(140, 140, 140),
    value: Rgba::rgb(128, 128, 128),
    section: Rgba::rgb(107, 113, 134),
    chevron: Rgba::rgb(77, 77, 77),
    accent: Rgba::rgb(84, 136, 248),
    track: Rgba::rgb(46, 46, 46),
    switch_off: Rgba::rgb(58, 58, 58),
    search: Rgba::rgb(28, 28, 28),
    search_icon: Rgba::rgb(110, 110, 110),
    // White at 36%: 110 over the search field.
    placeholder: Rgba::rgb(255, 255, 255).with_alpha(92),
    badge: Rgba::rgb(29, 40, 64),
    avatar: Rgba::rgb(51, 51, 51),
    avatar_glyph: Rgba::rgb(140, 140, 140),
    status: Rgba::rgb(224, 224, 224),
    // The stock status bar's light icons, sampled over black and over the
    // lock screen's wallpaper.
    battery: BatteryInk {
        // As the stock Settings fill it over black and over a dark grey: the
        // green they fill with over light pages too.
        charging: Rgba::rgb(29, 205, 58),
        low: Rgba::rgb(230, 75, 59),
        level: Rgba::rgb(255, 255, 255).with_alpha(232),
        empty: Rgba::rgb(255, 255, 255).with_alpha(94),
        // As the stock Settings draw it over black and over a dark grey.
        bolt: Rgba::rgb(255, 255, 255).with_alpha(207),
    },
    mono_icon: Rgba::rgb(110, 110, 110),
    danger: Rgba::rgb(255, 92, 84),
    danger_soft: Rgba::rgb(52, 24, 24),
    info: Rgba::rgb(214, 214, 214),
    outline: Rgba::rgb(140, 140, 140),
    spec_label: Rgba::rgb(128, 128, 128),
    // White as strong as the black in the light theme.
    row_press: Rgba::rgb(255, 255, 255).with_alpha(30),
    button_press: Rgba::rgb(255, 255, 255).with_alpha(26),
    // As far from the card as the light theme's are from its own.
    round: Rgba::rgb(41, 41, 41),
    round_mark: Rgba::rgb(118, 118, 118),
    // The arrows as much softer than the chevron as in the light theme; the
    // list a step lighter than the cards it opens over, and its shadow
    // darker, for it to show on them.
    updown: Rgba::rgb(54, 54, 54),
    popup: Rgba::rgb(41, 41, 41),
    shadow: Rgba::rgb(0, 0, 0).with_alpha(77),
    divider: Rgba::rgb(46, 46, 46),
};

/// Layout measured on a Find X3's ColorOS 17 settings, in dp, at the
/// panel's 480 dpi. The page margin is 16 dp, the cards sit 16 dp apart,
/// a row is 58 dp tall, and the title rides in the header beside a 37 dp
/// round back button instead of standing over the page.
fn coloros_metrics(w: u32, h: u32, px_per_dp: f32) -> Metrics {
    let dp = |v: f32| v * px_per_dp;
    Metrics {
        w,
        h,
        px_per_dp,
        margin: dp(16.0),
        // The stock cards are continuous-curvature squircles; a round
        // corner of this radius reads the same at this size.
        card_radius: dp(28.0),
        card_gap: dp(16.0),
        section_block: dp(48.6),
        section_label_top: dp(27.0),
        row_h: dp(58.0),
        row_pad_v: dp(18.0),
        inner: dp(16.0),
        icon: dp(24.0),
        icon_gap: dp(14.7),
        title_px: dp(20.0),
        // The title starts after the 37 dp disc that holds the back arrow.
        title_x: dp(66.3),
        title_top: dp(50.5),
        // The stock arrow is a wide, shallow chevron centred in a white
        // disc 37 dp across, its centre 35.8 dp in from the page's edge.
        back_x: dp(26.0),
        back_cy: dp(57.8),
        back_w: dp(19.4),
        back_h: dp(16.2),
        back_stroke: dp(1.7),
        stroke: dp(2.0),
        content_top: dp(96.0),
        search_h: dp(40.0),
        search_text_inset: dp(40.0),
        after_search: dp(16.0),
        row_title_px: dp(17.0),
        sub_px: dp(14.0),
        sub_gap: dp(4.0),
        sub_pitch: dp(19.0),
        label_px: dp(13.0),
        account_title_px: dp(20.0),
        account_row_h: dp(72.0),
        avatar: dp(40.0),
        avatar_inset: dp(16.0),
        badge_px: dp(12.0),
        badge_w: dp(56.0),
        badge_h: dp(24.0),
        chevron_w: dp(6.5),
        chevron_h: dp(12.0),
        chevron_inset: dp(16.0),
        chevron_stroke: dp(1.6),
        value_gap: dp(8.0),
        switch_w: dp(44.0),
        switch_h: dp(26.0),
        knob: dp(20.0),
        slider_h: dp(24.0),
        slider_knob: dp(18.0),
        status_x: dp(37.0),
        status_right: dp(40.0),
        status_bolt_right: dp(36.0),
        status_cy: dp(20.0),
        status_px: dp(14.0),
    }
}

// Sampled from the stock「我的设备」page: in light mode the cards are
// the ordinary settings cards; in dark mode they are white at low opacity
// over the field.
const LIGHT_HERO: Hero = Hero {
    glow: &glow::LIGHT,
    version: Rgba::rgb(0, 0, 0).with_alpha(153),
    card: LIGHT.card,
    title: LIGHT.text,
    value: LIGHT.value,
    chevron: LIGHT.chevron,
};

const DARK_HERO: Hero = Hero {
    glow: &glow::DARK,
    version: Rgba::rgb(255, 255, 255).with_alpha(153),
    card: Rgba::rgb(255, 255, 255).with_alpha(36),
    title: Rgba::rgb(244, 245, 248),
    value: Rgba::rgb(157, 161, 188),
    chevron: Rgba::rgb(118, 122, 160),
};

// Sampled from a Find X3 running ColorOS 17: a cool grey page, pure white
// cards with far rounder corners than HyperOS draws, ink rather than tinted
// icon tiles, and the stock blue of its switches. The hairline is the
// 1 px #e0e0e0 line between two rows of a card, inset to the words.
const COLOROS_LIGHT: Palette = Palette {
    bg: Rgba::rgb(240, 241, 242),
    card: Rgba::rgb(255, 255, 255),
    text: Rgba::rgb(25, 25, 25),
    subtitle: Rgba::rgb(110, 111, 111),
    value: Rgba::rgb(153, 153, 153),
    section: Rgba::rgb(110, 111, 111),
    chevron: Rgba::rgb(179, 179, 179),
    // The switch's fill, sampled at #0080ff.
    accent: Rgba::rgb(0, 128, 255),
    track: Rgba::rgb(233, 234, 235),
    switch_off: Rgba::rgb(229, 229, 229),
    search: Rgba::rgb(255, 255, 255),
    search_icon: Rgba::rgb(153, 153, 153),
    placeholder: Rgba::rgb(0, 0, 0).with_alpha(64),
    badge: Rgba::rgb(240, 241, 242),
    avatar: Rgba::rgb(232, 234, 236),
    avatar_glyph: Rgba::rgb(255, 255, 255),
    status: Rgba::rgb(25, 25, 25),
    battery: BatteryInk {
        charging: Rgba::rgb(29, 205, 58),
        low: Rgba::rgb(245, 62, 55),
        level: Rgba::rgb(0, 0, 0).with_alpha(196),
        empty: Rgba::rgb(0, 0, 0).with_alpha(77),
        bolt: Rgba::rgb(0, 0, 0).with_alpha(172),
    },
    mono_icon: Rgba::rgb(160, 160, 160),
    danger: Rgba::rgb(245, 62, 55),
    danger_soft: Rgba::rgb(253, 235, 233),
    info: Rgba::rgb(25, 25, 25),
    // ColorOS rows carry bare line icons in the text's own ink.
    outline: Rgba::rgb(25, 25, 25),
    spec_label: Rgba::rgb(153, 153, 153),
    row_press: Rgba::rgb(0, 0, 0).with_alpha(17),
    button_press: Rgba::rgb(0, 0, 0).with_alpha(17),
    round: Rgba::rgb(240, 241, 242),
    round_mark: Rgba::rgb(120, 120, 120),
    updown: Rgba::rgb(179, 179, 179),
    popup: Rgba::rgb(255, 255, 255),
    shadow: Rgba::rgb(0, 0, 0).with_alpha(40),
    divider: Rgba::rgb(224, 224, 224),
};

// The same design in the dark, where ColorOS lifts its cards only a little
// from a near-black page.
const COLOROS_DARK: Palette = Palette {
    bg: Rgba::rgb(15, 15, 16),
    card: Rgba::rgb(28, 28, 30),
    text: Rgba::rgb(233, 233, 233),
    subtitle: Rgba::rgb(150, 150, 150),
    value: Rgba::rgb(128, 128, 128),
    section: Rgba::rgb(140, 140, 140),
    chevron: Rgba::rgb(90, 90, 90),
    accent: Rgba::rgb(51, 153, 255),
    track: Rgba::rgb(44, 44, 46),
    switch_off: Rgba::rgb(58, 58, 60),
    search: Rgba::rgb(28, 28, 30),
    search_icon: Rgba::rgb(128, 128, 128),
    placeholder: Rgba::rgb(255, 255, 255).with_alpha(77),
    badge: Rgba::rgb(40, 40, 42),
    avatar: Rgba::rgb(51, 51, 53),
    avatar_glyph: Rgba::rgb(150, 150, 150),
    status: Rgba::rgb(233, 233, 233),
    battery: BatteryInk {
        charging: Rgba::rgb(29, 205, 58),
        low: Rgba::rgb(230, 75, 59),
        level: Rgba::rgb(255, 255, 255).with_alpha(232),
        empty: Rgba::rgb(255, 255, 255).with_alpha(94),
        bolt: Rgba::rgb(255, 255, 255).with_alpha(207),
    },
    mono_icon: Rgba::rgb(120, 120, 120),
    danger: Rgba::rgb(255, 92, 84),
    danger_soft: Rgba::rgb(52, 24, 24),
    info: Rgba::rgb(233, 233, 233),
    outline: Rgba::rgb(233, 233, 233),
    spec_label: Rgba::rgb(128, 128, 128),
    row_press: Rgba::rgb(255, 255, 255).with_alpha(20),
    button_press: Rgba::rgb(255, 255, 255).with_alpha(20),
    round: Rgba::rgb(44, 44, 46),
    round_mark: Rgba::rgb(160, 160, 160),
    updown: Rgba::rgb(90, 90, 90),
    popup: Rgba::rgb(38, 38, 40),
    shadow: Rgba::rgb(0, 0, 0).with_alpha(90),
    divider: Rgba::rgb(44, 44, 46),
};

const COLOROS_LIGHT_HERO: Hero = Hero {
    glow: &glow::LIGHT,
    version: Rgba::rgb(0, 0, 0).with_alpha(153),
    card: COLOROS_LIGHT.card,
    title: COLOROS_LIGHT.text,
    value: COLOROS_LIGHT.value,
    chevron: COLOROS_LIGHT.chevron,
};

const COLOROS_DARK_HERO: Hero = Hero {
    glow: &glow::DARK,
    version: Rgba::rgb(255, 255, 255).with_alpha(153),
    card: COLOROS_DARK.card,
    title: COLOROS_DARK.text,
    value: COLOROS_DARK.value,
    chevron: COLOROS_DARK.chevron,
};
