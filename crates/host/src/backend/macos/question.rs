//! What a module's accessibility question is told when it is not answered in full.
//!
//! Its own file for the reason `budget.rs` has one: a note per thread, with nothing macOS about it,
//! so `backend/mod.rs` borrows it by `#[path]` under `cfg(test)` and its rules run where they are
//! written. The walks and reads of `ax.rs` say why a question was not answered in full — an
//! application that did not answer a read, one in the not-answering quarantine, a walk stopped at
//! its bound — and `element_ask` (`mod.rs`) settles the answer with it: what was found stands,
//! nothing found is "not known", a dump says it is not all of the tree (`ElementAnswer::settled`).
//!
//! **Only inside a module's question.** The same reads serve the host itself — a window's controls,
//! the window in front, the focus chain — and a reason noted there would be told to nobody, or to
//! the next question asked. So a reason is noted only between [`begin`] and [`end`].

use std::cell::RefCell;

use crate::errors::HostError;

thread_local! {
    /// The question being asked: `None` outside one, `Some` of the first reason it is not answered
    /// in full, once one is noted.
    static QUESTION: RefCell<Option<Option<HostError>>> = const { RefCell::new(None) };
}

/// A module's question begins: nothing is noted against it yet.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn begin() {
    QUESTION.with(|q| *q.borrow_mut() = Some(None));
}

/// The question is answered: why it was not answered in full, if anything said so. Nothing is
/// noted after it until the next one begins.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn end() -> Option<HostError> {
    QUESTION.with(|q| q.borrow_mut().take().flatten())
}

/// What keeps the question being asked from a whole answer, noted: the first one only, since it is
/// what stopped the rest. Outside a module's question, nothing.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn not_known(why: HostError) {
    QUESTION.with(|q| {
        if let Some(first) = q.borrow_mut().as_mut() {
            if first.is_none() {
                *first = Some(why);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::element;

    /// The first reason inside a question is the one told; none outside one is kept for the next;
    /// a question that met none is told nothing; and a question's end forgets it.
    #[test]
    fn a_question_is_told_the_first_reason_noted_while_it_is_asked() {
        not_known(HostError::of(&element::NOT_ANSWERING));
        begin();
        assert_eq!(end(), None, "a reason noted before the question began is no reason of it");
        begin();
        not_known(HostError::of(&element::QUARANTINED));
        not_known(HostError::of(&element::SEARCH_CUT));
        assert_eq!(end(), Some(HostError::of(&element::QUARANTINED)));
        assert_eq!(end(), None, "told once");
        not_known(HostError::of(&element::HOT_TOO_SLOW));
        begin();
        assert_eq!(end(), None, "nothing noted between two questions reaches the second");
    }
}
