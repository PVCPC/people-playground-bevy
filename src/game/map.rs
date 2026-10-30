//! The map's scripts that share UnityEngine.Random with the human: PlayAudioAtRandom (the ambience
//! emitter) and MapLightBehaviour (the lights switching on after the map loads).
use super::{script, Bounds, Game};
use crate::assets::db::{Component, ObjRef, Prefab};
use std::collections::HashMap;
use std::sync::Arc as Rc;

script! {
    PlayAudioAtRandom {
        ChancePerSecond: f32 = 0.5,
        ChooseRandomClip: bool = false,
        PlaceInRandomPositionInCircle: bool = false,
        OnlyOnEdge: bool = false,
        PositionCenter: [f32; 2] = [0.0, 0.0],
        Radius: f32 = 5000.0,
        t: f32 = 0.0,
    }
    links {
        /// Clips.Length; None when Clips is null.
        clips: Option<usize> = None,
        /// AudioSource.isPlaying, kept by the audio layer.
        isPlaying: bool = false,
        /// AudioSource.transform.localPosition (xy).
        sourcePosition: [f32; 2] = [0.0, 0.0],
        /// The clip index the audio layer plays next; None plays AudioSource.clip.
        play: Option<Option<usize>> = None,
    }
}

script! {
    MapLight {
        TimeIntervalInSeconds: f32 = 0.7,
        Order: f32 = 0.0,
        Activated: bool = false,
    }
    links {
        /// Switches.Length; None when Switches is null.
        switches: Option<usize> = None,
        hasAudio: bool = false,
        /// The pending DelayedActivation's WaitForSeconds: Time.timeAsDouble it resumes at.
        wake: Option<f64> = None,
        /// The switch clip the audio layer plays next.
        play: Option<Option<usize>> = None,
    }
}

/// What MapConfig.Start reads: its BoundingBoxBehaviour's box, the parent's PlayAudioAtRandom, the
/// OverrideAmbientSounds ambience (Chance, Clips.Length), its EnvironmentalSettings, and the weather prefabs
/// (Snow, Rain, Fog; LightningCreator) with the original instance ids Instantiate orders their clones by.
#[derive(Clone, Debug)]
pub struct MapConfigStart {
    pub bounds: Bounds,
    pub audio: Option<usize>,
    pub ambience: Option<(f32, usize)>,
    pub gravity: f32,
    pub ambient_temperature: f32,
    pub lightning_chance: f32,
    pub weather: Vec<Option<Rc<Prefab>>>,
    pub lightning: Option<Rc<Prefab>>,
    pub original_ids: Rc<HashMap<ObjRef, i32>>,
}

impl PartialEq for MapConfigStart {
    /// The same MapConfig's Start: the shared prefabs compare by identity.
    fn eq(&self, o: &Self) -> bool {
        let same = |a: &Option<Rc<Prefab>>, b: &Option<Rc<Prefab>>| match (a, b) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (a, b) => a.is_none() && b.is_none(),
        };
        (self.bounds, self.audio, self.ambience, self.gravity, self.ambient_temperature, self.lightning_chance) == (o.bounds, o.audio, o.ambience, o.gravity, o.ambient_temperature, o.lightning_chance)
            && self.weather.len() == o.weather.len()
            && self.weather.iter().zip(&o.weather).all(|(a, b)| same(a, b))
            && same(&self.lightning, &o.lightning)
            && Rc::ptr_eq(&self.original_ids, &o.original_ids)
    }
}

impl Game {
    /// MapLoaderBehaviour.InstantiateMap: Object.Instantiate(map.Prefab, Vector3.zero, Quaternion.identity,
    /// WORLD.transform). WORLD sits at the origin unscaled, and its PlayAudioAtRandom is `ambient_audio[0]`
    /// (`load_main`). The Starts are queued.
    pub fn load_map(&mut self, db: &mut crate::assets::db::AssetDb, prefab: &crate::assets::db::Prefab) -> Result<(super::spawn::Instance, super::link::Linked), String> {
        if self.ambient_audio.len() != 1 {
            return Err("load_map before load_main".into());
        }
        self.spawn(db, prefab, [0.0; 3], [0.0, 0.0, 0.0, 1.0], Some(0))
    }

