//! Renders an engine sample to a WAV file: idle, a pull through the gears, lift-off.
//! `cargo run -p rivet-audio --example engine_demo -- out.wav`

use rivet_audio::{Command, Mixer};

const RATE: u32 = 48_000;

fn gear_rpm(ratio: f32) -> f32 {
    const SHIFTS: [f32; 6] = [0.0, 0.16, 0.34, 0.54, 0.76, 1.0];
    let g = (0..5).find(|&g| ratio < SHIFTS[g + 1]).unwrap_or(4);
    let t = ((ratio - SHIFTS[g]) / (SHIFTS[g + 1] - SHIFTS[g])).min(1.3);
    let floor = if g == 0 { 0.16 } else { 0.42 };
    floor + (0.95 - floor) * t
}

fn main() -> std::io::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "engine_demo.wav".into());
    let mut m = Mixer::new(RATE as f32);
    let mut samples: Vec<f32> = Vec::new();
    let step = 0.02f32; // command rate: 50 Hz, like a game frame
    let total = 9.0f32;
    let mut t = 0.0f32;
    while t < total {
        // 0-1.5 s idle, 1.5-7 s full throttle pulling to top speed, then lift off and coast.
        let (ratio, load) = if t < 1.5 {
            (0.0, 0.1)
        } else if t < 7.0 {
            (((t - 1.5) / 5.5).powf(0.8), 1.0)
        } else {
            (1.0 - (t - 7.0) * 0.15, 0.0)
        };
        m.apply(Command::Engine {
            id: 0,
            rpm: gear_rpm(ratio),
            load,
            gain: 0.9,
            pan: 0.0,
        });
        let mut buf = vec![0.0f32; (RATE as f32 * step) as usize * 2];
        m.render(&mut buf, 2);
        samples.extend(buf);
        t += step;
    }
    // 16-bit PCM stereo WAV.
    let data: Vec<u8> = samples
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect();
    let mut wav = Vec::with_capacity(44 + data.len());
    wav.extend(b"RIFF");
    wav.extend((36 + data.len() as u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(RATE.to_le_bytes());
    wav.extend((RATE * 4).to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((data.len() as u32).to_le_bytes());
    wav.extend(data);
    std::fs::write(&path, wav)?;
    println!("wrote {path}");
    Ok(())
}
