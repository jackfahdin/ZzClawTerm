//! Semantic GPUI actions dispatched by the resolved ZzClawTerm keymap.

use gpui::actions;

actions!(
    zzclawterm_shortcuts,
    [
        TerminalCopy,
        TerminalPaste,
        TerminalPasteSelected,
        TerminalFind,
        TerminalClear,
        TerminalSelectAll,
        ManageSyncGroups,
        ShowCommandSuggestions,
        ToggleRecording,
        NewSession,
        TemporarySshLink,
        QuickSwitch,
        NewLocalTerminal,
        CloseTab,
        NextTab,
        PreviousTab,
        DuplicateSession,
        MultiplexSsh,
        DuplicateSessionWithCommand,
        MultiplexSshWithCommand,
        ToggleLeftSidebar,
        ToggleRightSidebar,
        ZoomIn,
        ZoomOut,
        ResetZoom,
        OpenSettings,
        OpenChat,
        ShowAllCommands,
        RenameFile,
        CopySelectedConnections,
        LockScreen
    ]
);

#[derive(Clone, Debug, PartialEq, Eq, gpui::Action)]
#[action(namespace = zzclawterm_shortcuts, no_json)]
pub(crate) struct SwitchToTab {
    pub(crate) index: usize,
}
