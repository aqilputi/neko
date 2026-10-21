package rpc

// Client is the typed Go client of protocol v1: the tests use it, and so
// will `nk run` and a remote TUI.

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"sync"

	"github.com/coder/websocket"

	"github.com/aqilputi/nekuroddo/internal/core"
	"github.com/aqilputi/nekuroddo/internal/rpc/proto"
)

// APIError is a non-2xx answer.
type APIError struct {
	Status  int
	Code    string
	Message string
}

func (e *APIError) Error() string {
	return fmt.Sprintf("rpc: %d %s: %s", e.Status, e.Code, e.Message)
}

// IsCode reports whether err is an APIError with code.
func IsCode(err error, code string) bool {
	var api *APIError
	return errors.As(err, &api) && api.Code == code
}

// Client talks to one server.
type Client struct {
	base  string // http://host[:port], no trailing slash
	token string
	tr    *http.Transport
	hc    *http.Client
}

// Dial makes a client for "unix:///path", "tcp://127.0.0.1:port" or
// "http://127.0.0.1:port"; token is sent as a bearer token (TCP needs it).
// Nothing is connected until the first call.
func Dial(endpoint, token string) (*Client, error) {
	tr := &http.Transport{}
	var base string
	scheme, rest, _ := strings.Cut(endpoint, "://")
	switch scheme {
	case "unix":
		if rest == "" {
			return nil, errors.New("rpc: unix:// sem caminho")
		}
		tr.DialContext = func(ctx context.Context, _, _ string) (net.Conn, error) {
			var d net.Dialer
			return d.DialContext(ctx, "unix", rest)
		}
		base = "http://nk"
	case "tcp", "http":
		base = "http://" + strings.TrimSuffix(rest, "/")
	default:
		return nil, fmt.Errorf("rpc: endereço %q: use unix:///caminho ou tcp://127.0.0.1:porta", endpoint)
	}
	return &Client{base: base, token: token, tr: tr, hc: &http.Client{Transport: tr}}, nil
}

// Close drops idle connections.
func (c *Client) Close() { c.tr.CloseIdleConnections() }

func (c *Client) request(ctx context.Context, method, path string, in any) (*http.Request, error) {
	var body io.Reader
	if in != nil {
		b, err := json.Marshal(in)
		if err != nil {
			return nil, err
		}
		body = bytes.NewReader(b)
	}
	req, err := http.NewRequestWithContext(ctx, method, c.base+path, body)
	if err != nil {
		return nil, err
	}
	if in != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	if c.token != "" {
		req.Header.Set("Authorization", "Bearer "+c.token)
	}
	return req, nil
}

func apiError(resp *http.Response) error {
	var e proto.Error
	b, _ := io.ReadAll(io.LimitReader(resp.Body, maxBody))
	if json.Unmarshal(b, &e) != nil || e.Code == "" {
		e = proto.Error{Code: http.StatusText(resp.StatusCode), Error: strings.TrimSpace(string(b))}
	}
	return &APIError{Status: resp.StatusCode, Code: e.Code, Message: e.Error}
}

// do sends in (if not nil) as JSON and decodes the answer into out (if not nil).
func (c *Client) do(ctx context.Context, method, path string, in, out any) error {
	req, err := c.request(ctx, method, path, in)
	if err != nil {
		return err
	}
	resp, err := c.hc.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode/100 != 2 {
		return apiError(resp)
	}
	if out == nil {
		return nil
	}
	return json.NewDecoder(resp.Body).Decode(out)
}

// Hello is GET /v1/hello.
func (c *Client) Hello(ctx context.Context) (proto.Hello, error) {
	var h proto.Hello
	return h, c.do(ctx, http.MethodGet, "/v1/hello", nil, &h)
}

// SessionTail is GET /v1/sessions/{id}/tail: the last n lines of the
// session's activity (n <= 0: the server's default, proto.DefaultTail).
func (c *Client) SessionTail(ctx context.Context, sessionID string, n int) ([]core.TailLine, error) {
	path := "/v1/sessions/" + url.PathEscape(sessionID) + "/tail"
	if n > 0 {
		path += "?n=" + strconv.Itoa(n)
	}
	var t proto.Tail
	err := c.do(ctx, http.MethodGet, path, nil, &t)
	return t.Lines, err
}

// Snapshot is GET /v1/snapshot.
func (c *Client) Snapshot(ctx context.Context) (proto.Snapshot, error) {
	var s proto.Snapshot
	return s, c.do(ctx, http.MethodGet, "/v1/snapshot", nil, &s)
}

