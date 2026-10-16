package app

// Groups relate sessions doing similar work: a client picks sessions and
// groups them under a tag, each member learns about its peers through
// `nk hook-context`, and notes queued here reach them at their next tool call.
// When one session warns another (SendMessage), both ends get a ✉ cue and
// the message goes in the log.

import (
	"errors"
	"fmt"
	"runtime"
	"sort"
	"strings"
	"sync"
	"time"
	"weak"

	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/events"
	"github.com/aqilputi/nekuroddo/internal/groups"
	"github.com/aqilputi/nekuroddo/internal/store"
)

// Events of the groups slice (plan §2.5).
const (
	EvGroupChanged = "group.changed"
	EvPeerMessage  = "peer.message"
	EvMailCue      = "mail.cue"
)

// A ✉ cue blinks for MailCueFor, then stays dim until MailKeep.
const (
	MailCueFor = 90 * time.Second
	MailKeep   = 15 * time.Minute
)

// maxMessages is how many peer messages the log keeps.
const maxMessages = 100

var (
	ErrNoTargets = errors.New("nenhuma sessão escolhida")
	ErrNoGroup   = errors.New("grupo não existe")
)

// GroupChanged is the payload of group.changed ("" when several changed).
type GroupChanged struct {
	Tag string `json:"tag"`
}

// PeerMessage is the payload of peer.message.
type PeerMessage struct {
	To     string `json:"to"`
	From   string `json:"from"`
	FromID string `json:"from_id,omitempty"`
	Text   string `json:"text"`
}

// MailCueEvent is the payload of mail.cue: a ✉ appeared next to ID.
type MailCueEvent struct {
	ID   string `json:"id"`
	Sent bool   `json:"sent"`
}

// MailCue is the ✉ next to a session that just sent (Sent) or got a message.
type MailCue struct {
	At   time.Time `json:"at"`
	Sent bool      `json:"sent"`
}

// CueState is how a ✉ cue shows at a given time.
type CueState int

const (
	CueNone  CueState = iota // gone (or never there)
	CueBlink                 // fresh: blinking
	CueDim                   // older: dim
)

// State is the cue at now: blinking for MailCueFor, dim until MailKeep.
func (c MailCue) State(now time.Time) CueState {
	if c.At.IsZero() {
		return CueNone
	}
	switch age := now.Sub(c.At); {
	case age > MailKeep:
		return CueNone
	case age < MailCueFor:
		return CueBlink
	}
	return CueDim
}

// PeerMsg is one message seen between sessions.
type PeerMsg struct {
	At       time.Time `json:"at"`
	From     string    `json:"from,omitempty"` // session id; "" when the sender is not known
	To       string    `json:"to"`
	FromName string    `json:"from_name"`
	Text     string    `json:"text"`
}

// PeerSeen is what OnPeer found: the message and who to say told whom.
type PeerSeen struct {
	Msg       PeerMsg
	FromLabel string
	ToLabel   string
}

// NotifyResult is NotifyGroup's Result.Data.
type NotifyResult struct {
	Tag     string `json:"tag"`
	Members int    `json:"members"`
	Stopped int    `json:"stopped"` // not running: they get it when they resume
}

// groupState is the groups slice's state. It lives beside App (App's struct
// belongs to app.go) and is created on first use.
type groupState struct {
	mu   sync.Mutex
	file groups.File
	mail map[string]MailCue
	msgs []PeerMsg

	regOnce sync.Once
	regErr  error
}

// Keyed weakly, so the state goes away with its App.
var groupStates sync.Map // weak.Pointer[App] → *groupState

func (a *App) groupState() *groupState {
	k := weak.Make(a)
	v, ok := groupStates.Load(k)
	if !ok {
		var loaded bool
		v, loaded = groupStates.LoadOrStore(k, &groupState{file: a.ports.Groups.Load(), mail: map[string]MailCue{}})
		if !loaded {
			runtime.AddCleanup(a, func(k weak.Pointer[App]) { groupStates.Delete(k) }, k)
		}
	}
	g := v.(*groupState)
	g.regOnce.Do(func() { g.regErr = a.registerGroupCommands() })
	return g
}

func groupsCopy(f groups.File) groups.File {
	c := groups.File{Groups: make(map[string][]groups.Member, len(f.Groups))}
	for tag, ms := range f.Groups {
		c.Groups[tag] = append([]groups.Member(nil), ms...)
	}
	return c
}

