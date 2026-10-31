mod assets;
mod b2;
mod game;
mod game_assets;
mod hinge;
mod integration;
mod motor;
mod play;
mod replay;
mod ui;
mod unity;

use game_assets::HumanRig;
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut explicit_game = None;
    let mut cache = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("local-cache");
    let mut headless = false;
    let mut reference_probe = None;
    let mut native_probe = None;
    let mut b2_probe = None;
    let mut script = None;
    let mut looped = None;
    let mut play = None;
    let mut play_map = None;
    let mut item = None;
    let mut target = None;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--game-dir" => {
                explicit_game = Some(PathBuf::from(
                    arguments.next().ok_or("--game-dir needs a path")?,
                ))
            }
            "--cache" => cache = arguments.next().ok_or("--cache needs a path")?.into(),
            "--reference-test" => {
                reference_probe =
                    Some(PathBuf::from(arguments.next().ok_or(
                        "--reference-test needs the private probe executable path",
                    )?))
            }
            "--native-reference-test" => {
                native_probe = Some(PathBuf::from(
                    arguments
                        .next()
                        .ok_or("--native-reference-test needs a probe path")?,
                ));
            }
            "--b2-reference-test" => {
                b2_probe = Some(PathBuf::from(
                    arguments.next().ok_or("--b2-reference-test needs a probe path")?,
                ));
            }
            "--random-test" => {
                let path = PathBuf::from(arguments.next().ok_or("--random-test needs a recording")?);
                return replay::compare_random(&path);
            }
            "--physics-test" => {
                let path = PathBuf::from(arguments.next().ok_or("--physics-test needs a PPG_DUMP recording")?);
                return game::dump::physics::physics_test(&path);
            }
            "--loop-test" => {
                looped = Some(PathBuf::from(arguments.next().ok_or("--loop-test needs a PPG_DUMP recording")?));
            }
            "--play-test" => {
                play = Some(PathBuf::from(arguments.next().ok_or("--play-test needs a PPG_DUMP recording")?));
                headless = true;
            }
            "--play" => {
                play = Some(PathBuf::from(arguments.next().ok_or("--play needs a PPG_DUMP recording")?));
            }
            "--play-map" => play_map = Some(arguments.next().ok_or("--play-map needs a map name")?),
            "--item" => item = Some(arguments.next().ok_or("--item needs a catalog item name")?),
            "--target" => target = Some(arguments.next().ok_or("--target needs a catalog item name")?),
            "--play-map-test" => {
                play_map = Some(arguments.next().ok_or("--play-map-test needs a map name")?);
                headless = true;
            }
            "--script-test" => {
                script = Some(PathBuf::from(arguments.next().ok_or("--script-test needs a PPG_DUMP recording")?));
            }
            _ => return Err(format!("Unknown argument: {argument}")),
        }
    }
    if let Some(probe) = b2_probe {
        unity::compare_trig(&probe)?;
        return b2::probe::compare_reference(&probe);
    }
    let game = game_assets::find_game(explicit_game)?;
    let rig = HumanRig::load(&cache, &game)?;
    let cache = cache.canonicalize().map_err(|e| e.to_string())?;
    let start = match (play, play_map) {
        (Some(recording), _) => Some(play::Start::Recording(recording)),
        (None, Some(map)) => Some(play::Start::Map(game.clone(), map, item, target)),
        // Without a map, and without one of the other tests, the main menu.
        (None, None) if script.is_none() && looped.is_none() && reference_probe.is_none() && native_probe.is_none() && !headless => Some(play::Start::Menu(game.clone())),
        (None, None) => None,
    };
    if let Some(start) = start {
        if headless {
            return play::test(&start, &rig);
        }
        return play::run(&start, rig, cache);
    }
    if let Some(dump) = script {
        return game::dump::script_test(&dump, &rig);
    }
    if let Some(dump) = looped {
        return game::dump::loop_test(&dump, &rig);
    }
    if let Some(probe) = reference_probe {
        return motor::compare_reference(&probe);
    }
    if let Some(probe) = native_probe {
        hinge::compare_reference(&probe)?;
        return integration::compare_reference(&probe);
    }
    Ok(())
}
