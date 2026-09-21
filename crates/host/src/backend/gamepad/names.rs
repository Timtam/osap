//! What the buttons and axes are called, and what each source's raw report means in those names.
//!
//! **Positional names, SDL3's set in snake_case.** `south` is the bottom face button whatever
//! is printed on it — A on an Xbox pad, Cross on a PlayStation one, B on a Nintendo one — so a
//! module written for "the button that confirms in a menu" works on every family without a
//! table of its own. What is printed on the pad is the `label`, which exists for speech: a
//! blind player who is told "press Cross" by the game needs to hear "Cross" from us too.
//!
//! **Derived buttons** are the hub's own: a stick pushed far enough in one direction and a
//! trigger pulled past its threshold behave like buttons, with hysteresis, so a module can
//! listen for `left_stick_down` the way it listens for `dpad_down` and never write a threshold.
//!
//! **Unknown devices** (step 5, the HID source) report `button<N>` for HID button usage N and
//! the HID usage names for their axes. They are accepted here already, so a listener written
//! for such a pad does not become an error the day that source arrives.
//!
//! Everything in this file is pure and compiled on every platform, which is the point: the
//! XInput bit map below is tested on the macOS runner too, and a wrong bit is a wrong button
//! announced to somebody who cannot see which one they pressed.

use super::{Axis, Button, Family, Source};

/// Every physical button a mapped pad can have, in SDL3's order. The order is also the
/// order in which the hub reports several edges found in one report, so it is part of the
/// behaviour, not only of the listing.
pub const PHYSICAL: [Button; 21] = [
    Button::South,
    Button::East,
    Button::West,
    Button::North,
    Button::Back,
    Button::Guide,
    Button::Start,
    Button::LeftStick,
    Button::RightStick,
    Button::LeftShoulder,
    Button::RightShoulder,
    Button::DpadUp,
    Button::DpadDown,
    Button::DpadLeft,
    Button::DpadRight,
    Button::Misc1,
    Button::RightPaddle1,
    Button::LeftPaddle1,
    Button::RightPaddle2,
    Button::LeftPaddle2,
    Button::Touchpad,
];

/// The buttons the hub derives from analog values. Never reported by a source.
pub const DERIVED: [Button; 10] = [
    Button::LeftTrigger,
    Button::RightTrigger,
    Button::LeftStickUp,
    Button::LeftStickDown,
    Button::LeftStickLeft,
    Button::LeftStickRight,
    Button::RightStickUp,
    Button::RightStickDown,
    Button::RightStickLeft,
    Button::RightStickRight,
];

/// The six axes of a mapped pad. Sticks run -1..1 with y positive DOWN, as SDL has it;
/// triggers run 0..1.
pub const AXES: [Axis; 6] =
    [Axis::LeftX, Axis::LeftY, Axis::RightX, Axis::RightY, Axis::LeftTrigger, Axis::RightTrigger];

/// HID Generic Desktop usages an unmapped pad's axes are named after.
const HID_AXES: [(u16, &str); 8] = [
    (0x30, "x"),
    (0x31, "y"),
    (0x32, "z"),
    (0x33, "rx"),
    (0x34, "ry"),
    (0x35, "rz"),
    (0x36, "slider"),
    (0x37, "dial"),
];

