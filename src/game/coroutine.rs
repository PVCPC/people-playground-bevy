//! What the translated IEnumerator methods (tools/iltrans) run on: Unity's MonoBehaviour coroutines, riding the
//! queues of destroyable.rs (`Game::coroutines`, `end_of_frame`, `fixed_update_waits`: the DelayedCallManager's
//! callbacks by mode) with `Step::Routine`.
//!
//! UnityPlayer.dll (image base 0x180000000):
//! - MonoBehaviour::StartCoroutineManaged2 1808bc440: only on an activeInHierarchy GameObject (`disabled` is fine),
//!   then TryCreateAndRunCoroutine 1808bd030 runs the first MoveNext at once (Coroutine::Run 1808b2bc0).
//! - Coroutine::HandleIEnumerableCurrentReturnValue 1808b2230 -> CallDelayed 180632470 by what was yielded:
//!   WaitForSeconds / null: mode kRunDynamicFrameRate|kWaitForNextFrame (0xa), wake = dynamic time (TimeManager+0x60)
//!   + seconds, runs from frameCount + 1 on; WaitForFixedUpdate: mode 1, wake = fixed time (+0x30), any frame;
//!   WaitForEndOfFrame: mode 0x20, wake = current time (+0x90) - 1. An IEnumerator (WaitUntil and
//!   WaitForSecondsRealtime are CustomYieldInstructions, so those too) is a nested coroutine: its first step runs
//!   inside TryCreateAndRunCoroutine; if it already ended the parent waits one frame (mode 0xa, 0 s), else the
//!   parent has no callback until the child ends, and Coroutine::Run resumes it right there (1808b2d1b).
//! - DelayedCallManager::Update 180634dd0 (mode) runs the callbacks in wake order, ties in insertion order
//!   (180631c80 inserts after equal times), whose wake <= the current time, frame <= frameCount, mode matches and
//!   that were not added during this very pass. Callers: Update.ScriptRunDelayedDynamicFrameRate and
//!   PostLateUpdate.ScriptRunDelayedDynamicFrameRate (1807704e0, mode 2), FixedUpdate.ScriptRunDelayedFixedFrameRate
//!   (1807700c0, mode 1), TriggerEndOfFrameCallbacks (1807702a0, mode 0x20).
//! - MonoBehaviour::Deactivate 1808b7580 (GameObject deactivated or the component destroyed) stops every
//!   coroutine; `enabled = false` does not. StopCoroutine(string) 1808bca30 only matches coroutines started by name
//!   (CompareCoroutineMethodName 1808b1b90 is false for a null name); StopCoroutine releases 1808b77b0, which
//!   clears the behaviour, so a coroutine stopped inside its own step is not scheduled again (Run 1808b2d6c).
use super::destroyable::{Coroutine, Step};
use super::translated::rt::Go;
use super::Game;
use std::collections::BTreeMap;

/// One MoveNext of a translated IEnumerator.
pub type Routine = Box<dyn FnMut(&mut Game) -> Yield + Send + Sync>;

/// What a translated IEnumerator yields (the kinds the game uses: WaitForSeconds 60, WaitForEndOfFrame 34,
/// WaitUntil 18, WaitForFixedUpdate 7, WaitForSecondsRealtime 3, nested IEnumerator 3, null 1).
pub enum Yield {
    /// `yield return null`
    Null,
    WaitForSeconds(f32),
    WaitForFixedUpdate,
    WaitForEndOfFrame,
    WaitForSecondsRealtime(f32),
    /// `new WaitUntil(predicate)`
    WaitUntil(Box<dyn FnMut(&mut Game) -> bool + Send + Sync>),
    /// `yield return SomeEnumerator()`
    Routine(Routine),
    /// The end of the method (`yield break` or falling off).
    Done,
}

/// The Coroutine object StartCoroutine returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoroutineId(pub u64);

struct Co {
    owner: i32,
    routine: Option<Routine>,
    /// The coroutine that yielded this one, resumed when it ends.
    parent: Option<u64>,
    child: Option<u64>,
}