// Commands is GET /v1/commands; Available is computed for sel.
func (c *Client) Commands(ctx context.Context, sel proto.Context) ([]proto.CommandInfo, error) {
	q := url.Values{}
	set := func(k, v string) {
		if v != "" {
			q.Set(k, v)
		}
	}
	set("session_id", sel.SessionID)
	set("tab", sel.Tab)
	set("issue_key", sel.IssueKey)
	set("picked", strings.Join(sel.Picked, ","))
	if sel.TodoID != 0 {
		q.Set("todo_id", strconv.Itoa(sel.TodoID))
	}
	path := "/v1/commands"
	if len(q) > 0 {
		path += "?" + q.Encode()
	}
	var out []proto.CommandInfo
	return out, c.do(ctx, http.MethodGet, path, nil, &out)
}

// Keymap is GET /v1/keymap.
func (c *Client) Keymap(ctx context.Context) (core.Keymap, error) {
	var km core.Keymap
	return km, c.do(ctx, http.MethodGet, "/v1/keymap", nil, &km)
}

// Run is POST /v1/commands/{name}.
func (c *Client) Run(ctx context.Context, name string, args map[string]string, sel proto.Context) (proto.Result, error) {
	var r proto.Result
	return r, c.do(ctx, http.MethodPost, "/v1/commands/"+url.PathEscape(name), proto.RunRequest{Args: args, Context: sel}, &r)
}

// Confirm is POST /v1/confirm/{token}: yes runs the parked action, no drops it.
func (c *Client) Confirm(ctx context.Context, token string, yes bool) (proto.Result, error) {
	var r proto.Result
	return r, c.do(ctx, http.MethodPost, "/v1/confirm/"+url.PathEscape(token), proto.ConfirmRequest{Yes: yes}, &r)
}

// Panes is GET /v1/panes.
func (c *Client) Panes(ctx context.Context) ([]proto.Pane, error) {
	var ps []proto.Pane
	return ps, c.do(ctx, http.MethodGet, "/v1/panes", nil, &ps)
}

// OpenPane is POST /v1/panes.
func (c *Client) OpenPane(ctx context.Context, req proto.PaneOpenRequest) (proto.PaneOpened, error) {
	var out proto.PaneOpened
	return out, c.do(ctx, http.MethodPost, "/v1/panes", req, &out)
}

// ResizePane is POST /v1/panes/{id}/resize.
func (c *Client) ResizePane(ctx context.Context, id string, cols, rows int) error {
	return c.do(ctx, http.MethodPost, "/v1/panes/"+url.PathEscape(id)+"/resize", proto.Resize{Cols: cols, Rows: rows}, nil)
}

// ClosePane is DELETE /v1/panes/{id}.
func (c *Client) ClosePane(ctx context.Context, id string) error {
	return c.do(ctx, http.MethodDelete, "/v1/panes/"+url.PathEscape(id), nil, nil)
}

// ViewTree is POST /v1/view/tree: the tree built with st (send back the
// state of the previous answer to keep its order, palette and cursor).
func (c *Client) ViewTree(ctx context.Context, st proto.ViewState) (proto.Tree, error) {
	var out proto.Tree
	return out, c.do(ctx, http.MethodPost, "/v1/view/tree", proto.TreeRequest{State: st}, &out)
}

// Theme is GET /v1/theme: the named theme, or the active one when name is "".
func (c *Client) Theme(ctx context.Context, name string) (proto.Theme, error) {
	path := "/v1/theme"
	if name != "" {
		path += "?name=" + url.QueryEscape(name)
	}
	var out proto.Theme
	return out, c.do(ctx, http.MethodGet, path, nil, &out)
}

// LuaEval is POST /v1/lua/eval. A Lua error comes back in LuaEval.Error,
// not as err.
func (c *Client) LuaEval(ctx context.Context, code string) (proto.LuaEval, error) {
	var out proto.LuaEval
	return out, c.do(ctx, http.MethodPost, "/v1/lua/eval", proto.LuaEvalRequest{Code: code}, &out)
}

// ---- events ----

// EventStream is an open GET /v1/events. C closes when the stream ends
// (Close, the context, or the server); Err then says why, and LastID is what
// to resume from.
type EventStream struct {
	C <-chan proto.Event

	cancel context.CancelFunc
	done   chan struct{}
	mu     sync.Mutex
	err    error
	lastID string
}

