//! SoundOnShakeBehaviour (the bushes' and trees' branches): FixedUpdate compares the Rigidbody2D's velocity with the last
//! step's and, while its AudioSource is silent, a hard enough shake draws Random.value and plays one of Clips. The
//! sound is presentation, but AudioSource.isPlaying gates the draw, so the end of each one-shot is kept.
#![allow(non_snake_case)]

use super::Game;

#[derive(Clone, Debug, Default)]
pub struct SoundOnShake {
    /// The component's instance id.
    pub id: i32,
    /// rb: the instance id of GetComponent<Rigidbody2D>() in Awake.
    pub rb: i32,
    /// AudioSource: its component id.
    pub AudioSource: i32,
    /// AudioClip.length of each of Clips.
    pub Clips: Vec<f32>,
    pub ShakeThreshold: f32,
    pub previousVelocity: [f32; 2],
    /// The clone root's Transform id, and whether its AudioSources follow Time.timeScale (Global.AddAudioSource from
    /// an AudioSourceTimeScaleBehaviour above them).
    pub root: i32,
    pub time_scaled: bool,
}

impl Game {
    /// AudioSource.isPlaying: a one-shot is still sounding.
    // ponytail: the mixer runs on the DSP clock; with pitch = Time.timeScale a clip lasts its length in scaled time,
    // which Time.fixedTime only matches while frames keep up with the wall clock (the game's own isPlaying drifts too).
    pub(super) fn audio_is_playing(&self, source: i32) -> bool {
        self.audio_playing_until.get(&source).is_some_and(|&t| self.time.fixedTimeAsDouble < t)
    }

    /// SoundOnShakeBehaviour.FixedUpdate
    pub fn sound_on_shake_fixed_update(&mut self, s: usize) {
        let sh = &self.sound_on_shakes[s];
        let velocity = self.rigidbody_velocity_by_id(sh.rb);
        if self.audio_is_playing(sh.AudioSource) {
            self.sound_on_shakes[s].previousVelocity = velocity;
            return;
        }
        let (dx, dy) = (sh.previousVelocity[0] - velocity[0], sh.previousVelocity[1] - velocity[1]);
        let sqr_magnitude = dx * dx + dy * dy;
        let num = sh.ShakeThreshold * sh.ShakeThreshold;
        self.sound_on_shakes[s].previousVelocity = velocity;
        if sqr_magnitude > num && self.random.value() > 0.5 {
            let sh = &self.sound_on_shakes[s];
            if !sh.time_scaled {
                unimplemented!("SoundOnShakeBehaviour: an AudioSource outside AudioSourceTimeScaleBehaviour (its pitch, real time)");
            }
            let (source, n) = (sh.AudioSource, sh.Clips.len());
            let k = self.pick_random(n).unwrap_or_else(|| unimplemented!("Clips.PickRandom() of an empty array (IndexOutOfRangeException)"));
            let until = self.time.fixedTimeAsDouble + self.sound_on_shakes[s].Clips[k] as f64;
            let end = self.audio_playing_until.entry(source).or_insert(until);
            *end = end.max(until);
        }
    }
}
