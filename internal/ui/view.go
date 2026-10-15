package ui

import (
	"fmt"
	"image/color"
	"strings"
	"time"

	"charm.land/bubbles/v2/help"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	uv "github.com/charmbracelet/ultraviolet"

	"github.com/fakio/neko/internal/config"
	"github.com/fakio/neko/internal/jira"
	"github.com/fakio/neko/internal/store"
	"github.com/fakio/neko/internal/theme"
)

func lipglossWidth(s string) int { return lipgloss.Width(s) }

// layout returns the left list width, the right panel width and the height of
// the body (between the tab bar and the status line).
func (m *Model) layout() (int, int, int) {
	bodyH := m.h - 1 /*header*/ - 1 /*tabs*/ - 1 /*status*/
	if m.zoom && m.active != "" {
		return 0, m.w, max(bodyH, 5)
	}
	left := max(m.w*45/100, 44)
	if left > m.w-30 {
		left = m.w / 2
	}
	return left, m.w - left, max(bodyH, 5)
}

// uiRects are the screen areas mouse handling and cell drawing need.
type uiRects struct {
	tabs, left, right, pane uv.Rectangle
}

func (m *Model) rects() uiRects {
	lw, rw, bh := m.layout()
	return uiRects{
		tabs:  uv.Rect(0, 1, m.w, 1),
		left:  uv.Rect(0, 2, lw, bh),
		right: uv.Rect(lw, 2, rw, bh),
		pane:  uv.Rect(lw+1, 3, max(rw-2, 1), max(bh-2, 1)), // inside the border
	}
}

func (m Model) View() tea.View {
	var v tea.View
	v.AltScreen = true
	v.ReportFocus = true
	busy, waiting, _ := m.counts()
	v.WindowTitle = fmt.Sprintf("neko · %d esperando · %d rodando", waiting, busy)
	// OSC 7501: terminals that support it show in the tab who is waiting.
	ps := &tea.ProgramStatus{App: "neko", State: tea.ProgramStateIdle}
	switch {
	case waiting > 0:
		ps.State, ps.Kind, ps.Message = tea.ProgramStateBlocked, tea.ProgramStatusKindQuestion, fmt.Sprintf("%d te esperando", waiting)
	case busy > 0:
		ps.State, ps.Message = tea.ProgramStateWorking, fmt.Sprintf("%d rodando", busy)
	}
	v.ProgramStatus = ps
	if m.w == 0 {
		v.SetContent("carregando…")
		return v
	}
	t := m.t()
	bg := lipgloss.NewStyle().Background(t.Bg).Foreground(t.Text)
	v.MouseMode = tea.MouseModeCellMotion

	lw, rw, bh := m.layout()
	right := m.right(t, rw, bh)
	body := right
	if lw > 0 {
		body = lipgloss.JoinHorizontal(lipgloss.Top, m.left(t, lw, bh), right)
	}
	screen := bg.Width(m.w).Height(m.h).MaxHeight(m.h).Render(
		lipgloss.JoinVertical(lipgloss.Left, m.header(t), m.tabs(t), body, m.status(t)))

	// Compose on a cell buffer: the embedded claude is copied cell by cell
	// (no ANSI re-parse, widths as the emulator measured them) and dialogs
	// are drawn on top.
	canvas := uv.NewScreenBuffer(m.w, m.h)
	uv.NewStyledString(screen).Draw(canvas, canvas.Bounds())
	if p := m.panes[m.active]; p != nil {
		p.Draw(canvas, m.rects().pane)
	}
	if d := m.top(); d != nil {
		w := d.width(&m)
		box := dialogBox(t, d.title(), d.view(&m, t, w), w)
		bw, bh := lipgloss.Size(box)
		x, y := max((m.w-bw)/2, 0), max((m.h-bh)/3, 1)
		uv.NewStyledString(box).Draw(canvas, uv.Rect(x, y, bw, bh))
	}
	v.SetContent(canvas.Render())
	v.BackgroundColor = t.Bg
	// The embedded claude's cursor becomes the real terminal cursor.
	if p := m.panes[m.active]; p != nil && m.mode == modeInsert && m.top() == nil {
		if x, y, ok := p.Cursor(); ok {
			r := m.rects().pane
			if x >= 0 && y >= 0 && x < r.Dx() && y < r.Dy() {
				v.Cursor = tea.NewCursor(r.Min.X+x, r.Min.Y+y)
			}
		}
	}
	return v
}

// ---- header & tabs ----

func (m Model) counts() (busy, waiting, done int) {
	for _, s := range m.snap.Sessions {
		switch {
		case s.State == "busy":
			busy++
		case s.Attention == store.AttnBlocked || s.Attention == store.AttnIdle:
			waiting++
		case s.Attention == store.AttnReturned:
			done++
		}
	}
	return
}

