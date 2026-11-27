//! The soundtrack's songs as sequencer data: original pieces, one for the menus and
//! one per circuit. `music.rs` plays them.
//!
//! Every pattern is a 16-character string, one character per 16th note of a 4/4 bar.

/// 16ths in a bar.
pub(crate) const STEPS: usize = 16;

/// Which piece plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// The menus: a laid-back groove in B minor, 112 BPM.
    #[default]
    Menu,
    /// Dustbowl: crossover rock in E minor, 168 BPM.
    Dust,
    /// Castle Grindstone: galloping metal in D minor, 176 BPM.
    Castle,
    /// Skyway Zero: bright synth-rock in F-sharp minor, 150 BPM.
    Sky,
    Glass,
    Solar,
}

impl Theme {
    pub const ALL: [Theme; 6] = [
        Theme::Menu,
        Theme::Dust,
        Theme::Castle,
        Theme::Sky,
        Theme::Glass,
        Theme::Solar,
    ];

    pub(crate) fn song(self) -> &'static Song {
        match self {
            Theme::Menu => &MENU,
            Theme::Dust => &DUST,
            Theme::Castle => &CASTLE,
            Theme::Sky => &SKY,
            Theme::Glass => &GLASS,
            Theme::Solar => &SOLAR,
        }
    }
}

/// How a song sounds: the same band, set up differently.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tone {
    /// Rhythm-guitar gain into the distortion.
    pub drive: f32,
    /// Guitar cabinet low-pass (Hz): lower is warmer.
    pub cab: f32,
    /// Mix levels.
    pub guitar: f32,
    pub lead: f32,
    pub arp: f32,
    /// Lead send into the ping-pong delay.
    pub delay: f32,
    /// Semitones added to the written melody.
    pub lead_shift: f32,
    /// A clean, gliding synth lead instead of an overdriven guitar-like one.
    pub synth: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Song {
    pub bpm: f32,
    /// Section the song returns to after the last one.
    pub loop_to: usize,
    pub sections: &'static [Section],
    /// Pitch classes (0 = C) of the roots whose chords take a major third (the rest minor),
    /// for the arpeggio.
    pub majors: &'static [u8],
    pub tone: Tone,
}

