//! Renders a soundtrack piece to a WAV file: about one pass through the song and into the
//! loop, with the final-lap layers coming in two thirds of the way.
//! `cargo run -p rivet-audio --release --example music_demo -- [menu|dust|castle|sky] [out.wav]`

use rivet_audio::{Command, Mixer, Theme};

const RATE: u32 = 48_000;

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let theme = match args.next().as_deref() {
        Some("menu") => Theme::Menu,
        None | Some("dust") => Theme::Dust,
        Some("castle") => Theme::Castle,
        Some("sky") => Theme::Sky,
        Some(other) => {
            eprintln!("unknown theme '{other}' (use menu, dust, castle or sky)");
            std::process::exit(2);
        }
    };
    let path = args.next().unwrap_or_else(|| "music_demo.wav".into());
    let mut m = Mixer::new(RATE as f32);
    let mut samples: Vec<f32> = Vec::new();
    let step = 0.02f32; // command rate: 50 Hz, like a game frame
    let total = 92.0f32;
    let mut t = 0.0f32;
    let started = std::time::Instant::now();
    while t < total {
        let intensity = if t > total * 0.66 { 1.0 } else { 0.0 };
        let gain = if t > total - 2.0 { 0.0 } else { 1.0 };
        m.apply(Command::Music {
            theme,
            gain,
            muffle: 0.0,
            intensity,
        });
        let mut buf = vec![0.0f32; (RATE as f32 * step) as usize * 2];
        m.render(&mut buf, 2);
        samples.extend(buf);
        t += step;
    }
    let took = started.elapsed().as_secs_f32();
    println!(
        "rendered {total} s in {took:.2} s ({:.1}% of one core in real time)",
        took / total * 100.0
    );
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
