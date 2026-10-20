//! Live editing state for one factory: the document, its solved plan, undo
//! history and autosave.

use planner_core::factory::now_iso;
use planner_core::solve::solve_factory;
use planner_core::store::Store;
use planner_core::{Factory, GameData, GameSettings, Plan};
use std::sync::Arc;
use std::time::{Duration, Instant};

const HISTORY: usize = 200;
const SAVE_DELAY: Duration = Duration::from_millis(400);

/// A change to the factory, computed from the current document and plan.
pub type Edit = Box<dyn FnOnce(&Factory, &Plan) -> Factory>;

pub struct Editor {
    pub factory: Factory,
    pub plan: Plan,
    pub data: Arc<GameData>,
    settings: GameSettings,
    past: Vec<Factory>,
    future: Vec<Factory>,
    last_merge: Option<String>,
    dirty_at: Option<Instant>,
    pub save_error: Option<String>,
    pub solve_ms: f64,
}

impl Editor {
    pub fn new(factory: Factory, data: Arc<GameData>, settings: GameSettings) -> Editor {
        let mut ed = Editor {
            plan: Plan::empty(planner_core::PlanStatus::Ok, None, vec![]),
            factory,
            data,
            settings,
            past: vec![],
            future: vec![],
            last_merge: None,
            dirty_at: None,
            save_error: None,
            solve_ms: 0.0,
        };
        ed.resolve();
        ed
    }

    fn resolve(&mut self) {
        let t0 = Instant::now();
        self.plan = solve_factory(&self.data, &self.factory, &self.settings);
        self.solve_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    pub fn set_settings(&mut self, settings: GameSettings) {
        self.settings = settings;
        self.resolve();
    }

    /// Apply an edit; `merge` folds rapid edits of one field (typing,
    /// dragging) into a single undo step.
    pub fn apply(&mut self, edit: Edit, merge: Option<String>) {
        let mut next = edit(&self.factory, &self.plan);
        if next == self.factory {
            return;
        }
        if merge.is_none() || merge != self.last_merge {
            self.past.push(self.factory.clone());
            if self.past.len() > HISTORY {
                self.past.remove(0);
            }
        }
        self.last_merge = merge;
        self.future.clear();
        next.updated_at = now_iso();
        self.factory = next;
        self.changed();
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.past.pop() {
            self.future.push(std::mem::replace(&mut self.factory, prev));
            self.last_merge = None;
            self.changed();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.future.pop() {
            self.past.push(std::mem::replace(&mut self.factory, next));
            self.last_merge = None;
            self.changed();
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    fn changed(&mut self) {
        self.resolve();
        self.dirty_at = Some(Instant::now());
    }

    pub fn status(&self) -> &'static str {
        match (&self.save_error, self.dirty_at) {
            (Some(_), _) => "save failed",
            (None, Some(_)) => "saving…",
            (None, None) => "saved",
        }
    }

    /// Save if the last edit is older than the debounce delay (or `force`).
    /// Returns how long until the next check is due, if one is.
    pub fn autosave(&mut self, store: &Store, force: bool) -> Option<Duration> {
        let at = self.dirty_at?;
        let waited = at.elapsed();
        if !force && waited < SAVE_DELAY {
            return Some(SAVE_DELAY - waited);
        }
        match store.save_factory(&self.factory) {
            Ok(()) => {
                self.dirty_at = None;
                self.save_error = None;
            }
            Err(e) => {
                self.save_error = Some(format!("{e:#}"));
                self.dirty_at = None;
            }
        }
        None
    }
}