    /// MapLightBehaviour.Start: Activated = StartEnabled (Global.Awake clears it), UpdateActivated(playSound:
    /// true) on the off branch (rendering only), then DelayedActivation(4) up to its WaitForSeconds, whose
    /// DelayedCallManager entry is the current time plus the float seconds.
    pub fn map_light_start(&mut self, l: usize) {
        self.map_lights[l].Activated = false;
        let m = &self.map_lights[l];
        let (interval, order) = (m.TimeIntervalInSeconds, m.Order);
        let seconds = interval * self.random.range(1.0, 1.1) * (order + 4.0);
        self.map_lights[l].wake = Some(self.time.timeAsDouble + seconds as f64);
    }

    /// MapConfig.Start: Reinitialise, then ApplySettings(Settings): SetGravity, SetFloodLights (Settings is the
    /// same object: no change) and PhysicalBehaviour.AmbientTemperature. The overlay fades, the heat/cold
    /// post-processing and the foliage tint are rendering.
    pub fn map_config_start(&mut self, m: &MapConfigStart) {
        let (b, z) = (m.bounds, 0.0);
        let size = [b.extents[0] * 2.0, b.extents[1] * 2.0];
        // PrepareWeatherInstance: Instantiate(prefab, MapConfig.transform) (the map root sits at the origin),
        // then position (center, 20), localScale (size, 1) and SetActive(false). Only the clones' instance ids
        // and their scripts reach the simulation; the overlays are rendering, so their Instances are dropped.
        for w in m.weather.iter().flatten() {
            let (mut inst, behaviours) = self.instantiate_native(w, [b.center[0], b.center[1], 20.0], root_rotation(w), &m.original_ids);
            weather_awakes(w, &behaviours);
            inst.set_local_scale(0, [size[0], size[1], 1.0]);
            // Their OnDisables (AudioSourceTimeScaleBehaviour has none, PostProcessVolume's is rendering).
            self.set_active(&mut inst, 0, false);
        }
        // PrepareLightningPrefab: the clone at (min.x, max.y, 0) stays active; ApplySettings sets its Chance.
        if let Some(w) = &m.lightning {
            let (_, behaviours) = self.instantiate_native(w, [b.center[0] - b.extents[0], b.center[1] + b.extents[1], z], root_rotation(w), &m.original_ids);
            weather_awakes(w, &behaviours);
            if !super::mathf::approximately(m.lightning_chance, 0.0) {
                unimplemented!("WeatherLightningBehaviour.Update with Lightning_chance {}", m.lightning_chance);
            }
        }
        if let Some(a) = m.audio {
            let s = &mut self.ambient_audio[a];
            s.PositionCenter = m.bounds.center;
            s.Radius = super::mathf::max(m.bounds.extents[0], m.bounds.extents[1]) * 1.41421;
            if let Some((chance, clips)) = m.ambience {
                s.ChancePerSecond = chance;
                s.clips = Some(clips);
            }
        }
        // Physics2D.gravity = Gravity * Vector2.up.
        self.world.gravity = crate::b2::math::Vec2::new(0.0 * m.gravity, 1.0 * m.gravity);
        // WaterBehaviour.waters is empty: a WaterBehaviour does not link (awake_behaviours).
        self.AmbientTemperature = m.ambient_temperature;
    }

    /// PlayAudioAtRandom.Update
    pub fn play_audio_at_random_update(&mut self, a: usize) {
        let dt = self.time.deltaTime;
        let s = &mut self.ambient_audio[a];
        if s.ChooseRandomClip && s.clips.map_or(true, |n| n == 0) {
            return;
        }
        s.t += dt;
        if !(s.t > 1.0) {
            return;
        }
        s.t = 0.0;
        let chance = s.ChancePerSecond;
        if self.random.value() <= chance && !self.ambient_audio[a].isPlaying {
            let s = &self.ambient_audio[a];
            if s.PlaceInRandomPositionInCircle {
                let c = self.random.inside_unit_circle();
                let s = &self.ambient_audio[a];
                let c = if s.OnlyOnEdge { super::mathf::normalized2(c) } else { c };
                self.ambient_audio[a].sourcePosition = [s.PositionCenter[0] + c[0] * s.Radius, s.PositionCenter[1] + c[1] * s.Radius];
            }
            let s = &self.ambient_audio[a];
            let clip = if s.ChooseRandomClip { self.pick_random(s.clips.unwrap_or(0)) } else { None };
            self.ambient_audio[a].play = Some(clip);
        }
    }