func (m Model) header(t theme.Theme) string {
	base := lipgloss.NewStyle().Background(t.Surface)
	logo := base.Render(" ") + gradient(base.Bold(true), "ᓚᘏᗢ neko", t.Accent, t.Accent2) + base.Render(" ")
	busy, waiting, done := m.counts()
	pill := func(c color.Color, s string) string {
		return base.Foreground(c).Bold(true).Render(s)
	}
	stats := strings.Join([]string{
		pill(t.Busy, fmt.Sprintf("● %d rodando", busy)),
		pill(t.Blocked, fmt.Sprintf("◆ %d te esperando", waiting)),
		pill(t.Done, fmt.Sprintf("✓ %d voltaram", done)),
	}, base.Render("  "))

	// The subscription limits (claude -p /usage) are what matter on a plan;
	// the $ figure is only an estimate at API prices.
	var cost string
	if l, ok := m.usage.Find("session"); ok {
		cost = base.Foreground(t.Subtle).Render("  5h ") + costBar(t, float64(l.Pct), 100, 12, base) +
			base.Foreground(usageColor(t, l.Pct)).Bold(true).Render(fmt.Sprintf(" %d%%", l.Pct)) +
			base.Foreground(t.Muted).Render(" ↻"+l.ShortReset())
		if w, ok := m.usage.Find("all models"); ok {
			cost += base.Foreground(t.Subtle).Render("  semana ") + base.Foreground(usageColor(t, w.Pct)).Bold(true).Render(fmt.Sprintf("%d%%", w.Pct))
		}
	} else {
		cost = base.Foreground(t.Subtle).Render("  hoje ≈") + base.Foreground(t.Text).Bold(true).Render(fmt.Sprintf("$%.2f", m.snap.TodayCost))
		if b := m.cfg.DailyBudgetUSD; b > 0 {
			cost += base.Render(" ") + costBar(t, m.snap.TodayCost, b, 14, base) + base.Foreground(t.Muted).Render(fmt.Sprintf(" /$%.0f", b))
		}
	}
	right := base.Foreground(t.Muted).Render(" " + t.Name + " ")
	if m.snap.Err != nil {
		right = base.Foreground(t.Error).Render(" ⚠ " + truncate(m.snap.Err.Error(), 40) + " ")
	}
	left := logo + base.Render(" ") + stats + cost
	gap := m.w - lipgloss.Width(left) - lipgloss.Width(right)
	if gap < 0 {
		return base.Width(m.w).MaxWidth(m.w).Render(left)
	}
	return left + base.Render(strings.Repeat(" ", gap)) + right
}

func (m Model) tabs(t theme.Theme) string {
	base := lipgloss.NewStyle().Background(t.Bg)
	names := []string{
		"1 Sessões",
		fmt.Sprintf("2 Inbox (%d)", len(m.inbox)),
		fmt.Sprintf("3 Todos (%d)", len(m.todos)),
		fmt.Sprintf("4 Arquivo (%d)", m.archivedN),
	}
	var out []string
	for i, n := range names {
		st := base.Foreground(t.Muted).Padding(0, 1)
		if tab(i) == m.tab {
			st = lipgloss.NewStyle().Background(t.Accent).Foreground(t.Bg).Bold(true).Padding(0, 1)
		} else if i == 1 && len(m.inbox) > 0 {
			st = base.Foreground(t.Blocked).Bold(true).Padding(0, 1)
		}
		out = append(out, st.Render(n))
	}
	s := strings.Join(out, base.Render(" "))
	view := fmt.Sprintf("   ativas ≤%.0fd", m.cfg.StaleDays)
	if m.showAll {
		view = "   tudo"
	}
	s += base.Foreground(t.Muted).Render(view + " · ordem: " + sortModes[m.sortMode])
	if m.hidden > 0 {
		s += base.Foreground(t.Muted).Render(fmt.Sprintf(" · %d ocultas (H)", m.hidden))
	}
	if m.filter != "" {
		s += base.Foreground(t.Accent2).Render("   filtro: /" + m.filter)
	}
	// one line, always: cut instead of wrapping (wrapping pushed the UI down)
	return base.Width(m.w).MaxWidth(m.w).MaxHeight(1).Render(lipgloss.NewStyle().MaxWidth(m.w).Render(s))
}

// ---- cost bar ----

// costBar draws a gradient bar: green while cheap, red as it approaches max.
func costBar(t theme.Theme, v, maxV float64, w int, base lipgloss.Style) string {
	if w <= 0 {
		return ""
	}
	frac := 0.0
	if maxV > 0 {
		frac = min(v/maxV, 1)
	}
	cols := lipgloss.Blend1D(w, t.CostLow, t.CostHigh)
	filled := int(frac*float64(w) + 0.5)
	if v > 0 && filled == 0 {
		filled = 1
	}
	var b strings.Builder
	for i := 0; i < w; i++ {
		if i < filled {
			b.WriteString(base.Foreground(cols[i]).Render("█"))
		} else {
			b.WriteString(base.Foreground(t.Overlay).Render("░"))
		}
	}
	return b.String()
}

// pctBar is a bar of p% with 1/8-cell precision, so a 2% share still shows.
func pctBar(t theme.Theme, p float64, w int, base lipgloss.Style) string {
	if w <= 0 {
		return ""
	}
	cols := lipgloss.Blend1D(w, t.CostLow, t.CostHigh)
	eighths := int(min(max(p, 0), 100)/100*float64(w*8) + 0.5)
	if p > 0 && eighths == 0 {
		eighths = 1
	}
	parts := []string{"", "▏", "▎", "▍", "▌", "▋", "▊", "▉"}
	var b strings.Builder
	for i := 0; i < w; i++ {
		switch fill := eighths - i*8; {
		case fill >= 8:
			b.WriteString(base.Foreground(cols[i]).Render("█"))
		case fill > 0:
			b.WriteString(base.Foreground(cols[i]).Background(t.Overlay).Render(parts[fill]))
		default:
			b.WriteString(base.Foreground(t.Overlay).Render("░"))
		}
	}
	return b.String()
}

