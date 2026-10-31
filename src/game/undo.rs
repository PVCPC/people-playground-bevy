//! UndoControllerBehaviour with the IUndoableAction it keeps, and the text NotificationControllerBehaviour.Show
//! queues for the UI. ObjectCreationAction is the only action a rewritten tool registers (CatalogBehaviour.Spawn);
//! PasteLoadAction needs ObjectStateConverter (copy and paste), FreezeObjectAction is [Obsolete] and never created.
use super::Game;

/// ObjectCreationAction: the objects (by their GameObject's Transform id) its Undo destroys.
#[derive(Clone, Debug)]
pub struct ObjectCreationAction {
    pub displayName: String,
    pub RelevantObjects: Vec<i32>,
}

/// UndoControllerBehaviour: `history` and the static currentHistoryPosition (Awake sets it to -1).
#[derive(Clone, Debug)]
pub struct UndoController {
    pub history: Vec<ObjectCreationAction>,
    pub currentHistoryPosition: i32,
}

impl Default for UndoController {
    fn default() -> Self {
        Self { history: Vec::new(), currentHistoryPosition: -1 }
    }
}

impl Game {
    /// NotificationControllerBehaviour.Show: the text goes to the UI's notifications.
    pub fn notify(&mut self, text: String) {
        self.notifications.push(text);
    }

    /// UndoControllerBehaviour.RegisterAction: inserted after the current position (undone actions stay behind it).
    pub fn undo_register_action(&mut self, action: ObjectCreationAction) {
        let u = &mut self.undo;
        u.history.insert((u.currentHistoryPosition + 1) as usize, action);
        u.currentHistoryPosition += 1;
    }

    /// FindRelevantAction + DeregisterAction: the first action related to the object leaves the history, and the
    /// position steps back when it was at or before it.
    pub fn undo_deregister_related(&mut self, object: i32) {
        let u = &mut self.undo;
        let Some(i) = u.history.iter().position(|a| a.RelevantObjects.contains(&object)) else { return };
        if i as i32 <= u.currentHistoryPosition {
            u.currentHistoryPosition -= 1;
        }
        u.history.remove(i);
    }

    /// UndoControllerBehaviour.Undo: ObjectCreationAction.Undo destroys its objects and shows "<b>NAME</b> removed".
    pub fn undo(&mut self) {
        if self.undo.currentHistoryPosition == -1 {
            return;
        }
        let i = self.undo.currentHistoryPosition as usize;
        for o in self.undo.history[i].RelevantObjects.clone() {
            // A BloodWireBehaviour's component (Object.Destroy(component)), else a GameObject.
            if self.blood_wires.iter().any(|w| w.id == o) {
                self.tr_destroy_component(o, 0.0);
            } else {
                self.destroy_object(o);
            }
        }
        let name = self.undo.history[i].displayName.to_uppercase();
        self.undo.history[i].displayName = name.clone();
        self.notify(format!("<b>{name}</b> removed"));
        self.undo.currentHistoryPosition -= 1;
    }

    /// UndoControllerBehaviour.ClearHistory
    pub fn undo_clear_history(&mut self) {
        self.undo.currentHistoryPosition = -1;
        self.undo.history.clear();
    }

    /// UndoControllerBehaviour.Update (no pause menu, dialog or UI block is ever open).
    pub fn undo_controller_update(&mut self) {
        if self.input.up("undo") {
            self.undo();
        }
    }
}