#[derive(Default)]
pub struct Coroutines {
    next: u64,
    cos: BTreeMap<u64, Co>,
}

/// Whether DelayedCallManager::Update's frame test lets `c` run in `frame` (only routines carry one).
pub fn frame_reached(c: &Coroutine, frame: i64) -> bool {
    !matches!(c.step, Step::Routine(_, from) if from > frame)
}

impl Game {
    /// MonoBehaviour.StartCoroutine(IEnumerator): `owner` is the component's instance id, `go` the id of a component
    /// of its GameObject. Nothing starts on an inactive GameObject (Unity logs an error).
    pub fn start_coroutine(&mut self, owner: i32, go: i32, r: Routine) -> Option<CoroutineId> {
        if !self.tr_active_in_hierarchy(Go::Local(go)) {
            return None;
        }
        Some(self.co_start(owner, r))
    }

    pub(super) fn co_start(&mut self, owner: i32, r: Routine) -> CoroutineId {
        let id = self.co_new(owner, r);
        self.co_run(id);
        CoroutineId(id)
    }

    /// MonoBehaviour.StopAllCoroutines, and what deactivating or destroying the GameObject does to each of its
    /// components.
    pub fn stop_all_coroutines(&mut self, owner: i32) {
        self.co.cos.retain(|_, c| c.owner != owner);
        self.drop_stale_waits();
    }

    fn drop_stale_waits(&mut self) {
        let live = &self.co.cos;
        let stale = |c: &Coroutine| matches!(c.step, Step::Routine(id, _) if !live.contains_key(&id));
        self.coroutines.retain(|c| !stale(c));
        self.end_of_frame.retain(|c| !stale(c));
        self.fixed_update_waits.retain(|c| !stale(c));
    }

    /// MonoBehaviour.StopCoroutine(Coroutine): it and the nested coroutines it waits for.
    pub fn stop_coroutine(&mut self, id: CoroutineId) {
        let mut at = Some(id.0);
        while let Some(i) = at {
            at = self.co.cos.remove(&i).and_then(|c| c.child);
        }
        self.drop_stale_waits();
    }

    /// MonoBehaviour.StopCoroutine(string): only coroutines started with StartCoroutine(string) match, and the game
    /// starts none that way.
    pub fn stop_coroutine_by_name(&mut self, _owner: i32, _name: &str) {}

    fn co_new(&mut self, owner: i32, r: Routine) -> u64 {
        self.co.next += 1;
        let id = self.co.next;
        self.co.cos.insert(id, Co { owner, routine: Some(r), parent: None, child: None });
        id
    }

    /// Coroutine::Run: one MoveNext, then what it yielded.
    pub(super) fn co_run(&mut self, id: u64) {
        let Some(mut routine) = self.co.cos.get_mut(&id).and_then(|c| c.routine.take()) else { return };
        let y = routine(self);
        // Stopped during its own step: its yield is dropped.
        let Some(c) = self.co.cos.get_mut(&id) else { return };
        c.routine = Some(routine);
        match y {
            Yield::Done => {
                let c = self.co.cos.remove(&id).unwrap();
                if let Some(p) = c.parent.filter(|p| self.co.cos.contains_key(p)) {
                    self.co.cos.get_mut(&p).unwrap().child = None;
                    self.co_run(p);
                }
            }
            Yield::Null => self.co_wait_dynamic(id, 0.0),
            Yield::WaitForSeconds(s) => self.co_wait_dynamic(id, s),
            Yield::WaitForFixedUpdate => {
                let (seq, at, owner) = self.co_entry(id);
                self.fixed_update_waits.push(Coroutine { seq, at, owner, step: Step::Routine(id, i64::MIN) });
            }
            Yield::WaitForEndOfFrame => {
                let (seq, at, owner) = self.co_entry(id);
                self.end_of_frame.push(Coroutine { seq, at, owner, step: Step::Routine(id, i64::MIN) });
            }
            Yield::WaitUntil(mut keep) => {
                // CustomYieldInstruction.MoveNext is keepWaiting, Current is null.
                self.co_nested(id, Box::new(move |g| if keep(g) { Yield::Done } else { Yield::Null }));
            }
            Yield::WaitForSecondsRealtime(s) => {
                let mut until = -1.0f32;
                self.co_nested(
                    id,
                    Box::new(move |g| {
                        let now = g.time.realtimeSinceStartup as f32;
                        if until < 0.0 {
                            until = now + s;
                        }
                        if now < until {
                            Yield::Null
                        } else {
                            Yield::Done
                        }
                    }),
                );
            }
            Yield::Routine(r) => self.co_nested(id, r),
        }
    }