func money(v float64) string {
	switch {
	case v >= 100:
		return fmt.Sprintf("$%.0f", v)
	case v >= 10:
		return fmt.Sprintf("$%.1f", v)
	}
	return fmt.Sprintf("$%.2f", v)
}

func ago(t time.Time) string {
	if t.IsZero() {
		return ""
	}
	d := time.Since(t)
	switch {
	case d < time.Minute:
		return "agora"
	case d < time.Hour:
		return fmt.Sprintf("%dm", int(d.Minutes()))
	case d < 24*time.Hour:
		return fmt.Sprintf("%dh", int(d.Hours()))
	}
	return fmt.Sprintf("%dd", int(d.Hours()/24))
}

// ---- state icons ----

func stateIcon(t theme.Theme, s *store.Session, frame int) (string, color.Color) {
	switch {
	case s.Archived:
		return "⌫", t.Muted
	case s.Ended:
		return "⊘", t.Muted
	case s.Stale && s.State != "busy":
		return "z", t.Muted
	case s.State == "busy":
		spin := []string{"◐", "◓", "◑", "◒"}
		return spin[frame%4], t.Busy
	case s.Attention == store.AttnBlocked:
		if frame%8 < 4 {
			return "◆", t.Blocked
		}
		return "◇", t.Blocked
	case s.Attention == store.AttnReturned:
		return "✓", t.Done
	case s.Attention == store.AttnIdle:
		return "❯", t.Blocked
	case s.State == "done":
		return "✓", t.Muted
	case s.State == "stopped":
		return "■", t.Idle
	case s.State == "failed":
		return "✗", t.Error
	case s.Kind == "history":
		return "·", t.Muted
	}
	return "○", t.Idle
}

func stateLabel(s *store.Session) string {
	if s.Archived {
		return "arquivada"
	}
	if s.Ended {
		return "encerrada"
	}
	if s.Stale && s.State != "busy" {
		return "parada (" + s.State + ")"
	}
	switch s.Attention {
	case store.AttnBlocked:
		if s.WaitingFor != "" {
			return "te esperando · " + s.WaitingFor
		}
		return "te esperando"
	case store.AttnReturned:
		return "voltou"
	case store.AttnIdle:
		return "no prompt"
	}
	switch s.State {
	case "busy":
		return "rodando"
	case "done":
		return "concluída"
	case "stopped":
		return "parada"
	case "history":
		return "histórico"
	case "failed":
		return "falhou"
	}
	return s.State
}

// ---- left: tree / inbox / todos ----

func panel(t theme.Theme, w, h int, focused bool, title string, body string) string {
	bc := t.Border
	if focused {
		bc = t.Accent
	}
	st := lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).BorderForeground(bc).BorderBackground(t.Bg).
		Background(t.Bg).Width(w).Height(h).MaxHeight(h)
	out := st.Render(body)
	if title != "" {
		// draw the title into the top border
		lines := strings.SplitN(out, "\n", 2)
		tt := lipgloss.NewStyle().Foreground(t.Bg).Background(bc).Bold(true).Render(" " + title + " ")
		top := lipgloss.NewStyle().Foreground(bc).Background(t.Bg).Render("╭─") + tt
		rest := w - lipgloss.Width(top) - 1
		if rest > 0 {
			top += lipgloss.NewStyle().Foreground(bc).Background(t.Bg).Render(strings.Repeat("─", rest) + "╮")
			lines[0] = top
		}
		out = strings.Join(lines, "\n")
	}
	return out
}

func (m *Model) left(t theme.Theme, w, h int) string {
	inner := h - 2
	focused := m.mode != modeInsert
	switch m.tab {
	case tabInbox:
		return panel(t, w, h, focused, "inbox · quem te espera", m.inboxView(t, w-2, inner))
	case tabTodos:
		return panel(t, w, h, focused, "todos", m.todoView(t, w-2, inner))
	}
	if m.tab == tabArchive {
		return panel(t, w, h, focused, "arquivo · X restaura", m.treeView(t, w-2, inner))
	}
	return panel(t, w, h, focused, "sessões", m.treeView(t, w-2, inner))
}

func scroll(cursor, n, h int) (int, int) {
	if n <= h {
		return 0, n
	}
	start := max(0, min(cursor-h/2, n-h))
	return start, start + h
}

