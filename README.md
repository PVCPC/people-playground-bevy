# People Playground / Bevy

An early Windows Rust/Bevy ragdoll sandbox prototype. **This is not a complete or verified 1:1 port.**

Requires your own installed copy of People Playground. Original assets, extracted caches, game DLLs, and recovered C# are not included in this repository.

## Current behavior

- A human ragdoll imported from the installed game's prefab: 14 bodies and 13 joints.
- Original skin texture, sprite rectangles, pivots, masses, damping, collider boxes, contact materials, anchors, and joint limits loaded from a private local cache.
- Mouse dragging and throwing, manual joint detachment, human/crate spawning, pause, and reset.
- Healthy-muscle control with imported Rest, Sitting, and Flat pose targets and strength settings; upright support while grounded.
- A limp toggle and joint-connectivity checks that disable muscles on disconnected limbs or after brain disconnection.
- Installation and cache-fingerprint checks at startup.
- Headless engine tests with no window or desktop control.

Bevy 0.20 handles rendering and Avian 0.8 handles collision and position constraints. Healthy static muscles use the recovered velocity-command calculations, applied by a sequential hinge velocity pass that co-solves anchor velocities and caps accumulated motor impulse by torque times the substep duration. The simulation uses a 50 Hz fixed update with 10 physics substeps and 16 hinge velocity iterations per substep. This replaces the experimental spring servo.

Fresh caches resolve each human body's runtime physics material through its physical-properties asset, replacing the prototype coefficients. Imported contacts use geometric-mean friction and maximum restitution mixing. Different materials on colliders within one compound body are rejected rather than silently merged. Native Unity motor-property limits are also applied before storing the speed for the next command: speed is bounded to ±1,000,000 degrees/second and torque to 0–1,000,000.

Living physiology, injury-dependent strength, wounds and blood, heat and electricity, the original item catalog, contraptions, saves, and mods are not implemented. The velocity pass uses standard [Box2D hinge constraint equations](https://github.com/erincatto/box2d/blob/v2.4.1/src/dynamics/b2_revolute_joint.cpp); this does not identify Unity's native Box2D fork or establish equivalent trajectories. Avian contact/position solving, solver ordering, and warm starting remain differences to investigate. The imported human starts with zero joint reference angles; nonzero-reference rigs have not been validated. Human bodies currently stay awake so small muscle commands continue to run. Pose tracking can be obstructed by floor contact. The crate and toolbar are prototype elements. Experimental desktop-toggle code exists, but overlay testing and further work are deferred.

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

Caches prepared before contact-material support retain the prototype friction and restitution and print a startup notice. Export and prepare a fresh cache in a new `--output` directory to import the material values; pass that directory with `--cache`.

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

The headless test requires a local game installation and prepared cache. It simulates passive settling, dragging, and release under gravity, checking body/joint counts, finite physics, floor containment, joint anchor gaps, and render-transform synchronization. With a muscle-enabled cache it also checks upright rest at five and ten seconds, limp collapse, all sitting-pose motors in free space, limb/brain disconnection, and independence between two humans. Unit tests check isolated hinge tracking, angular momentum conservation, the total torque budget across all velocity iterations, and disabled-joint behavior. These tests create no window or renderer.

`src/motor.rs` implements the healthy, static-pose motor-command calculations used by the live simulation, retaining the original degree units until conversion at the physics boundary. A local comparison against extracted C# methods passed 2,052 deterministic cases with identical float bits for speed and torque, including the exact half-turn boundary. The check verifies command math; it does not establish native physics, animated-pose, or injury parity.

Development uses a private C# reference executable built from the locally recovered methods and installed Unity managed math library, with engine state stubbed for healthy static limbs. The executable, recovered source, and game libraries are excluded from this repository; the ordinary tests above do not require it. With that private probe available, repeat the comparison without a window:

```powershell
cargo run -- --reference-test ".\reference-probe\bin\Debug\net9.0\ReferenceProbe.exe"
```

An optional `--smoke-test` opens a rendered window and checks the original texture, floor collisions, and joints over at least four seconds of simulation. Run that only when explicitly desired.