pub fn button_name(b: Button) -> String {
    let s = match b {
        Button::South => "south",
        Button::East => "east",
        Button::West => "west",
        Button::North => "north",
        Button::Back => "back",
        Button::Guide => "guide",
        Button::Start => "start",
        Button::LeftStick => "left_stick",
        Button::RightStick => "right_stick",
        Button::LeftShoulder => "left_shoulder",
        Button::RightShoulder => "right_shoulder",
        Button::DpadUp => "dpad_up",
        Button::DpadDown => "dpad_down",
        Button::DpadLeft => "dpad_left",
        Button::DpadRight => "dpad_right",
        Button::Misc1 => "misc1",
        Button::RightPaddle1 => "right_paddle1",
        Button::LeftPaddle1 => "left_paddle1",
        Button::RightPaddle2 => "right_paddle2",
        Button::LeftPaddle2 => "left_paddle2",
        Button::Touchpad => "touchpad",
        Button::LeftTrigger => "left_trigger",
        Button::RightTrigger => "right_trigger",
        Button::LeftStickUp => "left_stick_up",
        Button::LeftStickDown => "left_stick_down",
        Button::LeftStickLeft => "left_stick_left",
        Button::LeftStickRight => "left_stick_right",
        Button::RightStickUp => "right_stick_up",
        Button::RightStickDown => "right_stick_down",
        Button::RightStickLeft => "right_stick_left",
        Button::RightStickRight => "right_stick_right",
        Button::Other(n) => return format!("button{n}"),
    };
    s.to_string()
}

/// The button a module named, or `None` for a typo. Exact and lowercase: these names are
/// identifiers a module writes once, and accepting `South` beside `south` would only mean two
/// spellings in the wild that a search for one of them misses.
pub fn parse_button(s: &str) -> Option<Button> {
    if let Some(n) = s.strip_prefix("button") {
        // `button0` is refused: HID button usages start at 1, and a module that wrote 0 has
        // counted from zero and would never hear the button it means.
        return match n.parse::<u16>() {
            Ok(v) if v >= 1 && !n.starts_with('0') && !n.starts_with('+') => Some(Button::Other(v)),
            _ => None,
        };
    }
    PHYSICAL.iter().chain(DERIVED.iter()).copied().find(|b| button_name(*b) == s)
}

pub fn axis_name(a: Axis) -> String {
    let s = match a {
        Axis::LeftX => "left_x",
        Axis::LeftY => "left_y",
        Axis::RightX => "right_x",
        Axis::RightY => "right_y",
        Axis::LeftTrigger => "left_trigger",
        Axis::RightTrigger => "right_trigger",
        Axis::Other(u) => {
            return HID_AXES
                .iter()
                .find(|(usage, _)| *usage == u)
                .map(|(_, n)| (*n).to_string())
                .unwrap_or_else(|| format!("axis{u:#06x}"))
        }
    };
    s.to_string()
}

pub fn parse_axis(s: &str) -> Option<Axis> {
    if let Some(a) = AXES.iter().copied().find(|a| axis_name(*a) == s) {
        return Some(a);
    }
    HID_AXES.iter().find(|(_, n)| *n == s).map(|(u, _)| Axis::Other(*u))
}

/// The other half of a stick, for the radial dead zone. `None` for a trigger or an unmapped
/// axis, which are dead-zoned on their own.
pub fn partner(a: Axis) -> Option<Axis> {
    match a {
        Axis::LeftX => Some(Axis::LeftY),
        Axis::LeftY => Some(Axis::LeftX),
        Axis::RightX => Some(Axis::RightY),
        Axis::RightY => Some(Axis::RightX),
        _ => None,
    }
}

pub fn family_name(f: Family) -> &'static str {
    match f {
        Family::Xbox => "xbox",
        Family::PlayStation => "playstation",
        Family::Nintendo => "nintendo",
        Family::Generic => "generic",
    }
}

pub fn source_name(s: Source) -> &'static str {
    match s {
        Source::XInput => "xinput",
        Source::Hid => "hid",
        Source::GameController => "gamecontroller",
        Source::Fake => "fake",
    }
}