// Groups is a copy of the groups as last read or written.
func (a *App) Groups() groups.File {
	g := a.groupState()
	g.mu.Lock()
	defer g.mu.Unlock()
	return groupsCopy(g.file)
}

// Mail is a copy of the ✉ cues by session id.
func (a *App) Mail() map[string]MailCue {
	g := a.groupState()
	g.mu.Lock()
	defer g.mu.Unlock()
	out := make(map[string]MailCue, len(g.mail))
	for id, c := range g.mail {
		out[id] = c
	}
	return out
}

// Messages is a copy of the message log, oldest first (at most 100).
func (a *App) Messages() []PeerMsg {
	g := a.groupState()
	g.mu.Lock()
	defer g.mu.Unlock()
	return append([]PeerMsg(nil), g.msgs...)
}

// MailBlinking reports whether any ✉ is still blinking (clients keep their
// fast animation clock on meanwhile).
func (a *App) MailBlinking() bool {
	now := a.clock.Now()
	for _, c := range a.Mail() {
		if c.State(now) == CueBlink {
			return true
		}
	}
	return false
}

// Peers are the sessions sharing a group with id, id itself included, sorted;
// nil when it is in no group.
func (a *App) Peers(id string) []string { return PeersIn(a.Groups(), id) }

// PeersIn is Peers over a given groups file.
func PeersIn(f groups.File, id string) []string {
	seen := map[string]bool{}
	for _, tag := range f.Of(id) {
		for _, mb := range f.Groups[tag] {
			seen[mb.SessionID] = true
		}
	}
	if len(seen) == 0 {
		return nil
	}
	out := make([]string, 0, len(seen))
	for s := range seen {
		out = append(out, s)
	}
	sort.Strings(out)
	return out
}

// groupSessionsOr is ss, or the app's snapshot when the client passed none.
func (a *App) groupSessionsOr(ss []store.Session) []store.Session {
	if len(ss) > 0 {
		return ss
	}
	return a.Snapshot().Sessions
}

func groupFindSession(ss []store.Session, id string) *store.Session {
	for i := range ss {
		if ss[i].SessionID == id {
			return &ss[i]
		}
	}
	return nil
}

// SessionNamed finds the sender of a message by the name Claude gave it
// (names cut with "…" in some listings match by prefix).
func SessionNamed(ss []store.Session, name string) *store.Session {
	if name == "" {
		return nil
	}
	for i := range ss {
		if s := &ss[i]; s.Name == name || s.Label() == name {
			return s
		}
	}
	if p, ok := strings.CutSuffix(name, "…"); ok && p != "" {
		for i := range ss {
			if s := &ss[i]; strings.HasPrefix(s.Name, p) {
				return s
			}
		}
	}
	return nil
}

// groupMemberOf is how a session is kept in groups.json: Name is its SendMessage
// address.
func groupMemberOf(s *store.Session) groups.Member {
	mb := groups.Member{SessionID: s.SessionID, Name: s.Name, Label: s.Label(), Cwd: s.Cwd}
	if mb.Name == "" {
		mb.Name = s.Label()
	}
	return mb
}

// saveGroups writes f and makes it the app's state.
func (a *App) saveGroups(f groups.File) error {
	if err := a.ports.Groups.Save(f); err != nil {
		return err
	}
	g := a.groupState()
	g.mu.Lock()
	g.file = groupsCopy(f)
	g.mu.Unlock()
	return nil
}

// reloadGroups re-reads the store (another nekuroddo may have changed it).
func (a *App) reloadGroups() groups.File {
	f := a.ports.Groups.Load()
	g := a.groupState()
	g.mu.Lock()
	g.file = groupsCopy(f)
	g.mu.Unlock()
	return f
}

func groupFail(prefix string, err error) (Result, error) {
	return Result{Toast: prefix + err.Error(), Err: true}, err
}

