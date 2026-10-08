//! Transient card playback: never changes a saved face or editor Undo history.
use crate::domain::theme::{ElementId, Theme};
use std::{borrow::Cow, collections::BTreeMap, time::Duration};

struct State {
    names: Vec<String>,
    selected: usize,
    face: usize,
    seconds: u32,
    due: Duration,
}
#[derive(Default)]
pub(super) struct CardTimers {
    states: BTreeMap<ElementId, State>,
    last: Duration,
    pub next: Option<Duration>,
}
impl CardTimers {
    pub fn update(&mut self, theme: &Theme, now: Duration, enabled: bool) -> bool {
        if !enabled || now < self.last {
            self.states.clear();
        }
        self.last = now;
        self.next = None;
        self.states.retain(|id, _| {
            theme.element(*id).is_some_and(|e| {
                e.visible
                    && e.card
                        .as_ref()
                        .is_some_and(|c| c.rotation_seconds.is_some() && c.faces.len() > 1)
            })
        });
        let mut changed = false;
        if !enabled {
            return changed;
        }
        for e in &theme.elements {
            let Some(card) = e.card.as_ref().filter(|c| e.visible && c.faces.len() > 1) else {
                continue;
            };
            let Some(seconds) = card.rotation_seconds.filter(|s| (5..=3600).contains(s)) else {
                continue;
            };
            let interval = Duration::from_secs(u64::from(seconds));
            let state = self.states.entry(e.id).or_insert_with(|| State {
                names: card.faces.clone(),
                selected: card.active_face,
                face: card.active_face,
                seconds,
                due: now + interval,
            });
            if state.names != card.faces
                || state.selected != card.active_face
                || state.seconds != seconds
            {
                *state = State {
                    names: card.faces.clone(),
                    selected: card.active_face,
                    face: card.active_face,
                    seconds,
                    due: now + interval,
                };
                changed = true;
            }
            if now >= state.due {
                state.face = (state.face + 1) % card.faces.len();
                // Resume once after a stall/standby; never queue missed transitions.
                state.due = now + interval;
                changed = true;
            }
            self.next = Some(self.next.map_or(state.due, |next| next.min(state.due)));
        }
        changed
    }
    pub fn theme<'a>(&self, theme: &'a Theme) -> Cow<'a, Theme> {
        if !self.states.iter().any(|(id, s)| {
            theme
                .element(*id)
                .and_then(|e| e.card.as_ref())
                .is_some_and(|c| c.active_face != s.face)
        }) {
            return Cow::Borrowed(theme);
        }
        let mut rendered = theme.clone();
        for e in &mut rendered.elements {
            if let Some(state) = self.states.get(&e.id)
                && let Some(card) = &mut e.card
            {
                card.active_face = state.face;
            }
        }
        Cow::Owned(rendered)
    }
}
