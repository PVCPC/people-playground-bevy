//! The map's scripts that share UnityEngine.Random with the human: PlayAudioAtRandom (the ambience
//! emitter) and MapLightBehaviour (the lights switching on after the map loads).
use super::{script, Game};

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

impl Game {
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