// Events opens the event stream. lastID "" starts live; otherwise the server
// replays what came after it (Last-Event-ID), sending a "gap" event first if
// it no longer has all of it.
func (c *Client) Events(ctx context.Context, lastID string) (*EventStream, error) {
	ctx, cancel := context.WithCancel(ctx)
	req, err := c.request(ctx, http.MethodGet, "/v1/events", nil)
	if err != nil {
		cancel()
		return nil, err
	}
	req.Header.Set("Accept", "text/event-stream")
	if lastID != "" {
		req.Header.Set("Last-Event-ID", lastID)
	}
	resp, err := c.hc.Do(req)
	if err != nil {
		cancel()
		return nil, err
	}
	if resp.StatusCode != http.StatusOK {
		defer resp.Body.Close()
		cancel()
		return nil, apiError(resp)
	}
	ch := make(chan proto.Event, 64)
	s := &EventStream{C: ch, cancel: cancel, done: make(chan struct{}), lastID: lastID}
	go s.read(ctx, resp.Body, ch)
	return s, nil
}

func (s *EventStream) read(ctx context.Context, body io.ReadCloser, ch chan<- proto.Event) {
	defer close(s.done)
	defer close(ch)
	defer body.Close()
	r := bufio.NewReaderSize(body, 64<<10)
	var id, data string
	for {
		line, err := r.ReadString('\n')
		if err != nil {
			s.setErr(err)
			return
		}
		line = strings.TrimSuffix(strings.TrimSuffix(line, "\n"), "\r")
		switch {
		case line == "":
			if data != "" {
				var ev proto.Event
				if json.Unmarshal([]byte(data), &ev) == nil {
					if id != "" {
						s.mu.Lock()
						s.lastID = id
						s.mu.Unlock()
					}
					select {
					case ch <- ev:
					case <-ctx.Done():
						s.setErr(ctx.Err())
						return
					}
				}
			}
			id, data = "", ""
		case strings.HasPrefix(line, "id:"):
			id = strings.TrimSpace(strings.TrimPrefix(line, "id:"))
		case strings.HasPrefix(line, "data:"):
			data += strings.TrimPrefix(strings.TrimPrefix(line, "data:"), " ")
		}
		// "event:", "retry:" and ":" comments: the name is in the data too
	}
}

func (s *EventStream) setErr(err error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.err == nil {
		s.err = err
	}
}

// Err is why the stream ended (nil while it runs).
func (s *EventStream) Err() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.err
}

// LastID is the id of the last event received (to resume from).
func (s *EventStream) LastID() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.lastID
}

// Close ends the stream and waits for its reader.
func (s *EventStream) Close() {
	s.cancel()
	<-s.done
}

// ---- panes ----

// Frame is one message from a pane socket: raw output (Data) or a control
// frame (title, exit).
type Frame struct {
	Data    []byte
	Control *proto.Control
}

// PaneConn is an open pane WebSocket.
type PaneConn struct {
	c *websocket.Conn
}

// AttachPane opens WS /v1/panes/{id}/ws.
func (c *Client) AttachPane(ctx context.Context, id string) (*PaneConn, error) {
	u := "ws" + strings.TrimPrefix(c.base, "http") + "/v1/panes/" + url.PathEscape(id) + "/ws"
	if c.token != "" {
		u += "?token=" + url.QueryEscape(c.token)
	}
	conn, resp, err := websocket.Dial(ctx, u, &websocket.DialOptions{HTTPClient: c.hc})
	if resp != nil && resp.Body != nil {
		resp.Body.Close()
	}
	if err != nil {
		return nil, err
	}
	conn.SetReadLimit(wsReadLimit)
	return &PaneConn{c: conn}, nil
}

// Write sends bytes to the pane's program (keys, paste: already encoded).
func (p *PaneConn) Write(ctx context.Context, b []byte) error {
	return p.c.Write(ctx, websocket.MessageBinary, b)
}

// Resize asks for a new size (last writer wins).
func (p *PaneConn) Resize(ctx context.Context, cols, rows int) error {
	return p.sendControl(ctx, proto.Control{Type: proto.CtlResize, Cols: cols, Rows: rows})
}

func (p *PaneConn) sendControl(ctx context.Context, ctl proto.Control) error {
	b, _ := json.Marshal(ctl)
	return p.writeText(ctx, string(b))
}

func (p *PaneConn) writeText(ctx context.Context, s string) error {
	return p.c.Write(ctx, websocket.MessageText, []byte(s))
}

// Read waits for the next frame. After the exit frame the server closes the
// socket and Read fails.
func (p *PaneConn) Read(ctx context.Context) (Frame, error) {
	typ, b, err := p.c.Read(ctx)
	if err != nil {
		return Frame{}, err
	}
	if typ == websocket.MessageBinary {
		return Frame{Data: b}, nil
	}
	var ctl proto.Control
	if err := json.Unmarshal(b, &ctl); err != nil {
		return Frame{}, fmt.Errorf("rpc: frame de controle: %w", err)
	}
	return Frame{Control: &ctl}, nil
}

// Close leaves the pane running and closes this viewer.
func (p *PaneConn) Close() { _ = p.c.Close(websocket.StatusNormalClosure, "") }