// GroupTargets puts the client's targets (picked sessions, else the one
// under the cursor) in group tag. ss is the client's view of the sessions
// (the app's snapshot when empty); ids it does not know are skipped.
func (a *App) GroupTargets(ctx Context, tag string, ss ...store.Session) (Result, error) {
	tag = core.CleanTag(tag)
	ss = a.groupSessionsOr(ss)
	var ms []groups.Member
	for _, id := range ctx.Targets() {
		if s := groupFindSession(ss, id); s != nil {
			ms = append(ms, groupMemberOf(s))
		}
	}
	if tag == "" || len(ms) == 0 {
		return Result{Toast: "group: escolha sessões com space (ou ponha o cursor numa)", Err: true}, ErrNoTargets
	}
	f := a.reloadGroups()
	f.Add(tag, ms...)
	if err := a.saveGroups(f); err != nil {
		return groupFail("group: ", err)
	}
	a.Publish(EvGroupChanged, GroupChanged{Tag: tag})
	return Info(fmt.Sprintf("#%s: %d sessões — cada uma fica sabendo dos pares na próxima ferramenta", tag, len(f.Groups[tag]))), nil
}

// UngroupTargets takes the client's targets out of group tag (of every group
// when tag is "").
func (a *App) UngroupTargets(ctx Context, tag string) (Result, error) {
	tag = core.CleanTag(tag)
	f := a.reloadGroups()
	n := f.Remove(tag, ctx.Targets()...)
	if err := a.saveGroups(f); err != nil {
		return groupFail("ungroup: ", err)
	}
	if n == 0 {
		return Result{Toast: "ungroup: nada a tirar", Err: true}, nil
	}
	a.Publish(EvGroupChanged, GroupChanged{Tag: tag})
	return Info(fmt.Sprintf("%d saíram do grupo", n)), nil
}

// DissolveGroup drops group tag; its members are told at their next tool.
func (a *App) DissolveGroup(tag string) (Result, error) {
	tag = core.CleanTag(tag)
	if tag == "" {
		return Result{Toast: "dissolve: qual grupo?", Err: true}, ErrNoGroup
	}
	f := a.reloadGroups()
	delete(f.Groups, tag)
	if err := a.saveGroups(f); err != nil {
		return groupFail("", err)
	}
	a.Publish(EvGroupChanged, GroupChanged{Tag: tag})
	return Info("#" + tag + " desfeito"), nil
}

// NotifyGroup queues text for every member of tag; each gets it at its next
// tool call or prompt, so sessions not running only see it when they resume
// (they are counted in NotifyResult.Stopped and in the toast).
func (a *App) NotifyGroup(tag, text string, ss ...store.Session) (Result, error) {
	tag = core.CleanTag(tag)
	ms := a.reloadGroups().Groups[tag]
	if len(ms) == 0 {
		return Result{Toast: "notify: #" + tag + " não existe", Err: true}, ErrNoGroup
	}
	ss = a.groupSessionsOr(ss)
	now := a.clock.Now()
	stopped := 0
	for _, mb := range ms {
		if err := a.ports.Groups.Queue(mb.SessionID, groups.Note{At: now, Tag: tag, Text: text}); err != nil {
			return groupFail("notify: ", err)
		}
		a.mailCue(mb.SessionID, MailCue{At: now})
		a.logPeerMsg(PeerMsg{At: now, To: mb.SessionID, FromName: "você", Text: "[#" + tag + "] " + text})
		if s := groupFindSession(ss, mb.SessionID); s == nil || s.State != "busy" {
			stopped++
		}
	}
	msg := fmt.Sprintf("✉ #%s: aviso na fila de %d sessões", tag, len(ms))
	if stopped > 0 {
		msg += fmt.Sprintf(" (%d paradas recebem quando voltarem a rodar)", stopped)
	}
	return Result{Toast: msg, Data: NotifyResult{Tag: tag, Members: len(ms), Stopped: stopped}}, nil
}

// OnPeer records a message one session sent another (a "peer" hook event):
// ✉ on both ends and the message in the log. The sender is found by name.
func (a *App) OnPeer(e events.Event, ss ...store.Session) PeerSeen {
	ss = a.groupSessionsOr(ss)
	now := e.At
	if now.IsZero() {
		now = a.clock.Now()
	}
	a.mailCue(e.SessionID, MailCue{At: now})
	seen := PeerSeen{Msg: PeerMsg{At: now, To: e.SessionID, FromName: e.From, Text: e.Message}, FromLabel: e.From, ToLabel: "?"}
	if from := SessionNamed(ss, e.From); from != nil {
		seen.Msg.From = from.SessionID
		seen.FromLabel = from.Label()
		a.mailCue(from.SessionID, MailCue{At: now, Sent: true})
	}
	if to := groupFindSession(ss, e.SessionID); to != nil {
		seen.ToLabel = to.Label()
	}
	a.logPeerMsg(seen.Msg)
	return seen
}