/// What is printed on the pad, in English, for speech. Modules translate it if they speak
/// another language; the positional name is the thing to match on.
pub fn label(b: Button, family: Family) -> String {
    // The four directions and the derived stick directions read the same on every family.
    let common = match b {
        Button::DpadUp => Some("Up"),
        Button::DpadDown => Some("Down"),
        Button::DpadLeft => Some("Left"),
        Button::DpadRight => Some("Right"),
        Button::LeftStickUp => Some("Left stick up"),
        Button::LeftStickDown => Some("Left stick down"),
        Button::LeftStickLeft => Some("Left stick left"),
        Button::LeftStickRight => Some("Left stick right"),
        Button::RightStickUp => Some("Right stick up"),
        Button::RightStickDown => Some("Right stick down"),
        Button::RightStickLeft => Some("Right stick left"),
        Button::RightStickRight => Some("Right stick right"),
        Button::Touchpad => Some("Touchpad"),
        _ => None,
    };
    if let Some(s) = common {
        return s.to_string();
    }
    if let Button::Other(n) = b {
        return format!("Button {n}");
    }
    let s = match family {
        Family::Xbox => match b {
            Button::South => "A",
            Button::East => "B",
            Button::West => "X",
            Button::North => "Y",
            Button::Back => "View",
            Button::Start => "Menu",
            Button::Guide => "Xbox",
            Button::LeftShoulder => "LB",
            Button::RightShoulder => "RB",
            Button::LeftTrigger => "LT",
            Button::RightTrigger => "RT",
            Button::LeftStick => "Left stick",
            Button::RightStick => "Right stick",
            Button::Misc1 => "Share",
            // The Elite's back paddles, as Microsoft prints them: P1 and P2 on the right, P3
            // and P4 on the left, upper before lower. SDL's names are positional.
            Button::RightPaddle1 => "P1",
            Button::RightPaddle2 => "P2",
            Button::LeftPaddle1 => "P3",
            Button::LeftPaddle2 => "P4",
            _ => "",
        },
        Family::PlayStation => match b {
            Button::South => "Cross",
            Button::East => "Circle",
            Button::West => "Square",
            Button::North => "Triangle",
            // "Create" on a DualSense; one label per family keeps this a table, and "Share"
            // is what both a DualShock 4 and every Sony manual before 2020 call it.
            Button::Back => "Share",
            Button::Start => "Options",
            Button::Guide => "PS",
            Button::LeftShoulder => "L1",
            Button::RightShoulder => "R1",
            Button::LeftTrigger => "L2",
            Button::RightTrigger => "R2",
            Button::LeftStick => "L3",
            Button::RightStick => "R3",
            Button::Misc1 => "Mute",
            Button::LeftPaddle1 => "Left function",
            Button::RightPaddle1 => "Right function",
            Button::LeftPaddle2 => "Left back",
            Button::RightPaddle2 => "Right back",
            _ => "",
        },
        // Positional: Nintendo puts B at the bottom and A on the right, the other way round
        // from Xbox, and that difference is exactly why modules match on `south`.
        Family::Nintendo => match b {
            Button::South => "B",
            Button::East => "A",
            Button::West => "Y",
            Button::North => "X",
            Button::Back => "Minus",
            Button::Start => "Plus",
            Button::Guide => "Home",
            Button::Misc1 => "Capture",
            Button::LeftShoulder => "L",
            Button::RightShoulder => "R",
            Button::LeftTrigger => "ZL",
            Button::RightTrigger => "ZR",
            Button::LeftStick => "Left stick",
            Button::RightStick => "Right stick",
            Button::RightPaddle1 => "Right paddle",
            Button::LeftPaddle1 => "Left paddle",
            Button::RightPaddle2 => "Right paddle 2",
            Button::LeftPaddle2 => "Left paddle 2",
            _ => "",
        },
        Family::Generic => match b {
            Button::South => "South button",
            Button::East => "East button",
            Button::West => "West button",
            Button::North => "North button",
            Button::Back => "Back",
            Button::Start => "Start",
            Button::Guide => "Guide",
            Button::Misc1 => "Misc",
            Button::LeftShoulder => "Left shoulder",
            Button::RightShoulder => "Right shoulder",
            Button::LeftTrigger => "Left trigger",
            Button::RightTrigger => "Right trigger",
            Button::LeftStick => "Left stick",
            Button::RightStick => "Right stick",
            Button::RightPaddle1 => "Right paddle 1",
            Button::LeftPaddle1 => "Left paddle 1",
            Button::RightPaddle2 => "Right paddle 2",
            Button::LeftPaddle2 => "Left paddle 2",
            _ => "",
        },
    };
    s.to_string()
}