func (m *Model) treeView(t theme.Theme, w, h int) string {
	if len(m.rows) == 0 {
		msg := "nenhuma sessão ainda… (c cria uma)"
		if m.tab == tabArchive {
			msg = "nada arquivado — X numa sessão (ou num contexto) arquiva"
		}
		if m.refreshing && m.snap.At.IsZero() {
			msg = "procurando sessões…"
		}
		return lipgloss.NewStyle().Foreground(t.Muted).Background(t.Bg).Padding(1, 2).Render(msg)
	}
	frame := m.catFrame()
	start, end := scroll(m.cursor, len(m.rows), h)
	var lines []string
	barW := 8
	for i := start; i < end; i++ {
		r := m.rows[i]
		sel := i == m.cursor
		bgc := t.Bg
		if sel {
			bgc = t.Overlay
		}
		base := lipgloss.NewStyle().Background(bgc)
		var left, right string
		switch r.kind {
		case "ctx":
			cc := t.ContextColor(m.ctxColor[r.ctx])
			arrow := "▾"
			if m.collapsed["ctx:"+r.ctx] {
				arrow = "▸"
			}
			left = base.Foreground(cc).Bold(true).Render(fmt.Sprintf(" %s %s", arrow, r.ctx)) +
				base.Foreground(t.Muted).Render(fmt.Sprintf(" (%d)", r.ctxN))
			var ctxPct float64
			for i := range m.snap.Sessions {
				if ss := &m.snap.Sessions[i]; ss.Context == r.ctx {
					ctxPct += m.windowPct(ss)
				}
			}
			right = base.Foreground(cc).Render(pctText(ctxPct) + " ")
		case "sess":
			s := r.sess
			icon, ic := stateIcon(t, s, frame)
			cc := t.ContextColor(m.ctxColor[r.ctx])
			guide := base.Foreground(cc).Render("  │ ")
			name := s.Label()
			labelStyle := base.Foreground(t.Text)
			if s.Attention != store.AttnNone {
				labelStyle = labelStyle.Bold(true)
			}
			if s.Kind == "history" {
				labelStyle = base.Foreground(t.Subtle)
			}
			if s.Stale {
				labelStyle = base.Foreground(t.Muted).Italic(true)
			}
			sub := ""
			running, done := 0, 0
			for _, sa := range s.Subs {
				if sa.Done {
					done++
				} else {
					running++
				}
			}
			if running > 0 {
				sub += base.Foreground(t.Accent2).Render(fmt.Sprintf(" ⑂%d", running))
			}
			if done > 0 {
				sub += base.Foreground(t.Muted).Render(fmt.Sprintf(" ✓%d", done))
			}
			// share of the 5-hour window (empty when idle in this window)
			pct := m.windowPct(s)
			bar := base.Render(strings.Repeat(" ", barW))
			if pct > 0 {
				bar = pctBar(t, pct, barW, base)
			}
			right = bar + base.Foreground(t.Subtle).Render(fmt.Sprintf(" %4s %4s ", pctText(pct), ago(s.LastAt)))
			avail := w - lipgloss.Width(right) - 7 - lipgloss.Width(sub)
			left = guide + base.Foreground(ic).Bold(true).Render(icon) + base.Render(" ") +
				labelStyle.Render(truncate(name, max(avail, 4))) + sub
		case "sub":
			sa := r.sub
			cc := t.ContextColor(m.ctxColor[r.ctx])
			icon, ic := "◌", t.Busy
			if sa.Done {
				icon, ic = "●", t.Muted
			}
			desc := sa.Description
			if desc == "" {
				desc = sa.Type
			}
			right = base.Foreground(t.Muted).Render(shortModel(sa.Model) + " ")
			avail := w - lipgloss.Width(right) - 11
			left = base.Foreground(cc).Render("  │   └ ") + base.Foreground(ic).Render(icon) + base.Render(" ") +
				base.Foreground(t.Subtle).Italic(true).Render(truncate(desc, max(avail, 4)))
		}
		gap := w - lipgloss.Width(left) - lipgloss.Width(right)
		if gap < 1 {
			gap = 1
		}
		line := left + base.Render(strings.Repeat(" ", gap)) + right
		if sel {
			line = lipgloss.NewStyle().Background(bgc).Foreground(t.Accent).Render("▌") + lipgloss.NewStyle().MaxWidth(w-1).Render(stripFirst(line))
		}
		lines = append(lines, lipgloss.NewStyle().MaxWidth(w).Render(line))
	}
	return strings.Join(lines, "\n")
}

// stripFirst drops the first cell of a styled line (room for the cursor bar).
func stripFirst(s string) string {
	return lipgloss.NewStyle().MaxWidth(lipgloss.Width(s)).Render(strings.Replace(s, " ", "", 1))
}

func shortModel(m string) string {
	m = strings.TrimPrefix(m, "claude-")
	if m == "" {
		return "?"
	}
	return m
}

func (m *Model) catFrame() int { return m.frame }