    /// The pending MapLightBehaviour coroutines (DelayedCallManager, between Update and LateUpdate):
    /// each resumes in the first frame whose time reaches its wake, in wake order.
    pub fn map_light_coroutines(&mut self) {
        let now = self.time.timeAsDouble;
        let mut due: Vec<usize> = (0..self.map_lights.len()).filter(|&l| self.map_lights[l].wake.is_some_and(|w| w <= now)).collect();
        due.sort_by(|&a, &b| self.map_lights[a].wake.partial_cmp(&self.map_lights[b].wake).unwrap());
        for l in due {
            // DelayedActivation after its WaitForSeconds.
            let m = &mut self.map_lights[l];
            m.wake = None;
            m.Activated = true;
            // UpdateActivated(playSound: true). ponytail: OnActivate listeners and OnGroup's scripts are
            // rendering only on the shipped maps (one Random draw per wake, per the trace).
            if m.hasAudio {
                let n = m.switches;
                let clip = self.pick_random(n.unwrap_or(0));
                self.map_lights[l].play = Some(clip);
            }
        }
    }
}

/// A prefab root's local rotation (Instantiate under an unrotated parent keeps it).
fn root_rotation(p: &Prefab) -> [f32; 4] {
    p.objects[0].components.iter().find_map(|(_, c)| match c {
        Component::Transform(t) => Some(t.local_rotation),
        _ => None,
    }).unwrap()
}

/// The weather clones' Awakes: AudioSourceTimeScaleBehaviour has none; PostProcessVolume's and
/// WeatherLightningBehaviour's (layer mask, vertex buffer, LightSprite off) are rendering.
fn weather_awakes(p: &Prefab, behaviours: &[(usize, usize)]) {
    for &(o, c) in behaviours {
        let Component::Script { class, .. } = &p.objects[o].components[c].1 else { unreachable!() };
        if !matches!(class.as_str(), "AudioSourceTimeScaleBehaviour" | "PostProcessVolume" | "WeatherLightningBehaviour") {
            unimplemented!("{class}.Awake in a weather prefab");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::db::AssetDb;
    use crate::assets::mono::Value;
    use std::path::Path;

    /// The Default map's scripts after their Starts against dump34map's first snapshot (made with the same
    /// config.json). The lights' wake times depend on the scene's Random state and are left out, and so is PlayAudioAtRandom.t (Update has run by the snapshot).
    #[test]
    fn map_scripts_match_recording() {
        let Ok(game) = crate::game_assets::find_game(None) else { return };
        let dump = Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-probe/game/dump34map.txt");
        let Ok(mut snaps) = super::super::dump::snapshots(&dump) else { return };
        let first = snaps.next().unwrap();
        let mut db = AssetDb::open(&game).unwrap();
        let mut map = None;
        for m in db.find_scripts("Map").unwrap() {
            let (_, _, v) = db.mono(m).unwrap().unwrap();
            let p = v.get("Prefab").and_then(Value::pptr).and_then(|p| db.resolve(m.file, p)).unwrap();
            if db.game_object(p).unwrap().name == "Default map" {
                map = Some(p);
            }
        }
        let prefab = db.prefab(map.unwrap()).unwrap();
        let mut g = Game::load_main(&mut db, &game).unwrap();
        g.world = crate::b2::world::World::new(crate::b2::math::Vec2::new(-0.0, -9.81));
        g.load_map(&mut db, &prefab).unwrap();
        g.run_pending_starts();
        let snap = |k: &str| first.snap.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone()).unwrap();
        assert_eq!(format!("{:08x}", g.AmbientTemperature.to_bits()), snap("ambient"));
        let b = g.BoundingBox;
        assert_eq!([b.center[0], b.center[1], b.extents[0], b.extents[1]].map(|x| format!("{:08x}", x.to_bits())).join(","), snap("bounds"));
        assert_eq!((g.world.gravity.x.to_bits(), g.world.gravity.y.to_bits()), ((-0.0f32).to_bits(), (-9.81f32).to_bits()));
        let out: Vec<String> = super::super::dump::compare(&mut g, &first).into_iter().filter(|l| (l.starts_with("MapLight") || l.starts_with("PlayAudioAtRandom")) && !l.contains(".wake:") && !l.contains(".t:")).collect();
        assert!(out.is_empty(), "{} mismatches:\n{}", out.len(), out.join("\n"));
        assert_eq!(g.map_lights.len(), first.of("MapLightBehaviour").count());
    }
}

