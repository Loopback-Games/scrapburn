//! Control bindings. Placeholder: Task 10 of the menu redesign makes these rebindable.

/// The player's control bindings. Today the controls are fixed in `controls.rs`, so `Default`
/// is the only value and carries no data yet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Bindings;

/// A row of the Controls tab: one control and the keys or buttons it uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindRow {
    Accelerate,
    Brake,
    Steer,
    DriftHop,
    UseItem,
    Respawn,
    LookBehind,
    /// Fixed: pause is never rebindable, so the player can't lock themselves out.
    Pause,
}

impl BindRow {
    pub const ALL: [BindRow; 8] = [
        BindRow::Accelerate,
        BindRow::Brake,
        BindRow::Steer,
        BindRow::DriftHop,
        BindRow::UseItem,
        BindRow::Respawn,
        BindRow::LookBehind,
        BindRow::Pause,
    ];
}

impl Bindings {
    /// The keys (or, with `pad`, the gamepad buttons) bound to `row`, for display.
    pub fn text(&self, row: BindRow, pad: bool) -> String {
        let (keys, buttons) = match row {
            BindRow::Accelerate => ("W / Up", "RT or A"),
            BindRow::Brake => ("S / Down", "LT"),
            BindRow::Steer => ("A D / Left Right", "Left stick"),
            BindRow::DriftHop => ("Space / Shift", "LB or RB"),
            BindRow::UseItem => ("E / Ctrl / J", "X"),
            BindRow::Respawn => ("R", "B"),
            BindRow::LookBehind => ("Q", "Y"),
            BindRow::Pause => ("Esc", "Menu"),
        };
        if pad { buttons } else { keys }.to_string()
    }
}