func (m *Model) inboxView(t theme.Theme, w, h int) string {
	if len(m.inbox) == 0 {
		return lipgloss.NewStyle().Foreground(t.Done).Background(t.Bg).Padding(1, 2).
			Render("ninguém te esperando ✨\no gato aprova.")
	}
	frame := m.catFrame()
	var lines []string
	perItem := 4
	start, end := scroll(m.inboxCur, len(m.inbox), max(h/perItem, 1))
	for i := start; i < end; i++ {
		s := m.inbox[i]
		sel := i == m.inboxCur
		bgc := t.Bg
		if sel {
			bgc = t.Overlay
		}
		base := lipgloss.NewStyle().Background(bgc).Width(w)
		icon, ic := stateIcon(t, s, frame)
		cc := t.ContextColor(m.ctxColor[s.Context])
		head := lipgloss.NewStyle().Background(bgc).Foreground(ic).Bold(true).Render(" "+icon+" "+stateLabel(s)) +
			lipgloss.NewStyle().Background(bgc).Foreground(cc).Render("  "+s.Context) +
			lipgloss.NewStyle().Background(bgc).Foreground(t.Muted).Render("  "+ago(s.LastAt))
		title := lipgloss.NewStyle().Background(bgc).Foreground(t.Text).Bold(true).Render("   " + truncate(s.Label(), w-4))
		what := s.LastText
		switch {
		case s.Needs != "":
			what = "❓ " + s.Needs
		case len(s.Questions) > 0:
			what = "❓ " + s.Questions[0].Question
		case s.WaitingTool != "" && s.Attention == store.AttnBlocked:
			what = "🔐 " + s.WaitingTool
		}
		body := lipgloss.NewStyle().Background(bgc).Foreground(t.Subtle).Italic(true).Render("   " + truncate(what, w-4))
		lines = append(lines, base.Render(head), base.Render(title), base.Render(body), lipgloss.NewStyle().Background(t.Bg).Width(w).Render(""))
	}
	return strings.Join(lines, "\n")
}

func (m *Model) todoView(t theme.Theme, w, h int) string {
	if !m.todoAvail {
		return lipgloss.NewStyle().Foreground(t.Muted).Background(t.Bg).Padding(1, 2).Render("skill todo não encontrada em ~/.claude/skills/todo")
	}
	if m.todoErr != nil {
		return lipgloss.NewStyle().Foreground(t.Error).Background(t.Bg).Padding(1, 2).Render(m.todoErr.Error())
	}
	if len(m.todos) == 0 {
		return lipgloss.NewStyle().Foreground(t.Done).Background(t.Bg).Padding(1, 2).Render("lista vazia 🎉")
	}
	type line struct {
		s   string
		idx int
	}
	var all []line
	group := ""
	gi := 0
	for i, r := range m.todos {
		if r.group != group {
			group = r.group
			gi++
			head := " ▾ " + group
			if r.issue != nil {
				head = " ▾ ⚑ jira · minhas issues"
				if !m.jira.FetchedAt.IsZero() {
					head += fmt.Sprintf(" (há %s)", ago(m.jira.FetchedAt))
				}
				if m.jiraLoading {
					head += " · buscando…"
				}
			}
			all = append(all, line{lipgloss.NewStyle().Background(t.Bg).Foreground(t.ContextColor(gi)).Bold(true).Render(head), -1})
		}
		sel := i == m.todoCur
		bgc := t.Bg
		if sel {
			bgc = t.Overlay
		}
		b := lipgloss.NewStyle().Background(bgc)
		if r.issue != nil {
			all = append(all, line{m.issueLine(t, b, r.issue, w), i})
			continue
		}
		tk := r.task
		st := map[string]string{"todo": "☐", "doing": "◐", "waiting": "⧗", "done": "☑"}[tk.Status]
		stc := map[string]color.Color{"todo": t.Subtle, "doing": t.Busy, "waiting": t.Accent2, "done": t.Done}[tk.Status]
		if stc == nil {
			stc = t.Subtle
		}
		pri := ""
		if tk.Priority != nil {
			pri = b.Foreground(t.Blocked).Bold(true).Render(strings.Repeat("!", 4-*tk.Priority) + " ")
		}
		meta := ""
		if tk.When != nil {
			meta += " ◷ " + (*tk.When)[5:]
		}
		if tk.WaitingOn != nil {
			meta += " ⧗ " + *tk.WaitingOn
		}
		if len(tk.Contexts) > 0 {
			meta += " " + strings.Join(tk.Contexts, " ")
		}
		if k := tk.JiraKey(); k != "" {
			meta = " ⚑ " + k + meta
		}
		right := b.Foreground(t.Muted).Render(truncate(meta, w/2) + " ")
		left := b.Foreground(t.Muted).Render(fmt.Sprintf("   %3d ", tk.ID)) + b.Foreground(stc).Render(st+" ") + pri
		title := b.Foreground(t.Text).Render(truncate(tk.Title, max(w-lipgloss.Width(left)-lipgloss.Width(right)-1, 4)))
		gap := max(w-lipgloss.Width(left)-lipgloss.Width(title)-lipgloss.Width(right), 1)
		all = append(all, line{left + title + b.Render(strings.Repeat(" ", gap)) + right, i})
	}
	curLine := 0
	for i, l := range all {
		if l.idx == m.todoCur {
			curLine = i
		}
	}
	start, end := scroll(curLine, len(all), h)
	var out []string
	for _, l := range all[start:end] {
		out = append(out, lipgloss.NewStyle().MaxWidth(w).Render(l.s))
	}
	return strings.Join(out, "\n")
}