func (a *App) mailCue(id string, c MailCue) {
	g := a.groupState()
	g.mu.Lock()
	g.mail[id] = c
	g.mu.Unlock()
	a.Publish(EvMailCue, MailCueEvent{ID: id, Sent: c.Sent})
}

func (a *App) logPeerMsg(p PeerMsg) {
	g := a.groupState()
	g.mu.Lock()
	g.msgs = append(g.msgs, p)
	if len(g.msgs) > maxMessages {
		g.msgs = append([]PeerMsg(nil), g.msgs[len(g.msgs)-maxMessages:]...)
	}
	g.mu.Unlock()
	a.Publish(EvPeerMessage, PeerMessage{To: p.To, From: p.FromName, FromID: p.From, Text: p.Text})
}

// SyncGroupNames reloads the groups (another nekuroddo may have changed
// them) and refreshes the names kept for members when Claude renamed a
// session: they are the SendMessage addresses. It saves, and publishes
// group.changed, only when something changed.
func (a *App) SyncGroupNames(ss []store.Session) (changed bool, err error) {
	f := a.reloadGroups()
	for _, ms := range f.Groups {
		for i, mb := range ms {
			if s := groupFindSession(ss, mb.SessionID); s != nil {
				if nm := groupMemberOf(s); nm != mb {
					ms[i], changed = nm, true
				}
			}
		}
	}
	if !changed {
		return false, nil
	}
	if err := a.saveGroups(f); err != nil {
		return true, err
	}
	a.Publish(EvGroupChanged, GroupChanged{})
	return true, nil
}

// RegisterGroupCommands registers group.add, group.remove, group.notify and
// group.dissolve (plan §2.4). It runs once per App; the slice's first use
// calls it too.
func (a *App) RegisterGroupCommands() error { return a.groupState().regErr }

func (a *App) registerGroupCommands() error {
	tagArg := core.Arg{Name: "tag", Type: core.ArgTag, Prompt: "grupo"}
	reqTag := tagArg
	reqTag.Required = true
	textArg := core.Arg{Name: "text", Type: core.ArgString, Prompt: "mensagem", Required: true}
	cmds := []struct {
		c   core.Command
		run Runner
	}{
		{core.Command{Name: "group.add", Title: "agrupa as sessões escolhidas", Hint: "grupos", Mode: core.ModeNormal, Args: []core.Arg{reqTag}},
			func(ctx Context, args Args) (Result, error) { return a.GroupTargets(ctx, args.String("tag")) }},
		{core.Command{Name: "group.remove", Title: "tira as sessões escolhidas do grupo", Hint: "grupos", Mode: core.ModeNormal, Args: []core.Arg{tagArg}},
			func(ctx Context, args Args) (Result, error) { return a.UngroupTargets(ctx, args.String("tag")) }},
		{core.Command{Name: "group.notify", Title: "avisa o grupo", Hint: "grupos", Mode: core.ModeNormal, Args: []core.Arg{tagArg, textArg}},
			func(ctx Context, args Args) (Result, error) {
				tag := args.String("tag")
				if core.CleanTag(tag) == "" {
					if tags := a.Groups().Of(ctx.SessionID); len(tags) > 0 {
						tag = tags[0]
					}
				}
				return a.NotifyGroup(tag, args.String("text"))
			}},
		{core.Command{Name: "group.dissolve", Title: "desfaz o grupo", Hint: "grupos", Mode: core.ModeNormal, Args: []core.Arg{reqTag}, Confirm: true},
			func(_ Context, args Args) (Result, error) {
				tag := core.CleanTag(args.String("tag"))
				return a.Ask("desfazer o grupo #"+tag+"? as sessões são avisadas na próxima ferramenta",
					func() (Result, error) { return a.DissolveGroup(tag) }), nil
			}},
	}
	for _, c := range cmds {
		if err := a.reg.Register(c.c, c.run); err != nil {
			return err
		}
	}
	return nil
}
