// WP-6.2 — terminal windows. Each window = client.openPane({session_id,
// cols, rows}) + client.attachPane(pane_id, {onData, onTitle, onExit,
// onClose}) feeding xterm.js; PaneSocket.write/resize for input/size.
// store.state.value.panes tracks pane.opened/closed/exited/resized.
export {};