func (m *Model) issueLine(t theme.Theme, b lipgloss.Style, is *jira.Issue, w int) string {
	stc := map[string]color.Color{"new": t.Subtle, "indeterminate": t.Busy, "done": t.Done}[is.StatusCat]
	if stc == nil {
		stc = t.Subtle
	}
	icon := map[string]string{"new": "☐", "indeterminate": "◐", "done": "☑"}[is.StatusCat]
	if icon == "" {
		icon = "☐"
	}
	link := ""
	if tk := m.todoForKey(is.Key); tk != nil {
		link = fmt.Sprintf(" ↔#%d", tk.ID)
	}
	left := b.Foreground(t.Accent2).Bold(true).Render(fmt.Sprintf("   %-11s", is.Key)) + b.Foreground(stc).Render(icon+" ")
	right := b.Foreground(t.Muted).Render(truncate(is.Status+link+" "+ago(is.UpdatedAt()), w/3) + " ")
	title := b.Foreground(t.Text).Render(truncate(is.Summary, max(w-lipgloss.Width(left)-lipgloss.Width(right)-1, 4)))
	gap := max(w-lipgloss.Width(left)-lipgloss.Width(title)-lipgloss.Width(right), 1)
	return left + title + b.Render(strings.Repeat(" ", gap)) + right
}

// ---- right: pane or details ----

func (m *Model) right(t theme.Theme, w, h int) string {
	if p := m.panes[m.active]; p != nil {
		title := "▸ " + p.Title
		if wt := strings.TrimSpace(p.WinTitle()); wt != "" {
			title += " · " + wt
		}
		hint := "  · i entra · w fecha · z zoom"
		if m.mode == modeInsert {
			hint = "  · INSERT (ctrl+] sai)"
		}
		if n := p.Scrolled(); n > 0 {
			hint = fmt.Sprintf("  · ↑%d linhas (digite para voltar)", n)
		}
		title = truncate(title, max(w-lipgloss.Width(hint)-8, 10)) + hint
		// the terminal's cells are drawn over this box in View
		return panel(t, w, h, m.mode == modeInsert, title, "")
	}
	if m.tab == tabTodos {
		return panel(t, w, h, false, "tarefa", m.todoDetail(t, w-4))
	}
	s := m.selected()
	if s == nil {
		if r := m.cur(); r != nil && r.kind == "ctx" {
			return panel(t, w, h, false, r.ctx, m.ctxDetail(t, r.ctx, w-4))
		}
		return panel(t, w, h, false, "", "")
	}
	return panel(t, w, h, false, "detalhes", m.sessDetail(t, s, w-4))
}

func kv(t theme.Theme, k, v string, vc color.Color) string {
	return lipgloss.NewStyle().Background(t.Bg).Foreground(t.Muted).Width(12).Render(k) +
		lipgloss.NewStyle().Background(t.Bg).Foreground(vc).Render(v)
}

func wrap(t theme.Theme, s string, w int, c color.Color, italic bool) string {
	return lipgloss.NewStyle().Background(t.Bg).Foreground(c).Italic(italic).Width(w).Render(s)
}

func (m *Model) sessDetail(t theme.Theme, s *store.Session, w int) string {
	icon, ic := stateIcon(t, s, m.catFrame())
	cc := t.ContextColor(m.ctxColor[s.Context])
	b := lipgloss.NewStyle().Background(t.Bg)
	lines := []string{
		b.Foreground(t.Text).Bold(true).Width(w).Render(s.Label()),
		b.Foreground(ic).Bold(true).Render(icon+" "+stateLabel(s)) + b.Foreground(cc).Render("   "+s.Context),
		"",
		kv(t, "dir", config.Tilde(s.Cwd), t.Subtle),
		kv(t, "tipo", s.Kind+idSuffix(s), t.Subtle),
		kv(t, "modelo", shortModel(s.Model), t.Accent2),
		m.windowLine(t, s, b),
		kv(t, "tokens", fmt.Sprintf("%d out", s.Tokens), t.Subtle),
		kv(t, "início", s.Started.Format("02/01 15:04"), t.Subtle),
		kv(t, "atividade", ago(s.LastAt), t.Subtle),
	}
	if s.Archived {
		lines = append(lines, kv(t, "arquivada", s.ArchivedAt.Format("02/01 15:04")+" · X restaura", t.Muted))
	}
	if e, ok := m.events[s.SessionID]; ok {
		what := e.Name
		if e.Message != "" {
			what += " · " + e.Message
		}
		lines = append(lines, kv(t, "hook", truncate(what, w-14)+"  "+ago(e.At), t.Accent2))
	}
	if s.LastUser != "" {
		lines = append(lines, "", b.Foreground(t.Accent).Bold(true).Render("você pediu"), wrap(t, s.LastUser, w, t.Subtle, false))
	}
	if s.Detail != "" {
		lines = append(lines, "", b.Foreground(t.Accent).Bold(true).Render("status"), wrap(t, s.Detail, w, t.Subtle, false))
	}
	if s.Needs != "" {
		lines = append(lines, "", b.Foreground(t.Blocked).Bold(true).Render("❓ precisa de você"), wrap(t, s.Needs, w, t.Text, false))
	}
	n := 0
	for _, q := range s.Questions {
		lines = append(lines, "", b.Foreground(t.Blocked).Bold(true).Width(w).Render(q.Question))
		for _, o := range q.Options {
			n++
			opt := b.Foreground(t.Accent).Bold(true).Render(fmt.Sprintf(" %d ", n)) + b.Foreground(t.Text).Render(o.Label)
			if o.Description != "" {
				opt += b.Foreground(t.Muted).Render(" — " + o.Description)
			}
			lines = append(lines, lipgloss.NewStyle().MaxWidth(w).Render(opt))
		}
	}
	if s.SuggestedReply != "" {
		lines = append(lines, "", b.Foreground(t.Done).Bold(true).Render("💡 resposta sugerida (r já preenche)"), wrap(t, s.SuggestedReply, w, t.Text, true))
	}
	if s.WaitingTool != "" && s.Attention == store.AttnBlocked && s.Needs == "" && len(s.Questions) == 0 {
		lines = append(lines, "", b.Foreground(t.Blocked).Bold(true).Render("🔐 última ferramenta pendente"), wrap(t, s.WaitingTool, w, t.Text, false))
	}
	for _, l := range s.Links {
		icon := "🔗"
		if l.Kind == "pr" {
			icon = "⇡ PR"
		}
		lines = append(lines, b.Foreground(t.Accent2).Render(truncate(icon+" "+l.Title+"  "+l.Href, w)))
	}
	if s.LastText != "" {
		lines = append(lines, "", b.Foreground(t.Accent).Bold(true).Render("claude disse por último"), wrap(t, s.LastText, w, t.Text, true))
	}
	if len(s.Subs) > 0 {
		lines = append(lines, "", b.Foreground(t.Accent2).Bold(true).Render(fmt.Sprintf("subagents (%d)", len(s.Subs))))
		for _, sa := range s.Subs {
			st := "◌"
			if sa.Done {
				st = "●"
			}
			lines = append(lines, b.Foreground(t.Subtle).Render(truncate(fmt.Sprintf(" %s %s · %s", st, sa.Description, shortModel(sa.Model)), w)))
		}
	}
	hint := "enter abre aqui · O tela cheia · r responde"
	switch {
	case s.Kind == "interactive":
		hint = fmt.Sprintf("aberta num terminal (pid %d) — só leitura aqui", s.PID)
	case s.Kind == "history":
		hint = "enter retoma em background e abre · O retoma em tela cheia"
	}
	lines = append(lines, "", b.Foreground(t.Muted).Italic(true).Render(hint))
	return strings.Join(lines, "\n")
}