impl Song {
    /// The third (3 or 4 semitones) over `root`.
    pub fn third(&self, root: u8) -> u8 {
        if self.majors.contains(&(root % 12)) {
            4
        } else {
            3
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum BassPart {
    Off,
    /// Locks to the rhythm guitar, re-picking held chords on every 8th.
    Follow,
    /// Its own rhythm over the bar's chord: `x` picks the root, `o` the octave above, `.`
    /// rests, anything else holds.
    Pattern(&'static [&'static str]),
}

/// One bar of drums, a 16-character string per piece (empty: silent).
/// Kick and toms: `x` hits (`h` `m` `l` `f` pick the tom). Snare: `x`, `X` accent, `g`
/// ghost, `r` a roll stroke that swells across the bar. Hats: `x` closed, `-` soft, `o` open.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Groove {
    pub kick: &'static str,
    pub snare: &'static str,
    pub hat: &'static str,
    pub tom: &'static str,
}

const fn groove(
    kick: &'static str,
    snare: &'static str,
    hat: &'static str,
    tom: &'static str,
) -> Groove {
    Groove {
        kick,
        snare,
        hat,
        tom,
    }
}

/// A bar of melody: (MIDI note, length in 16ths); note 0 rests. Lengths sum to 16.
pub(crate) type Phrase = &'static [(u8, u8)];

/// Rhythm guitar for one bar: (pattern, notes). Pattern: `O` open, `H` half-muted chug,
/// `M` palm mute, `-` hold, `.` choke, `>` open on the next bar's chord (a push). Notes: a
/// chord letter per step (`C` `D` `E` `F` `G` `A` `B`, lowercase for the sharp), or empty
/// for the bar's chord.
pub(crate) type Riff = (&'static str, &'static str);

#[derive(Debug, Clone, Copy)]
pub(crate) struct Section {
    pub bars: usize,
    /// Root (MIDI, guitar register) of each bar, cycled.
    pub chords: &'static [u8],
    /// Rhythm guitar per bar, cycled.
    pub guitar: &'static [Riff],
    pub bass: BassPart,
    pub drums: &'static [Groove],
    /// The last bar is a tom fill.
    pub fill: bool,
    /// Crash on the first bar, then every this many bars (0: only the first).
    pub crash_every: usize,
    pub lead: Option<&'static [Phrase]>,
    /// Lead doubled an octave down (the last chorus).
    pub lead_double: bool,
    /// The arpeggio plays here regardless of intensity.
    pub arp: bool,
    /// The guitars open up through a sweeping filter (an intro).
    pub sweep: bool,
    /// A noise riser over the last two bars.
    pub riser: bool,
}

const fn section(
    bars: usize,
    chords: &'static [u8],
    guitar: &'static [Riff],
    drums: &'static [Groove],
) -> Section {
    Section {
        bars,
        chords,
        guitar,
        bass: BassPart::Follow,
        drums,
        fill: false,
        crash_every: 0,
        lead: None,
        lead_double: false,
        arp: false,
        sweep: false,
        riser: false,
    }
}

// Chord roots in the guitar register (MIDI), D#2 up to D3.
const DS: u8 = 39;
const E: u8 = 40;
const F: u8 = 41;
const FS: u8 = 42;
const G: u8 = 43;
const A: u8 = 45;
const AS: u8 = 46;
const B: u8 = 47;
const C: u8 = 48;
const CS: u8 = 49;
const D: u8 = 50;

/// The root for a chord letter in a riff's note string.
pub(crate) fn chord_letter(c: u8) -> Option<u8> {
    Some(match c {
        b'C' => C,
        b'c' => CS,
        b'D' => D,
        b'd' => DS,
        b'E' => E,
        b'F' => F,
        b'f' => FS,
        b'G' => G,
        b'g' => 44,
        b'A' => A,
        b'a' => AS,
        b'B' => B,
        _ => return None,
    })
}

const SILENT: &[Riff] = &[("................", "")];

// Shared parts.

const PRE_GTR: &[Riff] = &[
    ("H-H-H-H-H-H-H-H-", ""),
    ("H-H-H-H-H-H-H-H-", ""),
    ("H-H-H-H-H-H-H-H-", ""),
    ("O--O--O-O-O-OOOO", ""),
];

const CHORUS_GTR: &[Riff] = &[("O-----O-O----->-", ""), ("O--O--O-O-H-H->-", "")];

const PRE_DRUMS: &[Groove] = &[
    groove(
        "x...x...x...x...",
        "....x.......x...",
        "x-x-x-x-x-x-x-x-",
        "",
    ),
    groove(
        "x...x...x...x...",
        "....x.......x...",
        "x-x-x-x-x-x-x-x-",
        "",
    ),
    groove(
        "x...x...x...x...",
        "....x.......x..g",
        "x-x-x-x-x-x-x-x-",
        "",
    ),
    groove(
        "x...x...x...x...",
        "r.r.r.r.rrrrrrrr",
        "x-x-x-x-........",
        "",
    ),
];

const BREAK_DRUMS: &[Groove] = &[
    groove("x.......x.......", "", "", ""),
    groove("x.......x.....x.", "............x...", "", ""),
    groove(
        "x...x...x...x...",
        "....x.......x...",
        "-.-.-.-.-.-.-.-.",
        "",
    ),
    groove("x.x.x.x.x.x.x.x.", "rrrrrrrrrrrrrrrr", "", ""),
];

/// The last bar of a section that ends with a fill.
pub(crate) const FILL: Groove = groove(
    "x.....x.x.......",
    "....x.....g.....",
    "x.x.x.x.........",
    "........hhmml.ff",
);

// Dustbowl: crossover rock. Intro (the riff through an opening filter), verse, pre-chorus,
// chorus with the hook, a second verse with the arpeggio, pre, chorus, breakdown, and a
// final chorus.

const DUST_RIFF: &[Riff] = &[
    ("OMM.MMO-MM.MO-O-", "EEEEEEGGEEEEAAGG"),
    ("OMM.MMO---O---O-", "EEEEEEDDDDCCCCBB"),
];

const DUST_BREAK_GTR: &[Riff] = &[
    ("O---------------", ""),
    ("----------------", ""),
    ("................", ""),
    ("........H-H-HHHH", ""),
];

const DUST_INTRO_DRUMS: &[Groove] = &[
    groove("", "", "x.x.x.x.x.x.x.x.", ""),
    groove("", "", "x.xxx.xxx.xxx.xx", ""),
    groove("x...x...x...x...", "", "x.x.x.x.x.x.x.x.", ""),
    groove("x...x...x...x...", "r.r.r.r.rrrrrrrr", "", ""),
];

const DUST_VERSE_DRUMS: &[Groove] = &[
    groove(
        "x.....x.x.......",
        "....x.......x...",
        "x.x.x.x.x.x.x.x.",
        "",
    ),
    groove(
        "x.....x.x.x...x.",
        "....x.......x..g",
        "x.x.x.x.x.x.x.o.",
        "",
    ),
];

const DUST_CHORUS_DRUMS: &[Groove] = &[
    groove(
        "x.....x.x.x..x..",
        "....X.......X...",
        "x.o.x.o.x.o.x.o.",
        "",
    ),
    groove(
        "x.....x.x.x...x.",
        "....X.......X.X.",
        "x.o.x.o.x.o.x.o.",
        "",
    ),
];

const DUST_BREAK_BASS: &[&str] = &[
    "x.xx.xx.x.xx.xx.",
    "x.xx.xx.x.xx.xx.",
    "x.x.x.x.x.x.x.x.",
    "x.x.x.x.xxxxxxxx",
];

/// Eight bars over Em C G D (twice).
const DUST_HOOK: &[Phrase] = &[
    &[
        (64, 2),
        (67, 2),
        (69, 2),
        (71, 4),
        (69, 2),
        (67, 2),
        (64, 2),
    ],
    &[(67, 2), (64, 2), (62, 2), (64, 6), (0, 4)],
    &[
        (62, 2),
        (64, 2),
        (67, 2),
        (69, 4),
        (71, 2),
        (74, 2),
        (71, 2),
    ],
    &[(69, 6), (67, 2), (66, 4), (62, 4)],
    &[
        (64, 2),
        (67, 2),
        (69, 2),
        (71, 4),
        (69, 2),
        (71, 2),
        (74, 2),
    ],
    &[(76, 6), (74, 2), (71, 4), (67, 4)],
    &[
        (69, 2),
        (71, 2),
        (69, 2),
        (67, 4),
        (64, 2),
        (62, 2),
        (64, 2),
    ],
    &[(66, 4), (69, 4), (71, 8)],
];

const DUST_VERSE: Section = Section {
    fill: true,
    ..section(8, &[E], DUST_RIFF, DUST_VERSE_DRUMS)
};
const DUST_PRE: Section = section(4, &[C, D, A, B], PRE_GTR, PRE_DRUMS);
const DUST_CHORUS: Section = Section {
    fill: true,
    crash_every: 4,
    lead: Some(DUST_HOOK),
    ..section(8, &[E, C, G, D], CHORUS_GTR, DUST_CHORUS_DRUMS)
};

const DUST: Song = Song {
    bpm: 168.0,
    loop_to: 1,
    majors: &[7, 0, 2],
    tone: Tone {
        drive: 22.0,
        cab: 5200.0,
        guitar: 0.34,
        lead: 0.2,
        arp: 0.15,
        delay: 0.3,
        lead_shift: 12.0,
        synth: false,
    },
    sections: &[
        Section {
            bass: BassPart::Off,
            sweep: true,
            ..section(4, &[E], DUST_RIFF, DUST_INTRO_DRUMS)
        },
        DUST_VERSE,
        DUST_PRE,
        DUST_CHORUS,
        Section {
            arp: true,
            ..DUST_VERSE
        },
        DUST_PRE,
        DUST_CHORUS,
        Section {
            bass: BassPart::Pattern(DUST_BREAK_BASS),
            arp: true,
            riser: true,
            ..section(4, &[E], DUST_BREAK_GTR, BREAK_DRUMS)
        },
        Section {
            crash_every: 2,
            lead_double: true,
            arp: true,
            ..DUST_CHORUS
        },
    ],
};

// Castle Grindstone: galloping metal in D minor. War drums under the intro riff, a gallop
// verse on double kick, a harmonic-minor hook over a running 16th-note kick, and a
// half-time breakdown.

const CASTLE_RIFF: &[Riff] = &[
    ("M.MMM.MMO-MMO-MM", "DDDDDDDDFFDDGGDD"),
    ("M.MMM.MMO---O---", "DDDDDDDDaaaaAAAA"),
];

const CASTLE_PRE_GTR: &[Riff] = &[
    ("H-H-H-H-H-H-H-H-", ""),
    ("H-H-H-H-H-H-H-H-", ""),
    ("O-------O-------", ""),
    ("O--O--O-O-O-OOOO", ""),
];

const CASTLE_BREAK_GTR: &[Riff] = &[
    ("M..M..M.M..M..M.", ""),
    ("M..M..M.M..M..M.", ""),
    ("M..M..M.M..M..M.", ""),
    ("M..M..M.MMMMMMMM", ""),
];

const GALLOP: &str = "x.xxx.xxx.xxx.xx";
const WAR_DRUMS: &str = "h..h..m.l..l..f.";

const CASTLE_INTRO_DRUMS: &[Groove] = &[
    groove("", "", "", WAR_DRUMS),
    groove("", "", "x...x...x...x...", WAR_DRUMS),
    groove(GALLOP, "", "x...x...x...x...", WAR_DRUMS),
    groove(GALLOP, "r.r.r.r.rrrrrrrr", "", ""),
];

const CASTLE_VERSE_DRUMS: &[Groove] = &[
    groove(GALLOP, "....x.......x...", "x...x...x...x...", ""),
    groove(GALLOP, "....x.......x.x.", "x...x...x...o...", ""),
];

const CASTLE_PRE_DRUMS: &[Groove] = &[
    groove(
        "x.x.x.x.x.x.x.x.",
        "....x.......x...",
        "x-x-x-x-x-x-x-x-",
        "",
    ),
    groove(
        "x.x.x.x.x.x.x.x.",
        "....x.......x...",
        "x-x-x-x-x-x-x-x-",
        "",
    ),
    groove(
        "x.x.x.x.x.x.x.x.",
        "....x.......x..g",
        "x-x-x-x-x-x-x-x-",
        "",
    ),
    groove(
        "x.x.x.x.x.x.x.x.",
        "r.r.r.r.rrrrrrrr",
        "x-x-x-x-........",
        "",
    ),
];

const CASTLE_CHORUS_DRUMS: &[Groove] = &[
    groove(
        "xxxxxxxxxxxxxxxx",
        "....X.......X...",
        "x.x.x.x.x.x.x.x.",
        "",
    ),
    groove(
        "xxxxxxxxxxxxxxxx",
        "....X.......X.X.",
        "x.x.x.x.x.x.x.x.",
        "",
    ),
];

const CASTLE_BREAK_DRUMS: &[Groove] = &[
    groove(
        "x..x..x.x..x..x.",
        "........x.......",
        "x...x...x...x...",
        "",
    ),
    groove(
        "x..x..x.x..x..x.",
        "........x.......",
        "x...x...x...x...",
        "",
    ),
    groove(
        "x..x..x.x..x..x.",
        "........x.......",
        "x...x...x...x...",
        "",
    ),
    groove("x..x..x.xxxxxxxx", "rrrrrrrrrrrrrrrr", "", ""),
];

/// Eight bars over Dm Bb F C, Dm Bb C A.
const CASTLE_HOOK: &[Phrase] = &[
    &[(74, 4), (69, 2), (74, 2), (76, 2), (77, 4), (76, 2)],
    &[(74, 6), (72, 2), (70, 4), (69, 4)],
    &[(72, 2), (69, 2), (65, 2), (69, 2), (72, 4), (77, 4)],
    &[(76, 8), (74, 4), (72, 4)],
    &[(74, 4), (69, 2), (74, 2), (76, 2), (77, 4), (79, 2)],
    &[(77, 6), (76, 2), (74, 4), (77, 4)],
    &[(76, 4), (79, 4), (76, 2), (72, 2), (74, 2), (76, 2)],
    &[(73, 8), (69, 4), (76, 4)],
];

const CASTLE_VERSE: Section = Section {
    fill: true,
    ..section(8, &[D], CASTLE_RIFF, CASTLE_VERSE_DRUMS)
};
const CASTLE_PRE: Section = section(4, &[AS, C, A, A], CASTLE_PRE_GTR, CASTLE_PRE_DRUMS);
const CASTLE_CHORUS: Section = Section {
    fill: true,
    crash_every: 2,
    lead: Some(CASTLE_HOOK),
    ..section(
        8,
        &[D, AS, F, C, D, AS, C, A],
        CHORUS_GTR,
        CASTLE_CHORUS_DRUMS,
    )
};

const CASTLE: Song = Song {
    bpm: 176.0,
    loop_to: 1,
    majors: &[5, 9, 10, 0],
    tone: Tone {
        drive: 32.0,
        cab: 4600.0,
        guitar: 0.36,
        lead: 0.2,
        arp: 0.12,
        delay: 0.25,
        lead_shift: 0.0,
        synth: false,
    },
    sections: &[
        Section {
            bass: BassPart::Off,
            sweep: true,
            ..section(4, &[D], CASTLE_RIFF, CASTLE_INTRO_DRUMS)
        },
        CASTLE_VERSE,
        CASTLE_PRE,
        CASTLE_CHORUS,
        Section {
            arp: true,
            ..CASTLE_VERSE
        },
        CASTLE_PRE,
        CASTLE_CHORUS,
        Section {
            arp: true,
            riser: true,
            ..section(4, &[D], CASTLE_BREAK_GTR, CASTLE_BREAK_DRUMS)
        },
        Section {
            crash_every: 1,
            lead_double: true,
            arp: true,
            ..CASTLE_CHORUS
        },
    ],
};

// Skyway Zero: synth-rock. The arpeggio lifts off alone, then octave-bounce bass,
// four-on-the-floor drums and a gliding synth hook over F#m D A E.

const OCTAVES: &[&str] = &["xoxoxoxoxoxoxoxo"];

const SKY_VERSE_GTR: &[Riff] = &[("O-------M.M.M.M.", ""), ("O-----O-M.M.M.M.", "")];

const SKY_INTRO_DRUMS: &[Groove] = &[
    groove("", "", "", ""),
    groove("", "", "x-x-x-x-x-x-x-x-", ""),
    groove("x...x...x...x...", "", "x-x-x-x-x-x-x-x-", ""),
    groove(
        "x...x...x...x...",
        "r.r.r.r.rrrrrrrr",
        "x-x-x-x-........",
        "",
    ),
];

const SKY_VERSE_DRUMS: &[Groove] = &[
    groove(
        "x...x...x...x...",
        "....x.......x...",
        "x.o.x.o.x.o.x.o.",
        "",
    ),
    groove(
        "x...x...x...x..x",
        "....x.......x..g",
        "x.o.x.o.x.o.x.o.",
        "",
    ),
];

const SKY_CHORUS_DRUMS: &[Groove] = &[
    groove(
        "x...x...x...x...",
        "....X.......X...",
        "x-o-x-o-x-o-x-o-",
        "",
    ),
    groove(
        "x...x...x...x.x.",
        "....X.......X.X.",
        "x-o-x-o-x-o-x-o-",
        "",
    ),
];

const SKY_BREAK_BASS: &[&str] = &[
    "x...............",
    "x.......x.......",
    "xoxoxoxoxoxoxoxo",
    "xoxoxoxoxoxoxoxo",
];

/// Eight bars over D E F#m A, D E C#m F#m.
const SKY_HOOK: &[Phrase] = &[
    &[(66, 2), (69, 2), (74, 4), (73, 2), (74, 2), (76, 4)],
    &[(76, 6), (74, 2), (73, 4), (71, 4)],
    &[(73, 2), (69, 2), (66, 2), (69, 2), (73, 4), (76, 4)],
    &[(76, 4), (78, 4), (76, 4), (73, 4)],
    &[(66, 2), (69, 2), (74, 4), (73, 2), (74, 2), (78, 4)],
    &[(80, 6), (78, 2), (76, 4), (71, 4)],
    &[(76, 4), (68, 4), (73, 4), (76, 4)],
    &[(78, 12), (76, 2), (73, 2)],
];

const SKY_VERSE: Section = Section {
    bass: BassPart::Pattern(OCTAVES),
    fill: true,
    arp: true,
    ..section(8, &[FS, D, A, E], SKY_VERSE_GTR, SKY_VERSE_DRUMS)
};
const SKY_PRE: Section = section(4, &[D, E, CS, CS], PRE_GTR, PRE_DRUMS);
const SKY_CHORUS: Section = Section {
    bass: BassPart::Pattern(OCTAVES),
    fill: true,
    crash_every: 4,
    lead: Some(SKY_HOOK),
    arp: true,
    ..section(
        8,
        &[D, E, FS, A, D, E, CS, FS],
        CHORUS_GTR,
        SKY_CHORUS_DRUMS,
    )
};

const SKY: Song = Song {
    bpm: 150.0,
    loop_to: 1,
    majors: &[2, 4, 9],
    tone: Tone {
        drive: 9.0,
        cab: 7000.0,
        guitar: 0.26,
        lead: 0.24,
        arp: 0.22,
        delay: 0.4,
        lead_shift: 0.0,
        synth: true,
    },
    sections: &[
        Section {
            bass: BassPart::Off,
            arp: true,
            ..section(4, &[FS, FS, D, E], SILENT, SKY_INTRO_DRUMS)
        },
        SKY_VERSE,
        SKY_PRE,
        SKY_CHORUS,
        SKY_VERSE,
        SKY_PRE,
        SKY_CHORUS,
        Section {
            bass: BassPart::Pattern(SKY_BREAK_BASS),
            arp: true,
            riser: true,
            ..section(4, &[FS, FS, D, E], SILENT, BREAK_DRUMS)
        },
        Section {
            crash_every: 2,
            lead_double: true,
            ..SKY_CHORUS
        },
    ],
};

// The menus: a laid-back groove in B minor. Syncopated mutes and a ringing chord on beat
// three, a bouncing bass, a mellow synth hook.

const MENU_GTR: &[Riff] = &[("M..M..M.O-------", ""), ("M..M..M.O---M.M.", "")];

const MENU_BASS: &[&str] = &["x..x..x.x.o..x..", "x..x..x.x.o.x.x."];

const MENU_DRUMS: &[Groove] = &[
    groove(
        "x......x..x.....",
        "....x.......x...",
        "x-x-x-x-x-x-x-x-",
        "",
    ),
    groove(
        "x......x..x..x..",
        "....x.......x..g",
        "x-x-x-x-x-x-x-o-",
        "",
    ),
];

const MENU_INTRO_DRUMS: &[Groove] = &[
    groove("", "", "x-x-x-x-x-x-x-x-", ""),
    groove("", "", "x-x-x-x-x-x-x-x-", ""),
    groove(
        "x......x..x.....",
        "....x.......x...",
        "x-x-x-x-x-x-x-x-",
        "",
    ),
    groove(
        "x......x..x..x..",
        "....x.......x.gr",
        "x-x-x-x-x-x-x-o-",
        "",
    ),
];

/// Eight bars over Bm G D A (twice).
const MENU_HOOK: &[Phrase] = &[
    &[(78, 4), (74, 2), (71, 2), (74, 4), (76, 4)],
    &[(74, 8), (71, 4), (69, 4)],
    &[(69, 2), (71, 2), (74, 4), (78, 4), (76, 4)],
    &[(76, 12), (0, 4)],
    &[(78, 4), (81, 4), (78, 2), (76, 2), (74, 4)],
    &[(71, 8), (74, 4), (76, 4)],
    &[(78, 6), (76, 2), (74, 4), (69, 4)],
    &[(73, 8), (76, 4), (0, 4)],
];

const MENU_GROOVE: Section = Section {
    bass: BassPart::Pattern(MENU_BASS),
    fill: true,
    ..section(8, &[B, G, D, A], MENU_GTR, MENU_DRUMS)
};
const MENU_CHORUS: Section = Section {
    crash_every: 4,
    lead: Some(MENU_HOOK),
    ..MENU_GROOVE
};

const MENU: Song = Song {
    bpm: 112.0,
    loop_to: 1,
    majors: &[7, 2, 9],
    tone: Tone {
        drive: 10.0,
        cab: 3800.0,
        guitar: 0.28,
        lead: 0.2,
        arp: 0.14,
        delay: 0.45,
        lead_shift: 0.0,
        synth: true,
    },
    sections: &[
        Section {
            bass: BassPart::Off,
            sweep: true,
            fill: false,
            ..section(4, &[B, G, D, A], MENU_GTR, MENU_INTRO_DRUMS)
        },
        MENU_GROOVE,
        MENU_CHORUS,
        Section {
            arp: true,
            ..MENU_GROOVE
        },
        Section {
            lead_double: true,
            arp: true,
            ..MENU_CHORUS
        },
    ],
};

// Glass Orchard: a descending bell figure against a syncopated, palm-muted pulse.
const GLASS: Song = Song {
    bpm: 142.,
    majors: &[0, 5, 7],
    loop_to: 0,
    tone: Tone {
        cab: 4600.,
        delay: 0.54,
        arp: 0.26,
        ..SKY.tone
    },
    sections: &[
        Section {
            arp: true,
            ..section(
                8,
                &[A, F, C, G],
                &[("M--M--H-.M--H---", "")],
                SKY_VERSE_DRUMS,
            )
        },
        Section {
            lead: Some(&[
                &[(81, 3), (79, 3), (76, 2), (72, 4), (76, 4)],
                &[(77, 4), (76, 2), (72, 2), (69, 8)],
            ]),
            fill: true,
            ..section(8, &[A, F, C, G], CHORUS_GTR, PRE_DRUMS)
        },
    ],
};

// Helios Crown: a rising brass-like hook over a four-on-the-floor climb.
const SOLAR: Song = Song {
    bpm: 160.,
    majors: &[2, 7, 9],
    loop_to: 0,
    tone: Tone {
        drive: 10.,
        cab: 6700.,
        lead: 0.30,
        ..DUST.tone
    },
    sections: &[
        Section {
            fill: true,
            ..section(8, &[D, A, B, G], &[("O---H-H-O---H-H-", "")], PRE_DRUMS)
        },
        Section {
            lead: Some(&[
                &[(74, 3), (78, 3), (81, 6), (83, 4)],
                &[(81, 4), (78, 4), (76, 8)],
            ]),
            lead_double: true,
            ..section(8, &[G, A, D, D], CHORUS_GTR, SKY_CHORUS_DRUMS)
        },
    ],
};