    /// The nested coroutine of `parent`: run now; if it is over, the parent waits a frame, else it resumes when the
    /// child ends.
    fn co_nested(&mut self, parent: u64, r: Routine) {
        let owner = self.co.cos[&parent].owner;
        let child = self.co_new(owner, r);
        self.co_run(child);
        if self.co.cos.contains_key(&child) {
            self.co.cos.get_mut(&child).unwrap().parent = Some(parent);
            if let Some(p) = self.co.cos.get_mut(&parent) {
                p.child = Some(child);
            }
        } else if self.co.cos.contains_key(&parent) {
            self.co_wait_dynamic(parent, 0.0);
        }
    }

    fn co_entry(&mut self, id: u64) -> (u64, f64, i32) {
        self.coroutine_seq += 1;
        (self.coroutine_seq, self.time.timeAsDouble, self.co.cos[&id].owner)
    }

    /// CallDelayed(mode 0xa): the frame's time + seconds, from the next frame on.
    fn co_wait_dynamic(&mut self, id: u64, seconds: f32) {
        let (seq, _, owner) = self.co_entry(id);
        let at = self.time.dynamicTimeAsDouble + seconds as f64;
        let from = self.time.frameCount as i64 + 1;
        self.coroutines.push(Coroutine { seq, at, owner, step: Step::Routine(id, from) });
    }

