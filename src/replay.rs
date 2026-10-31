//! Checks against sequences recorded from the running game (private tools/recorder output).
use std::path::Path;

/// Seeded UnityEngine.Random sequences logged by the recorder's PPG_RANDOM_TEST mode.
pub fn compare_random(path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut random = crate::unity::Random::new(0);
    let (mut compared, mut mismatches) = (0, 0);
    let bits = |t: &str| u32::from_str_radix(t, 16).unwrap();
    for line in text.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        let (game, ours): (Vec<u32>, Vec<u32>) = match t.first().copied() {
            Some("rinit") => {
                random = crate::unity::Random::new(t[1].parse().unwrap());
                (t[2..6].iter().map(|s| bits(s)).collect(), random.state.to_vec())
            }
            Some("rv") => (vec![bits(t[1])], vec![random.value().to_bits()]),
            Some("rf") => {
                let (min, max) = (f32::from_bits(bits(t[1])), f32::from_bits(bits(t[2])));
                (vec![bits(t[3])], vec![random.range(min, max).to_bits()])
            }
            Some("ri") => (vec![t[3].parse::<i32>().unwrap() as u32], vec![random.range_int(t[1].parse().unwrap(), t[2].parse().unwrap()) as u32]),
            Some("rc") => (t[1..3].iter().map(|s| bits(s)).collect(), random.inside_unit_circle().map(f32::to_bits).to_vec()),
            Some("rs") => (t[1..4].iter().map(|s| bits(s)).collect(), random.inside_unit_sphere().map(f32::to_bits).to_vec()),
            Some("ro") => (t[1..4].iter().map(|s| bits(s)).collect(), random.on_unit_sphere().map(f32::to_bits).to_vec()),
            Some("rstate") => (t[1..5].iter().map(|s| bits(s)).collect(), random.state.to_vec()),
            _ => continue,
        };
        compared += 1;
        if game != ours {
            mismatches += 1;
            if mismatches <= 12 {
                println!("{line}: rewrite {ours:08x?}");
            }
        }
    }
    println!("Game Random replay: {compared} values, {mismatches} mismatches");
    if compared == 0 || mismatches > 0 {
        return Err("UnityEngine.Random differs from the recorded game".into());
    }
    Ok(())
}
