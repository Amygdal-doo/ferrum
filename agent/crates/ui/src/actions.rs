//! FerrumChat's actions (namespace `ferrum_chat`), bound by [`crate::init`].

use gpui::actions;

actions!(
    ferrum_chat,
    [
        /// Send the composer's message (queued while a turn runs).
        Send,
        NewLine,
        /// Next mode: Agent → Ask → Plan.
        CycleMode,
        /// Stop the running turn.
        Stop,
        /// Close the mention picker, then overlays, then stop.
        Dismiss,
        NewThread,
        ToggleThreads,
        ToggleChanges,
        OpenSettings,
        AcceptAll,
        RejectAll,
        MentionNext,
        MentionPrev,
        AcceptMention,
        BuildPlan,
    ]
);