/// XInput's report in canonical terms.
///
/// The bits of `XINPUT_GAMEPAD.wButtons` are written out rather than imported, so this table
/// compiles, and is tested, on every platform; `gamepad/xinput.rs` holds them to windows-sys's
/// `Win32::UI::Input::XboxController` at compile time. Only the Windows source calls into it,
/// hence the allowance everywhere else.
#[cfg_attr(not(windows), allow(dead_code))]
pub mod xinput {
    use super::super::{Axis, Button, Snapshot};

    pub const DPAD_UP: u16 = 0x0001;
    pub const DPAD_DOWN: u16 = 0x0002;
    pub const DPAD_LEFT: u16 = 0x0004;
    pub const DPAD_RIGHT: u16 = 0x0008;
    pub const START: u16 = 0x0010;
    pub const BACK: u16 = 0x0020;
    pub const LEFT_THUMB: u16 = 0x0040;
    pub const RIGHT_THUMB: u16 = 0x0080;
    pub const LEFT_SHOULDER: u16 = 0x0100;
    pub const RIGHT_SHOULDER: u16 = 0x0200;
    /// Not in the documented header: the Guide button, reported only by the undocumented
    /// ordinal 100 (`XInputGetStateEx`), as SDL reads it.
    pub const GUIDE: u16 = 0x0400;
    pub const A: u16 = 0x1000;
    pub const B: u16 = 0x2000;
    pub const X: u16 = 0x4000;
    pub const Y: u16 = 0x8000;

    const MAP: [(u16, Button); 15] = [
        (A, Button::South),
        (B, Button::East),
        (X, Button::West),
        (Y, Button::North),
        (BACK, Button::Back),
        (GUIDE, Button::Guide),
        (START, Button::Start),
        (LEFT_THUMB, Button::LeftStick),
        (RIGHT_THUMB, Button::RightStick),
        (LEFT_SHOULDER, Button::LeftShoulder),
        (RIGHT_SHOULDER, Button::RightShoulder),
        (DPAD_UP, Button::DpadUp),
        (DPAD_DOWN, Button::DpadDown),
        (DPAD_LEFT, Button::DpadLeft),
        (DPAD_RIGHT, Button::DpadRight),
    ];

    /// The buttons an XInput pad has. `guide` only when ordinal 100 was found, because a
    /// button listed and never reported is a button a module waits for in vain.
    pub fn buttons(guide: bool) -> Vec<Button> {
        MAP.iter().map(|(_, b)| *b).filter(|b| guide || *b != Button::Guide).collect()
    }

    /// A thumbstick component, -32768..32767, to -1..1.
    ///
    /// Divided by 32767 and clamped, so full deflection is exactly 1 in both directions: the
    /// negative range is one count longer, and dividing by 32768 there would make "fully left"
    /// -1 and "fully right" 0.99997 — a stick that can never reach the right edge.
    pub fn thumb(v: i16) -> f32 {
        (v as f32 / 32767.0).clamp(-1.0, 1.0)
    }

