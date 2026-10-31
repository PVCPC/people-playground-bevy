# People Playground / Bevy

An early Windows Rust/Bevy ragdoll sandbox prototype. **This is not a complete or verified 1:1 rewrite.**

Requires your own installed copy of People Playground. Original assets, extracted caches, game DLLs, and recovered C# are not included in this repository.

## Current behavior

- The game itself rewritten from the installed copy: humans and androids, limbs, circulation, items, maps and the menus.
- Installation and cache-fingerprint checks at startup.
- Map tests with no window or desktop control.

Bevy 0.20 handles rendering. Physics is a Rust rewrite of the Box2D build inside the game's UnityPlayer.dll (src/b2): broad phase, contacts, island solve, continuous (TOI) solve and hinge joints, each compared bit for bit against the DLL by a private probe. Muscles set HingeJoint2D motor fields and Rigidbody2D forces the way the native setters do. Fresh caches import the fixed timestep, gravity, velocity/position iteration counts and correction limits from the exported project settings (this installation: 50 Hz, 16/16 iterations, no substeps). Full trajectories have not yet been compared against the running game.

Fresh caches resolve each human body's runtime physics material through its physical-properties asset, replacing the prototype coefficients. Imported contacts use geometric-mean friction and maximum restitution mixing. Different materials on colliders within one compound body are rejected rather than silently merged. Native Unity motor-property limits are also applied before storing the speed for the next command: speed is bounded to ±1,000,000 degrees/second and torque to 0–1,000,000.

Joint signs follow the recovered native body convention: body A is the attached body, body B is the connected body, and the joint angle is B minus A minus the reference angle. Fresh caches preserve original pose and limit degrees and convert them using single-precision arithmetic at the physics boundary. Imported pose angles and limits retain their original signs. This corrects the prototype's mirrored nonzero poses.

Anything not rewritten yet stops at `unimplemented!()`; nothing silently stands in for it.

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

Legacy caches still load with the previous radian targets and default simulation settings; prepare a fresh cache to preserve original degrees and import project settings.

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

## Verify without opening a window

```powershell
python scripts/test_prepare_assets.py
cargo test
cargo run -- --play-map-test Default --item "Brick Cube" --target Human
```

The map test requires a local game installation and prepared cache. It spawns the item and the target on the map and runs the rewritten game without a window; it exits non-zero on any `unimplemented!` path. Unit tests create no window or renderer.

`src/motor.rs` implements the healthy, static-pose motor-command calculations used by the live simulation, retaining the original degree units until conversion at the physics boundary. A local comparison against extracted C# methods passed 2,052 deterministic cases with identical float bits for speed and torque, including the exact half-turn boundary. The check verifies command math; it does not establish native physics, animated-pose, or injury parity.

Development uses a private C# reference executable built from the locally recovered methods and installed Unity managed math library, with engine state stubbed for healthy static limbs. The executable, recovered source, and game libraries are excluded from this repository; the ordinary tests above do not require it. With that private probe available, repeat the comparison without a window:

```powershell
cargo run -- --reference-test ".\reference-probe\bin\Debug\net9.0\ReferenceProbe.exe"
```

`src/hinge.rs` has a separate comparison against the installed DLL's actual native `b2RevoluteJoint::SolveVelocityConstraints` machine code. All 4,096 prepared one-iteration cases match every float bit in both body velocities, the point/limit impulse, and the motor impulse, including active/equal/released limits, fixed rotation, motor disabling, and axis masks. Another 1,024 cases call the native `InitVelocityConstraints` and Unity `HingeJoint2D::GetJointAngle` functions and compare rotated anchors, the prepared matrix and motor mass, limit state, timestep scaling, warm/cold-start output bits, and signed joint angles including nonzero reference angles. The private probe verifies the DLL's SHA-256 before mapping it, does not run Unity initialization, and calls only the verified allocation-free arithmetic in its own process. It exits after returning results; no Ghidra server or game window is needed. A further 4,096 cases call native `SolvePositionConstraints` and compare every corrected position/angle bit and convergence flag across limits, axis masks, rotated anchors, center-of-mass offsets, and angular correction bounds. Another 4,096 cases compare the actual native velocity/position integration tasks, including dynamic and kinematic/static inputs, gravity, force, torque, damping, locked axes, displacement caps, and scalar/unrolled paths. Every compared position and velocity bit matches. The comparison does not cover whole-frame adapter behavior, force accumulation across API calls, or contacts.

```powershell
cargo run -- --native-reference-test ".\reference-probe\native\bin\Debug\net9.0\NativeProbe.exe"
```
