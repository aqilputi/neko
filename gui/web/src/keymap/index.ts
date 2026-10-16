// WP-6.2 — key resolution. Resolve KeyboardEvent → command name from
// store.state.value.keymap (GET /v1/keymap, core.BuildKeymap), honouring
// mode (normal/insert/input/*); `scope: client` commands (nav.*, tab.*,
// view.*) act on the tree via sync.refreshTree(patch), core ones go through
// sync.run(name, args).
export {};