    /// PostLateUpdate.ScriptRunDelayedDynamicFrameRate, which only the routines take part in (the queue's older
    /// users have no second pass). Nothing is due: every dynamic wait made this frame starts at the next.
    pub fn routines_dynamic_pass(&mut self) {
        let (now, frame) = (self.time.timeAsDouble, self.time.frameCount as i64);
        let mut due: Vec<(f64, u64)> =
            self.coroutines.iter().filter(|c| matches!(c.step, Step::Routine(..)) && c.at <= now && frame_reached(c, frame)).map(|c| (c.at, c.seq)).collect();
        due.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (_, seq) in due {
            if let Some(i) = self.coroutines.iter().position(|c| c.seq == seq) {
                let c = self.coroutines.remove(i);
                self.resume_coroutine(c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::Arc;

    type Log = Arc<Mutex<Vec<String>>>;

    /// A routine that logs `name` + step number, yielding `ys` in order, then ends.
    fn routine(log: &Log, name: &str, mut ys: Vec<fn() -> Yield>) -> Routine {
        let (log, name) = (log.clone(), name.to_string());
        ys.reverse();
        let mut n = 0;
        Box::new(move |_| {
            log.lock().unwrap().push(format!("{name}{n}"));
            n += 1;
            ys.pop().map_or(Yield::Done, |y| y())
        })
    }

    fn take(log: &Log) -> String {
        std::mem::take(&mut *log.lock().unwrap()).join(" ")
    }

    /// One player-loop frame of `dt` with `fixed` FixedUpdate steps of 0.02: the points that matter to coroutines.
    /// `update` runs in the Update scripts, `fixed_script` in each FixedUpdate.
    fn frame(g: &mut Game, dt: f64, fixed: usize, mut fixed_script: impl FnMut(&mut Game), update: impl FnOnce(&mut Game)) {
        g.time.frameCount += 1;
        g.time.dynamicTimeAsDouble += dt;
        for _ in 0..fixed {
            g.time.fixedTimeAsDouble += 0.02;
            g.time.timeAsDouble = g.time.fixedTimeAsDouble;
            fixed_script(g);
            for c in std::mem::take(&mut g.fixed_update_waits) {
                g.resume_coroutine(c);
            }
        }
        g.time.timeAsDouble = g.time.dynamicTimeAsDouble;
        update(g);
        g.gore_string_delayed_calls(true);
        g.end_of_frame_calls();
    }

    fn nop(_: &mut Game) {}

    #[test]
    fn first_step_runs_inside_start_and_null_waits_a_frame() {
        let (mut g, log) = (Game::default(), Log::default());
        let l = log.clone();
        // Started in the Update scripts, before the frame's own dynamic pass: it still waits for the next frame.
        frame(&mut g, 0.01, 0, nop, |g| {
            g.co_start(1, routine(&l, "a", vec![|| Yield::Null, || Yield::Null]));
        });
        assert_eq!(take(&log), "a0");
        frame(&mut g, 0.01, 0, nop, nop);
        assert_eq!(take(&log), "a1");
        frame(&mut g, 0.01, 0, nop, nop);
        assert_eq!(take(&log), "a2");
        frame(&mut g, 0.01, 0, nop, nop);
        assert_eq!(take(&log), "");
    }

    #[test]
    fn started_in_update_waits_for_next_frames_pass_and_order_is_wake_then_start() {
        let (mut g, log) = (Game::default(), Log::default());
        g.time.dynamicTimeAsDouble = 1.0;
        g.time.timeAsDouble = 1.0;
        let l = log.clone();
        frame(&mut g, 0.0, 0, nop, |g| {
            // Wakes at 1.5, 1.2, 1.2 (c after b), and a yield null (wake 1.0, so first).
            g.co_start(1, routine(&l, "a", vec![|| Yield::WaitForSeconds(0.5)]));
            g.co_start(1, routine(&l, "b", vec![|| Yield::WaitForSeconds(0.2)]));
            g.co_start(2, routine(&l, "c", vec![|| Yield::WaitForSeconds(0.2)]));
            g.co_start(2, routine(&l, "n", vec![|| Yield::Null]));
        });
        assert_eq!(take(&log), "a0 b0 c0 n0");
        frame(&mut g, 0.3, 0, nop, nop);
        assert_eq!(take(&log), "n1 b1 c1");
        frame(&mut g, 0.3, 0, nop, nop);
        assert_eq!(take(&log), "a1");
    }

    #[test]
    fn wait_for_seconds_wakes_when_time_reaches_it_exactly() {
        let (mut g, log) = (Game::default(), Log::default());
        g.co_start(1, routine(&log, "a", vec![|| Yield::WaitForSeconds(0.5)]));
        take(&log);
        frame(&mut g, 0.25, 0, nop, nop);
        frame(&mut g, 0.2499, 0, nop, nop);
        assert_eq!(take(&log), "");
        frame(&mut g, 0.0001, 0, nop, nop);
        assert_eq!(take(&log), "a1");
    }

    #[test]
    fn fixed_update_wait_ends_the_step_it_was_started_in_and_rewaits_for_the_next() {
        let (mut g, log) = (Game::default(), Log::default());
        let l = log.clone();
        let mut started = false;
        frame(
            &mut g,
            0.04,
            2,
            |g| {
                if !started {
                    started = true;
                    g.co_start(1, routine(&l, "f", vec![|| Yield::WaitForFixedUpdate, || Yield::WaitForFixedUpdate]));
                    l.lock().unwrap().push("script".into());
                }
            },
            nop,
        );
        // Step 1: script, first step, then its pass resumes it (once); step 2: resumes again.
        assert_eq!(take(&log), "f0 script f1 f2");
    }

    #[test]
    fn end_of_frame_runs_after_update_and_rewaits_for_the_next_frame() {
        let (mut g, log) = (Game::default(), Log::default());
        let l = log.clone();
        frame(
            &mut g,
            0.02,
            0,
            nop,
            |g| {
                g.co_start(1, routine(&l, "e", vec![|| Yield::WaitForEndOfFrame, || Yield::WaitForEndOfFrame]));
                g.co_start(1, routine(&l, "n", vec![|| Yield::Null]));
            },
        );
        assert_eq!(take(&log), "e0 n0 e1");
        frame(&mut g, 0.02, 0, nop, nop);
        assert_eq!(take(&log), "n1 e2");
    }

    #[test]
    fn wait_until_and_nested_routines() {
        let (mut g, log) = (Game::default(), Log::default());
        let flag = Arc::new(Mutex::new(false));
        let f = flag.clone();
        let l = log.clone();
        let mut step = 0;
        g.co_start(
            1,
            Box::new(move |_| {
                step += 1;
                l.lock().unwrap().push(format!("p{step}"));
                let f = f.clone();
                match step {
                    1 => Yield::WaitUntil(Box::new(move |_| *f.lock().unwrap())),
                    _ => Yield::Done,
                }
            }),
        );
        assert_eq!(take(&log), "p1");
        frame(&mut g, 0.01, 0, nop, nop);
        *flag.lock().unwrap() = true;
        // Resumed inside the pass that sees the predicate hold, not a frame later.
        frame(&mut g, 0.01, 0, nop, nop);
        assert_eq!(take(&log), "p2");

        // A predicate already true: the parent waits a frame like yield null.
        let l = log.clone();
        frame(&mut g, 0.01, 0, nop, |g| {
            let mut step = 0;
            g.co_start(
                1,
                Box::new(move |_| {
                    step += 1;
                    l.lock().unwrap().push(format!("q{step}"));
                    if step == 1 { Yield::WaitUntil(Box::new(|_| true)) } else { Yield::Done }
                }),
            );
        });
        assert_eq!(take(&log), "q1");
        frame(&mut g, 0.01, 0, nop, nop);
        assert_eq!(take(&log), "q2");

        // A nested routine that waits: the parent continues when it ends.
        let l = log.clone();
        let child = routine(&log, "c", vec![|| Yield::WaitForSeconds(0.05)]);
        let mut child = Some(child);
        let mut step = 0;
        g.co_start(
            1,
            Box::new(move |_| {
                step += 1;
                l.lock().unwrap().push(format!("r{step}"));
                if step == 1 { Yield::Routine(child.take().unwrap()) } else { Yield::Done }
            }),
        );
        assert_eq!(take(&log), "r1 c0");
        frame(&mut g, 0.1, 0, nop, nop);
        assert_eq!(take(&log), "c1 r2");
    }

    #[test]
    fn stopping_removes_pending_waits_and_a_stop_inside_its_own_step_does_not_reschedule() {
        let (mut g, log) = (Game::default(), Log::default());
        g.co_start(1, routine(&log, "a", vec![|| Yield::WaitForEndOfFrame, || Yield::Null]));
        g.co_start(1, routine(&log, "f", vec![|| Yield::WaitForFixedUpdate]));
        g.co_start(1, routine(&log, "s", vec![|| Yield::WaitForSeconds(1.0)]));
        let kept = g.co_start(2, routine(&log, "k", vec![|| Yield::Null, || Yield::Null]));
        take(&log);
        g.stop_all_coroutines(1);
        frame(&mut g, 2.0, 1, nop, nop);
        assert_eq!(take(&log), "k1");
        // Disabling the behaviour has no hook here: only StopAllCoroutines / deactivation / destroy stop.
        let l = log.clone();
        let mut first = true;
        g.co_start(
            3,
            Box::new(move |g| {
                l.lock().unwrap().push("self".into());
                if first {
                    first = false;
                    g.stop_all_coroutines(3);
                    return Yield::Null;
                }
                Yield::Done
            }),
        );
        frame(&mut g, 0.02, 0, nop, nop);
        frame(&mut g, 0.02, 0, nop, nop);
        assert_eq!(take(&log), "self k2");
        g.stop_coroutine(kept);
        assert!(g.co.cos.is_empty());
    }
}