    /// One report. The Y axes are negated: XInput has up positive, SDL and every screen
    /// coordinate have down positive, and a module should not have to know which source it
    /// is reading.
    pub fn snapshot(buttons: u16, left_trigger: u8, right_trigger: u8, thumbs: [i16; 4], guide: bool) -> Snapshot {
        let mut snap = Snapshot::default();
        for (bit, b) in MAP {
            if buttons & bit != 0 && (guide || b != Button::Guide) {
                snap.pressed.insert(b);
            }
        }
        snap.axes = vec![
            (Axis::LeftX, thumb(thumbs[0])),
            (Axis::LeftY, -thumb(thumbs[1])),
            (Axis::RightX, thumb(thumbs[2])),
            (Axis::RightY, -thumb(thumbs[3])),
            (Axis::LeftTrigger, left_trigger as f32 / 255.0),
            (Axis::RightTrigger, right_trigger as f32 / 255.0),
        ];
        snap
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_canonical_name_round_trips() {
        for b in PHYSICAL.iter().chain(DERIVED.iter()) {
            assert_eq!(parse_button(&button_name(*b)), Some(*b), "{b:?}");
        }
        for a in AXES {
            assert_eq!(parse_axis(&axis_name(a)), Some(a), "{a:?}");
        }
        // No two buttons share a name, or a listener would get a button it never named.
        let mut names: Vec<String> =
            PHYSICAL.iter().chain(DERIVED.iter()).map(|b| button_name(*b)).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), PHYSICAL.len() + DERIVED.len());
    }

    #[test]
    fn unmapped_names_are_accepted_and_typos_are_not() {
        assert_eq!(parse_button("button7"), Some(Button::Other(7)));
        assert_eq!(button_name(Button::Other(7)), "button7");
        assert_eq!(parse_axis("rz"), Some(Axis::Other(0x35)));
        assert_eq!(axis_name(Axis::Other(0x36)), "slider");
        for bad in ["South", "dpad-up", "button0", "button", "button07", "button+3", "a", ""] {
            assert_eq!(parse_button(bad), None, "{bad}");
        }
        for bad in ["leftx", "LEFT_X", "throttle", ""] {
            assert_eq!(parse_axis(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_same_position_reads_differently_per_family() {
        assert_eq!(label(Button::South, Family::Xbox), "A");
        assert_eq!(label(Button::South, Family::PlayStation), "Cross");
        assert_eq!(label(Button::South, Family::Nintendo), "B");
        assert_eq!(label(Button::East, Family::Nintendo), "A");
        assert_eq!(label(Button::DpadDown, Family::PlayStation), "Down");
        assert_eq!(label(Button::Other(3), Family::Generic), "Button 3");
        // Every canonical button has a label on every family: an empty string would be read
        // out as nothing at all.
        for f in [Family::Xbox, Family::PlayStation, Family::Nintendo, Family::Generic] {
            for b in PHYSICAL.iter().chain(DERIVED.iter()) {
                assert!(!label(*b, f).is_empty(), "{b:?} on {f:?}");
            }
        }
    }

    #[test]
    fn the_xinput_bit_map_is_the_documented_one() {
        let s = xinput::snapshot(xinput::A | xinput::DPAD_DOWN | xinput::START, 0, 0, [0; 4], false);
        let got: Vec<Button> = s.pressed.iter().copied().collect();
        assert_eq!(got, vec![Button::South, Button::Start, Button::DpadDown]);

        // Guide only through ordinal 100: the bit is ignored when the pad was not read that way.
        let without = xinput::snapshot(xinput::GUIDE, 0, 0, [0; 4], false);
        assert!(without.pressed.is_empty());
        let with = xinput::snapshot(xinput::GUIDE, 0, 0, [0; 4], true);
        assert!(with.pressed.contains(&Button::Guide));
        assert!(!xinput::buttons(false).contains(&Button::Guide));
        assert!(xinput::buttons(true).contains(&Button::Guide));
    }

    #[test]
    fn xinput_sticks_flip_y_and_reach_both_edges() {
        let s = xinput::snapshot(0, 255, 0, [32767, 32767, -32768, -32768], false);
        let v = |a: Axis| s.axes.iter().find(|(x, _)| *x == a).unwrap().1;
        assert_eq!(v(Axis::LeftX), 1.0);
        assert_eq!(v(Axis::LeftY), -1.0, "stick pushed up reads negative, like SDL");
        assert_eq!(v(Axis::RightX), -1.0, "-32768 clamps to exactly -1");
        assert_eq!(v(Axis::RightY), 1.0);
        assert_eq!(v(Axis::LeftTrigger), 1.0);
        assert_eq!(v(Axis::RightTrigger), 0.0);
        assert_eq!(xinput::thumb(0), 0.0);
    }
}