func idSuffix(s *store.Session) string {
	if s.ShortID != "" {
		return " · " + s.ShortID
	}
	return ""
}

func (m *Model) ctxDetail(t theme.Theme, ctx string, w int) string {
	b := lipgloss.NewStyle().Background(t.Bg)
	var total float64
	var n, busy, wait int
	var top []*store.Session
	for i := range m.snap.Sessions {
		s := &m.snap.Sessions[i]
		if s.Context != ctx || (s.Stale && !m.showAll) {
			continue
		}
		n++
		total += m.windowPct(s)
		if s.State == "busy" {
			busy++
		}
		if s.Attention != store.AttnNone {
			wait++
		}
		top = append(top, s)
	}
	lines := []string{
		b.Foreground(t.ContextColor(m.ctxColor[ctx])).Bold(true).Render(ctx),
		"",
		kv(t, "sessões", fmt.Sprint(n), t.Text),
		kv(t, "rodando", fmt.Sprint(busy), t.Busy),
		kv(t, "esperando", fmt.Sprint(wait), t.Blocked),
		kv(t, "janela 5h", "≈"+orDash(pctText(total))+" (estimativa por sessão)", t.Text),
		"",
		b.Foreground(t.Accent).Bold(true).Render("uso da janela de 5h por sessão"),
	}
	for _, s := range top {
		p := m.windowPct(s)
		if p <= 0 {
			continue
		}
		lines = append(lines, pctBar(t, p, 16, b)+b.Foreground(t.Subtle).Render(fmt.Sprintf(" %4s ", pctText(p))+truncate(s.Label(), w-24)))
	}
	return strings.Join(lines, "\n")
}

func (m *Model) todoDetail(t theme.Theme, w int) string {
	b := lipgloss.NewStyle().Background(t.Bg)
	if r := m.curRow(); r != nil && r.issue != nil {
		is := r.issue
		lines := []string{
			b.Foreground(t.Accent2).Bold(true).Render(is.Key) + b.Foreground(t.Muted).Render("  "+is.Type),
			b.Foreground(t.Text).Bold(true).Width(w).Render(is.Summary),
			"",
			kv(t, "status", is.Status, t.Busy),
			kv(t, "prioridade", is.Priority, t.Subtle),
			kv(t, "atualizada", ago(is.UpdatedAt()), t.Subtle),
			kv(t, "link", is.URL, t.Accent),
		}
		if tk := m.todoForKey(is.Key); tk != nil {
			lines = append(lines, kv(t, "todo", fmt.Sprintf("#%d %s", tk.ID, truncate(tk.Title, w-14)), t.Done))
		}
		lines = append(lines, "", b.Foreground(t.Muted).Italic(true).Render("enter dispara agent · J vira todo · o abre no browser · F atualiza"))
		return strings.Join(lines, "\n")
	}
	tk := m.curTodo()
	if tk == nil {
		return ""
	}
	lines := []string{
		b.Foreground(t.Text).Bold(true).Width(w).Render(fmt.Sprintf("#%d %s", tk.ID, tk.Title)),
		"",
		kv(t, "status", tk.Status, t.Busy),
		kv(t, "iniciativa", tk.Initiative, t.Accent2),
	}
	if len(tk.Contexts) > 0 {
		lines = append(lines, kv(t, "contextos", strings.Join(tk.Contexts, " "), t.Subtle))
	}
	if tk.When != nil {
		lines = append(lines, kv(t, "quando", *tk.When, t.Subtle))
	}
	if tk.Due != nil {
		lines = append(lines, kv(t, "prazo", *tk.Due, t.Blocked))
	}
	if tk.Desc != "" {
		lines = append(lines, "", wrap(t, tk.Desc, w, t.Text, false))
	}
	for _, r := range tk.Refs {
		lines = append(lines, kv(t, r.Kind, truncate(r.Value+"  "+r.URL, w-12), t.Accent))
	}
	for _, l := range tk.Links {
		lines = append(lines, kv(t, "link", l, t.Accent))
	}
	for _, n := range tk.Notes {
		lines = append(lines, "", b.Foreground(t.Muted).Render(n.At[:min(16, len(n.At))]), wrap(t, n.Text, w, t.Subtle, true))
	}
	hint := "enter dispara um agent · s start · D done · J cria no Jira"
	if tk.JiraKey() != "" {
		hint = "enter dispara um agent · s start · D done"
	}
	lines = append(lines, "", b.Foreground(t.Muted).Italic(true).Render(hint))
	return strings.Join(lines, "\n")
}

