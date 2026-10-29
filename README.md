# People Playground / Bevy

An early Windows Rust/Bevy ragdoll sandbox prototype. **This is not a complete or verified 1:1 port.**

Requires your own installed copy of People Playground. Original assets, extracted caches, game DLLs, and recovered C# are not included in this repository.

## Current behavior

- A human ragdoll imported from the installed game's prefab: 14 bodies and 13 joints.
- Original skin texture, sprite rectangles, pivots, masses, damping, collider boxes, anchors, and joint limits loaded from a private local cache.
- Mouse dragging and throwing, manual joint detachment, human/crate spawning, pause, and reset.
- Healthy-muscle control with imported Rest, Sitting, and Flat pose targets and strength settings; upright support while grounded.
- A limp toggle and joint-connectivity checks that disable muscles on disconnected limbs or after brain disconnection.
- Installation and cache-fingerprint checks at startup.
- Headless engine tests with no window or desktop control.

Bevy 0.20 handles rendering and Avian 0.8 handles physics. The simulation uses a 50 Hz fixed update with 10 physics substeps. Unity/Box2D and Avian do not produce equivalent trajectories by assumption; behavioral parity remains unverified.

Living physiology, injury-dependent strength, wounds and blood, heat and electricity, the original item catalog, contraptions, saves, and mods are not implemented. The muscle servo uses Avian's implicit spring-damper model with imported torque limits; its response has not been matched to the original game's motor solver. Pose tracking can be obstructed by floor contact. The crate and toolbar are prototype elements. Experimental desktop-toggle code exists, but overlay testing and further work are deferred.

## Requirements

- Windows x64 and Rust/Cargo; developed with Rust 1.99.
- An installed copy of People Playground.
- Python 3 and the dependency in `scripts/requirements.txt` for preparing the local cache.
- [AssetRipper](https://github.com/AssetRipper/AssetRipper/releases) for exporting the installed game's assets. Development used AssetRipper 2.0.0 and a Unity 2020.3.1f1 export.

## Prepare your local assets

1. Load your installed game's `People Playground_Data` directory in AssetRipper.
2. Use **Export Unity Project** and select a new, empty `unity-export` directory at the repository root. The expected prefab is `unity-export/ExportedProject/Assets/GameObject/Person.prefab`.
3. Install the importer's dependency:

   ```powershell
   python -m pip install -r scripts/requirements.txt
   ```

4. Preview the import with your own installation path:

   ```powershell
   python scripts/prepare-assets.py --game "F:\SteamLibrary\steamapps\common\People Playground"
   ```

5. Write the previewed cache:

   ```powershell
   python scripts/prepare-assets.py --game "F:\SteamLibrary\steamapps\common\People Playground" --write
   ```

The importer resolves the prefab's sprite and texture references and writes `local-cache/`. It refuses to overwrite an existing cache. After a game update, export again and select a fresh directory with `--output`; pass that directory to the app with `--cache`.

Caches prepared before muscle support still load as passive ragdolls. Prepare a fresh cache to import muscle settings and poses.

Generated assets, exports, recovered source, binaries, and caches must stay out of Git. The included `.gitignore` excludes them.

## Run

```powershell
powershell -NoProfile -File .\run.ps1
```

The launcher uses DirectX 12 unless `WGPU_BACKEND` is already set. The app searches common Steam libraries. For a custom installation:

```powershell
cargo run -- --game-dir "F:\SteamLibrary\steamapps\common\People Playground"
```

`PPG_GAME_DIR` can also specify the installation. `--cache "<directory>"` selects another prepared cache. The app requires the installed executable, gameplay assembly, and asset packages, and compares their sizes and modification times against the cache at startup. This is a dependency check, not Steam ownership verification.

| Control | Action |
| --- | --- |
| Left mouse | Drag and throw |
| Right mouse | Detach the selected limb's joint |
| N | Spawn a human at the cursor |
| B | Spawn a prototype crate |
| M, while hovering a human | Toggle that human's muscles / limp state |
| P, while hovering a human | Cycle Rest, Sitting, and Flat poses |
| Space | Pause or resume |
| R | Reset the sandbox |
| Escape | Exit |

F10 is an experimental desktop toggle; use the normal window for now.

## Verify without opening a window

```powershell
python scripts/test_prepare_assets.py
cargo test
cargo run -- --headless-test
```

The headless test requires a local game installation and prepared cache. It simulates passive settling, dragging, and release under gravity, checking body/joint counts, finite physics, floor containment, joint anchor gaps, and render-transform synchronization. With a muscle-enabled cache it also checks upright rest, limp collapse, all sitting-pose motors in free space, limb/brain disconnection, and independence between two humans. It creates no window or renderer.

An optional `--smoke-test` opens a rendered window and checks the original texture, floor collisions, and joints over at least four seconds of simulation. Run that only when explicitly desired.
