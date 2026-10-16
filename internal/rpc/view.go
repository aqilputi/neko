package rpc

import (
	"net/http"
	"time"

	"github.com/aqilputi/nekuroddo/internal/app"
	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
	"github.com/aqilputi/nekuroddo/internal/store"
	"github.com/aqilputi/nekuroddo/internal/theme"
)

// viewTree is POST /v1/view/tree: core.BuildTree over the core's snapshot
// with the state the client sent, so a GUI shows the same tree as the TUI
// without reimplementing it. The server keeps no view state: the answer's
// state goes back in the next request.
func (s *Server) viewTree(w http.ResponseWriter, r *http.Request) {
	var req proto.TreeRequest
	if !decode(w, r, &req) {
		return
	}
	vs, err := req.State.Core(s.a.Config().Sort)
	if err != nil {
		writeErr(w, http.StatusBadRequest, proto.CodeBadRequest, err.Error())
		return
	}
	rows, vs := core.BuildTree(s.a.Snapshot().Sessions, vs)
	out := proto.Tree{Rows: make([]proto.TreeRow, 0, len(rows)), State: proto.FromViewState(vs),
		Inbox: make([]string, 0, len(vs.Inbox)), Hidden: vs.Hidden, Archived: vs.ArchivedN}
	now, mail := s.a.Clock().Now(), s.a.Mail()
	for _, row := range rows {
		out.Rows = append(out.Rows, s.treeRow(row, vs, mail, now))
	}
	for _, x := range vs.Inbox {
		out.Inbox = append(out.Inbox, x.SessionID)
	}
	writeJSON(w, http.StatusOK, out)
}

func (s *Server) treeRow(row core.Row, vs core.ViewState, mail map[string]app.MailCue, now time.Time) proto.TreeRow {
	key := core.RowKey(row)
	out := proto.TreeRow{Key: key, Kind: row.Kind, Ctx: row.Ctx, Depth: row.Depth, Color: vs.CtxColor[row.Ctx],
		Collapsed: vs.Collapsed[key]}
	switch row.Kind {
	case core.RowCtx:
		out.CtxCost, out.CtxN = row.CtxCost, row.CtxN
	case core.RowSess:
		sessionRow(&out, row.Sess)
		for _, tag := range s.a.TagsOf(row.Sess.SessionID) {
			out.Groups = append(out.Groups, proto.RowGroup{Tag: tag, Color: core.GroupColorIndex(tag)})
		}
		if c, ok := mail[row.Sess.SessionID]; ok {
			switch c.State(now) {
			case app.CueBlink:
				out.Mail, out.MailState = &proto.MailCue{At: c.At, Sent: c.Sent}, proto.MailBlink
			case app.CueDim:
				out.Mail, out.MailState = &proto.MailCue{At: c.At, Sent: c.Sent}, proto.MailDim
			}
		}
	default:
		b := row.Sub
		out.SessionID, out.ShortID = row.Sess.SessionID, row.Sess.ShortID
		out.Label, out.State, out.Cost = b.Description, proto.SubState(*b), b.Cost
		if out.Label == "" {
			out.Label = b.Type
		}
		out.Sub = &proto.Sub{ID: b.ID, Description: b.Description, Type: b.Type, Model: b.Model,
			LastText: b.LastText, Cost: b.Cost, LastAt: b.LastAt, Done: b.Done}
	}
	return out
}

// sessionRow fills a session row's own fields.
func sessionRow(out *proto.TreeRow, x *store.Session) {
	out.SessionID, out.ShortID, out.Label, out.SessionKind = x.SessionID, x.ShortID, x.Label(), x.Kind
	out.State, out.Attention, out.Cost = x.State, int(x.Attention), x.Cost
	out.Stale, out.Archived, out.Ended, out.Finished = x.Stale, x.Archived, x.Ended, x.Finished
	out.Options = core.OptionCount(x.Questions)
	for _, b := range x.Subs {
		if !b.Done {
			out.SubsRunning++
		}
	}
}

// theme is GET /v1/theme[?name=]: the palette the GUI paints with — the
// named one, or the config's (the first theme when the config follows the
// terminal, which a server cannot see) — and every theme's name.
func (s *Server) theme(w http.ResponseWriter, r *http.Request) {
	th, _ := theme.Lookup(s.a.Config().Theme)
	active := th.Name
	if q := r.URL.Query(); q.Has("name") {
		var ok bool
		if th, ok = theme.Lookup(q.Get("name")); !ok {
			writeErr(w, http.StatusNotFound, proto.CodeNotFound, "tema "+q.Get("name")+" não existe")
			return
		}
	}
	writeJSON(w, http.StatusOK, proto.Theme{Name: th.Name, Tokens: th.Tokens(), Contexts: th.ContextTokens(),
		Themes: theme.Names(), Active: active})
}

// luaEval is POST /v1/lua/eval {code}: the palette's `:lua <code>` over
// HTTP, with the runtime's own budget. A Lua error is still a 200 (the call
// worked) with {error}; no runtime is 501.
func (s *Server) luaEval(w http.ResponseWriter, r *http.Request) {
	if s.o.Lua == nil {
		writeErr(w, http.StatusNotImplemented, proto.CodeNotImplemented, "lua: este servidor não tem runtime Lua")
		return
	}
	var req proto.LuaEvalRequest
	if !decode(w, r, &req) {
		return
	}
	if req.Code == "" {
		writeErr(w, http.StatusBadRequest, proto.CodeBadRequest, "lua: code vazio")
		return
	}
	res, err := s.o.Lua.Eval(req.Code)
	if err != nil {
		writeJSON(w, http.StatusOK, proto.LuaEval{Error: err.Error()})
		return
	}
	writeJSON(w, http.StatusOK, proto.LuaEval{Result: res})
}