// ---- status line ----

func (m Model) status(t theme.Theme) string {
	var modeS lipgloss.Style
	var name string
	switch m.mode {
	case modeInsert:
		modeS, name = lipgloss.NewStyle().Background(t.Done).Foreground(t.Bg), " INSERT "
	case modeInput:
		modeS, name = lipgloss.NewStyle().Background(t.Accent2).Foreground(t.Bg), " INPUT "
	default:
		modeS, name = lipgloss.NewStyle().Background(t.Accent).Foreground(t.Bg), " NORMAL "
	}
	base := lipgloss.NewStyle().Background(t.Surface).Foreground(t.Subtle)
	left := modeS.Bold(true).Render(name) + base.Render(" ")
	kitty := m.cat.View(t, base)
	// "ctrl+/ todos os comandos" never gets cut: it is drawn apart, right
	// before the cat.
	allKeys := ""
	if m.mode != modeInput {
		allKeys = base.Foreground(t.Accent).Bold(true).Render(" ctrl+/") + base.Foreground(t.Subtle).Render(" todos os comandos ")
	}
	kitty = allKeys + kitty
	room := max(m.w-lipgloss.Width(left)-lipgloss.Width(kitty)-1, 0)
	var mid string
	switch {
	case m.mode == modeInput:
		m.input.SetWidth(max(room-lipgloss.Width(m.input.Prompt)-1, 10))
		mid = m.input.View()
	case m.toast != "":
		c := t.Done
		if m.toastErr {
			c = t.Error
		}
		mid = base.Foreground(c).Render(m.toast)
	default:
		h := help.New()
		h.Styles = helpStyles(t)
		h.Styles.ShortKey = h.Styles.ShortKey.Background(t.Surface)
		h.Styles.ShortDesc = h.Styles.ShortDesc.Background(t.Surface)
		h.Styles.ShortSeparator = h.Styles.ShortSeparator.Background(t.Surface)
		h.Styles.Ellipsis = h.Styles.Ellipsis.Background(t.Surface)
		h.ShortSeparator = " · "
		h.SetWidth(room)
		mid = h.ShortHelpView(m.shortHelp())
		if m.hooksSeen {
			room -= 2
		}
	}
	mid = lipgloss.NewStyle().MaxWidth(room).Render(mid)
	live := ""
	if m.hooksSeen {
		live = base.Foreground(t.Done).Render("↯")
	}
	gap := max(m.w-lipgloss.Width(left)-lipgloss.Width(mid)-lipgloss.Width(live)-lipgloss.Width(kitty), 0)
	return lipgloss.NewStyle().MaxWidth(m.w).Render(left + mid + base.Render(strings.Repeat(" ", gap)) + live + kitty)
}

// gradient colors each rune of s from a to b (cheap: the header is short).
func gradient(st lipgloss.Style, s string, a, b color.Color) string {
	r := []rune(s)
	cols := lipgloss.Blend1D(max(len(r), 2), a, b)
	var out strings.Builder
	for i, ch := range r {
		out.WriteString(st.Foreground(cols[i]).Render(string(ch)))
	}
	return out.String()
}

func orDash(s string) string {
	if s == "" {
		return "—"
	}
	return s
}

// windowLine is the session's share of the 5-hour window in the details.
func (m *Model) windowLine(t theme.Theme, s *store.Session, b lipgloss.Style) string {
	l, ok := m.usage.Find("session")
	if !ok {
		return kv(t, "janela 5h", "— (sem /usage)", t.Muted)
	}
	p := m.windowPct(s)
	if p <= 0 {
		return kv(t, "janela 5h", fmt.Sprintf("sem uso nesta janela (total %d%%)", l.Pct), t.Muted)
	}
	return kv(t, "janela 5h", fmt.Sprintf("≈%s de %d%% usados", pctText(p), l.Pct), t.Text) + b.Render("  ") + pctBar(t, p, 20, b)
}
