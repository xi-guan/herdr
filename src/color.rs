use ratatui::style::Color;

use crate::api::schema::AgentStatus;
use crate::app::state::Palette;
// upstream keeps these beside the selection colours; re-exported so colour maths has one home
pub(crate) use crate::ui::{color_to_rgb, mix_rgb, relative_luminance};

/// the colour an agent asking for you wears on its border, its bar and its sidebar row alike.
pub(crate) fn attention_color(status: AgentStatus, palette: &Palette) -> Option<Color> {
    match status {
        AgentStatus::Blocked => Some(palette.red),
        // agents signal work in this tier themselves, so the row agrees with the pane
        AgentStatus::Working => Some(palette.peach),
        AgentStatus::Done => Some(palette.green),
        AgentStatus::Idle | AgentStatus::Unknown => None,
    }
}

/// text on an accent fill: punching the panel colour out vanishes on a dark accent and panel.
pub(crate) fn panel_contrast_fg(palette: &Palette) -> Color {
    ink_pole(palette.accent)
}

/// whichever of black or white gains contrast on `background`; unknown grounds answer white.
pub(crate) fn ink_pole(background: Color) -> Color {
    let against = |ink: Color| contrast_ratio(ink, background).unwrap_or(0.0);
    if against(Color::White) >= against(Color::Black) {
        Color::White
    } else {
        Color::Black
    }
}

/// the pole a colour recedes toward on `background`, so one call dims on dark and light alike.
pub(crate) fn ground_pole(background: Color) -> Color {
    match ink_pole(background) {
        Color::White => Color::Black,
        _ => Color::White,
    }
}

/// moves a concrete colour part of the way to `pole`; an ANSI slot is the terminal's to keep.
pub(crate) fn nudge_toward(color: Color, pole: Color, part: f32) -> Color {
    let (Color::Rgb(r, g, b), Some(pole)) = (color, color_to_rgb(pole)) else {
        return color;
    };
    let (r, g, b) = mix_rgb((r, g, b), pole, part);
    Color::Rgb(r, g, b)
}

/// the quietest step from `quiet` toward `toward` that reaches `floor` on `background`.
pub(crate) fn lift_until_legible(
    quiet: Color,
    toward: Color,
    background: Color,
    floor: f32,
) -> Color {
    const STEPS: u8 = 8;

    // a colour the terminal owns has no ratio to walk by
    let (Some((r, g, b)), Some(_)) = (color_to_rgb(quiet), color_to_rgb(toward)) else {
        return toward;
    };
    let quiet = Color::Rgb(r, g, b);
    for step in 0..=STEPS {
        let candidate = nudge_toward(quiet, toward, f32::from(step) / f32::from(STEPS));
        if contrast_ratio(candidate, background).is_none_or(|ratio| ratio >= floor) {
            return candidate;
        }
    }
    toward
}

/// WCAG contrast ratio, or `None` when either colour is one the terminal owns.
pub(crate) fn contrast_ratio(a: Color, b: Color) -> Option<f32> {
    let (first, second) = (
        relative_luminance(color_to_rgb(a)?),
        relative_luminance(color_to_rgb(b)?),
    );
    Some((first.max(second) + 0.05) / (first.min(second) + 0.05))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ink_pole_picks_the_ink_that_reads_on_the_fill() {
        assert_eq!(ink_pole(Color::Rgb(24, 24, 37)), Color::White);
        assert_eq!(ink_pole(Color::Rgb(239, 241, 245)), Color::Black);
        // a ground the terminal owns cannot be measured, and dark terminals are the norm
        assert_eq!(ink_pole(Color::Reset), Color::White);
        assert_eq!(ground_pole(Color::Rgb(24, 24, 37)), Color::Black);
        assert_eq!(ground_pole(Color::Rgb(239, 241, 245)), Color::White);
    }

    #[test]
    fn nudge_toward_moves_only_concrete_colours() {
        assert_eq!(
            nudge_toward(Color::Rgb(0, 100, 200), Color::White, 0.5),
            Color::Rgb(128, 178, 228)
        );
        assert_eq!(nudge_toward(Color::Blue, Color::White, 0.5), Color::Blue);
        assert_eq!(
            nudge_toward(Color::Rgb(0, 100, 200), Color::Reset, 0.5),
            Color::Rgb(0, 100, 200)
        );
    }

    #[test]
    fn lift_until_legible_stops_at_the_first_step_that_reaches_the_floor() {
        let ground = Color::Rgb(30, 30, 46);
        let quiet = Color::Rgb(40, 40, 56);
        let toward = Color::Rgb(205, 214, 244);

        let lifted = lift_until_legible(quiet, toward, ground, 3.0);
        let ratio = contrast_ratio(lifted, ground).expect("concrete colours resolve");
        assert!(ratio >= 3.0, "lifted to only {ratio:.2}:1");
        assert_ne!(
            lifted, toward,
            "a reachable floor stops short of the target"
        );

        // already legible stays put, and an unmeasurable end falls back to the target
        assert_eq!(lift_until_legible(toward, quiet, ground, 3.0), toward);
        assert_eq!(
            lift_until_legible(Color::Reset, toward, ground, 3.0),
            toward
        );
    }

    #[test]
    fn contrast_ratio_declines_colours_the_terminal_owns() {
        assert_eq!(contrast_ratio(Color::Reset, Color::Black), None);
        assert_eq!(contrast_ratio(Color::Indexed(42), Color::Black), None);
        let ratio = contrast_ratio(Color::White, Color::Black).expect("both resolve");
        assert!(
            (ratio - 21.0).abs() < 0.01,
            "black on white is {ratio:.2}:1"
        );
    }

    #[test]
    fn only_agents_asking_for_you_wear_an_attention_colour() {
        let palette = Palette::catppuccin();
        assert_eq!(
            attention_color(AgentStatus::Blocked, &palette),
            Some(palette.red)
        );
        assert_eq!(
            attention_color(AgentStatus::Working, &palette),
            Some(palette.peach)
        );
        assert_eq!(
            attention_color(AgentStatus::Done, &palette),
            Some(palette.green)
        );
        assert_eq!(attention_color(AgentStatus::Idle, &palette), None);
        assert_eq!(attention_color(AgentStatus::Unknown, &palette), None);
    }
}
