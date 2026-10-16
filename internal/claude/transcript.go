package claude

import (
	"bufio"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

// Usage is the token usage of one API request.
type Usage struct {
	Input        int64 `json:"input_tokens"`
	Output       int64 `json:"output_tokens"`
	CacheWrite   int64 `json:"cache_creation_input_tokens"`
	CacheRead    int64 `json:"cache_read_input_tokens"`
	CacheCreated struct {
		W5m int64 `json:"ephemeral_5m_input_tokens"`
		W1h int64 `json:"ephemeral_1h_input_tokens"`
	} `json:"cache_creation"`
}

// Pricer turns a model id into USD/MTok rates.
type Pricer func(model string) (in, out, cw5m, cr float64)

// Cost prices u. 1-hour cache writes cost 2× input, 5-minute ones 1.25×.
func (u Usage) Cost(model string, price Pricer) float64 {
	in, out, cw5, cr := price(model)
	w1h := u.CacheCreated.W1h
	w5 := u.CacheCreated.W5m
	if w1h+w5 == 0 {
		w5 = u.CacheWrite
	}
	return (float64(u.Input)*in + float64(u.Output)*out + float64(w5)*cw5 +
		float64(w1h)*in*2 + float64(u.CacheRead)*cr) / 1e6
}

// Transcript is the incrementally-parsed state of one .jsonl file.
type Transcript struct {
	Path      string
	SessionID string
	Cwd       string
	Title     string // ai-title (or the session's name)
	Model     string // last model seen
	Cost      float64
	CostByDay map[string]float64 // local date "2006-01-02" → USD, incl. subagents
	// costBuckets: usage weight (USD at API prices) per 5-minute slot, incl.
	// subagents — to tell how much of the current 5-hour window it used.
	costBuckets map[int64]float64
	Tokens      int64 // output tokens
	Requests    int
	LastUser    string
	FirstUser   string // the first real request: names a session with no title
	LastText    string // last assistant text block
	LastTool    string // last tool_use name (+ a hint of its input)
	LastAt      time.Time
	Started     time.Time
	// WaitingTool is set while the last assistant message ended in a tool_use
	// that has no result yet — usually a permission prompt or AskUserQuestion.
	WaitingTool string
	// Ended: the conversation was closed for good (EndConversation /
	// "ended-by-model"). Its background process may linger, still reported
	// as working by the daemon.
	Ended bool

	Subagents []*Subagent

	offset  int64
	partial []byte
	seen    map[string]bool // requestIds already counted
	pending map[string]string
	subs    map[string]*Subagent
	subRead map[string]*subState // read position per subagent file
}

// Subagent is one Agent-tool child of a session.
type Subagent struct {
	ID          string
	Description string
	Type        string
	Model       string
	Cost        float64
	LastText    string
	LastAt      time.Time
	Done        bool
	t           *Transcript
}

type record struct {
	Type        string          `json:"type"`
	Timestamp   string          `json:"timestamp"`
	Cwd         string          `json:"cwd"`
	SessionID   string          `json:"sessionId"`
	RequestID   string          `json:"requestId"`
	AITitle     string          `json:"aiTitle"`
	IsSidechain bool            `json:"isSidechain"`
	Message     json.RawMessage `json:"message"`
}

type message struct {
	ID      string          `json:"id"`
	Role    string          `json:"role"`
	Model   string          `json:"model"`
	Usage   *Usage          `json:"usage"`
	Content json.RawMessage `json:"content"`
}

type block struct {
	Type      string          `json:"type"`
	Text      string          `json:"text"`
	Name      string          `json:"name"`
	ID        string          `json:"id"`
	ToolUseID string          `json:"tool_use_id"`
	Input     json.RawMessage `json:"input"`
}

func NewTranscript(path string) *Transcript {
	return &Transcript{
		Path: path, SessionID: strings.TrimSuffix(filepath.Base(path), ".jsonl"),
		seen: map[string]bool{}, pending: map[string]string{}, subs: map[string]*Subagent{},
		CostByDay: map[string]float64{}, costBuckets: map[int64]float64{},
	}
}

// maxFirstRead caps how much of a huge transcript is parsed on first sight;
// cost from the skipped head is lost, which the UI marks with "≥".
const maxFirstRead = 64 << 20

// Refresh reads whatever was appended since the last call. Cheap when
// nothing changed (one stat).
func (t *Transcript) Refresh(price Pricer) error {
	if err := t.tail(t.Path, &t.offset, &t.partial, func(r record, m *message) { t.apply(r, m, price) }); err != nil {
		return err
	}
	t.refreshSubagents(price)
	return nil
}

func (t *Transcript) tail(path string, off *int64, partial *[]byte, fn func(record, *message)) error {
	f, err := os.Open(path)
	if err != nil {
		return err
	}
	defer f.Close()
	st, err := f.Stat()
	if err != nil {
		return err
	}
	if st.Size() < *off { // rewritten
		*off, *partial = 0, nil
	}
	if st.Size() == *off {
		return nil
	}
	if *off == 0 && st.Size() > maxFirstRead {
		*off = st.Size() - maxFirstRead
	}
	if _, err := f.Seek(*off, io.SeekStart); err != nil {
		return err
	}
	data, err := io.ReadAll(f)
	if err != nil {
		return err
	}
	*off += int64(len(data))
	data = append(*partial, data...)
	last := strings.LastIndexByte(string(data), '\n')
	if last < 0 {
		*partial = data
		return nil
	}
	*partial = append([]byte(nil), data[last+1:]...)
	sc := bufio.NewScanner(strings.NewReader(string(data[:last])))
	sc.Buffer(make([]byte, 1<<20), 64<<20)
	for sc.Scan() {
		var r record
		if json.Unmarshal(sc.Bytes(), &r) != nil {
			continue
		}
		var m *message
		if len(r.Message) > 0 {
			m = new(message)
			if json.Unmarshal(r.Message, m) != nil {
				m = nil
			}
		}
		fn(r, m)
	}
	return nil
}

func parseTime(s string) time.Time {
	t, _ := time.Parse(time.RFC3339Nano, s)
	return t
}

// day is the local calendar date of ts.
func day(ts time.Time) string { return ts.Local().Format("2006-01-02") }

func oneLine(s string, n int) string {
	s = strings.Join(strings.Fields(s), " ")
	if r := []rune(s); len(r) > n {
		return string(r[:n-1]) + "…"
	}
	return s
}

func blocks(raw json.RawMessage) ([]block, string) {
	var s string
	if json.Unmarshal(raw, &s) == nil {
		return nil, s
	}
	var bs []block
	_ = json.Unmarshal(raw, &bs)
	return bs, ""
}

func toolHint(b block) string {
	var in map[string]any
	_ = json.Unmarshal(b.Input, &in)
	for _, k := range []string{"description", "command", "file_path", "pattern", "url", "prompt"} {
		if v, ok := in[k].(string); ok && v != "" {
			return b.Name + ": " + oneLine(v, 60)
		}
	}
	return b.Name
}

func (t *Transcript) apply(r record, m *message, price Pricer) {
	if r.Cwd != "" && t.Cwd == "" {
		t.Cwd = r.Cwd
	}
	ts := parseTime(r.Timestamp)
	if !ts.IsZero() {
		if t.Started.IsZero() {
			t.Started = ts
		}
		if ts.After(t.LastAt) {
			t.LastAt = ts
		}
	}
	switch r.Type {
	case "ai-title":
		if r.AITitle != "" {
			t.Title = r.AITitle
		}
	case "ended-by-model":
		t.Ended = true
	case "assistant":
		if m == nil {
			return
		}
		key := r.RequestID
		if key == "" {
			key = m.ID
		}
		if m.Usage != nil && key != "" && !t.seen[key] {
			t.seen[key] = true
			c := m.Usage.Cost(m.Model, price)
			t.Cost += c
			t.CostByDay[day(ts)] += c
			t.costBuckets[ts.Unix()/300] += c
			t.Tokens += m.Usage.Output
			t.Requests++
		}
		if m.Model != "" && !strings.HasPrefix(m.Model, "<") {
			t.Model = m.Model
		}
		bs, _ := blocks(m.Content)
		for _, b := range bs {
			switch b.Type {
			case "text":
				if strings.TrimSpace(b.Text) != "" {
					t.LastText = oneLine(b.Text, 400)
					t.WaitingTool = ""
				}
			case "tool_use":
				if b.Name == "EndConversation" {
					t.Ended = true
				}
				t.LastTool = toolHint(b)
				t.pending[b.ID] = t.LastTool
				t.WaitingTool = t.LastTool
			}
		}
	case "user":
		if m == nil {
			return
		}
		bs, s := blocks(m.Content)
		if s != "" && !strings.HasPrefix(s, "<") {
			t.LastUser = oneLine(s, 200)
			t.WaitingTool = ""
			t.noteFirst(s)
		}
		for _, b := range bs {
			switch b.Type {
			case "tool_result":
				delete(t.pending, b.ToolUseID)
				if len(t.pending) == 0 {
					t.WaitingTool = ""
				}
			case "text":
				if !strings.HasPrefix(b.Text, "<") && strings.TrimSpace(b.Text) != "" {
					t.LastUser = oneLine(b.Text, 200)
					t.noteFirst(b.Text)
				}
			}
		}
	}
}

// noteFirst keeps the first user text that is a request, not a skill's
// preamble or an interruption notice.
func (t *Transcript) noteFirst(s string) {
	if t.FirstUser != "" {
		return
	}
	s = strings.TrimSpace(s)
	for _, p := range []string{"Base directory for this skill:", "[Request interrupted", "Caveat:", "[structured-output"} {
		if strings.HasPrefix(s, p) {
			return
		}
	}
	t.FirstUser = oneLine(s, 200)
}

// PromptName turns a request into a short session name: its first line,
// cut at a word boundary.
func PromptName(s string) string {
	s, _, _ = strings.Cut(strings.TrimSpace(s), "\n")
	s = strings.Join(strings.Fields(s), " ")
	const n = 48
	r := []rune(s)
	if len(r) <= n {
		return s
	}
	cut := string(r[:n])
	if i := strings.LastIndex(cut, " "); i > n/2 {
		cut = cut[:i]
	}
	return strings.TrimRight(cut, " ,.;:-") + "…"
}

// NekoInternal reports whether a request is one of nekuroddo's own headless calls
// (the Jira tool calls), which are not sessions of yours.
func NekoInternal(firstUser string) bool {
	return strings.HasPrefix(firstUser, "Chame ") && strings.Contains(firstUser, " exatamente uma vez com ") ||
		strings.HasPrefix(firstUser, "Use searchJiraIssuesUsingJql no site") // early versions

}

type subMeta struct {
	AgentType   string `json:"agentType"`
	Description string `json:"description"`
	Model       string `json:"model"`
}

// subagent transcripts live in <session>/subagents/agent-<id>.jsonl
type subState struct {
	off     int64
	partial []byte
}

func (t *Transcript) refreshSubagents(price Pricer) {
	dir := filepath.Join(strings.TrimSuffix(t.Path, ".jsonl"), "subagents")
	files, _ := filepath.Glob(filepath.Join(dir, "agent-*.jsonl"))
	for _, p := range files {
		id := strings.TrimSuffix(strings.TrimPrefix(filepath.Base(p), "agent-"), ".jsonl")
		s := t.subs[id]
		if s == nil {
			s = &Subagent{ID: id, t: t}
			if b, err := os.ReadFile(strings.TrimSuffix(p, ".jsonl") + ".meta.json"); err == nil {
				var meta subMeta
				if json.Unmarshal(b, &meta) == nil {
					s.Description, s.Type, s.Model = meta.Description, meta.AgentType, meta.Model
				}
			}
			t.subs[id] = s
			t.Subagents = append(t.Subagents, s)
		}
		if t.subRead == nil {
			t.subRead = map[string]*subState{}
		}
		st := t.subRead[p]
		if st == nil {
			st = &subState{}
			t.subRead[p] = st
		}
		seen := map[string]bool{}
		_ = t.tail(p, &st.off, &st.partial, func(r record, m *message) {
			if ts := parseTime(r.Timestamp); ts.After(s.LastAt) {
				s.LastAt = ts
			}
			if r.Type == "user" && m != nil && s.Description == "" {
				// no description in the meta: name it after the task it got
				bs, str := blocks(m.Content)
				for _, b := range bs {
					if str == "" && b.Type == "text" {
						str = b.Text
					}
				}
				if str = strings.TrimSpace(str); str != "" && !strings.HasPrefix(str, "<") {
					s.Description = PromptName(str)
				}
			}
			if r.Type != "assistant" || m == nil {
				return
			}
			key := r.RequestID + "/sub"
			if m.Usage != nil && !seen[key] && !t.seen[key+id] {
				seen[key], t.seen[key+id] = true, true
				c := m.Usage.Cost(m.Model, price)
				s.Cost += c
				st := parseTime(r.Timestamp)
				t.CostByDay[day(st)] += c
				t.costBuckets[st.Unix()/300] += c
			}
			if m.Model != "" && !strings.HasPrefix(m.Model, "<") {
				s.Model = m.Model
			}
			bs, _ := blocks(m.Content)
			for _, b := range bs {
				if b.Type == "text" && strings.TrimSpace(b.Text) != "" {
					s.LastText = oneLine(b.Text, 300)
				}
			}
		})
		// Without a status file, an agent idle for a while is treated as done.
		s.Done = time.Since(s.LastAt) > 2*time.Minute
	}
	sort.Slice(t.Subagents, func(i, j int) bool { return t.Subagents[i].LastAt.Before(t.Subagents[j].LastAt) })
}

// TotalCost is the session plus all of its subagents.
func (t *Transcript) TotalCost() float64 {
	c := t.Cost
	for _, s := range t.Subagents {
		c += s.Cost
	}
	return c
}

// CostSince is the usage weight (USD at API prices, subagents included) of
// requests made at or after since, at 5-minute resolution.
func (t *Transcript) CostSince(since time.Time) float64 {
	from := since.Unix() / 300
	var c float64
	for b, v := range t.costBuckets {
		if b >= from {
			c += v
		}
	}
	return c
}
