package claude

import (
	"math"
	"os"
	"path/filepath"
	"testing"
)

func flat(string) (float64, float64, float64, float64) { return 4, 20, 5, 0.2 } // opus 5.5

func TestCostDedupAndIncremental(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "s.jsonl")
	// Two content blocks of the same request repeat the same usage: count once.
	line := `{"type":"assistant","requestId":"r1","timestamp":"2026-10-15T10:00:00Z","message":{"model":"claude-opus-5-5","usage":{"input_tokens":1000000,"output_tokens":1000000,"cache_creation_input_tokens":1000000,"cache_read_input_tokens":1000000,"cache_creation":{"ephemeral_1h_input_tokens":1000000,"ephemeral_5m_input_tokens":0}},"content":[{"type":"text","text":"oi"}]}}` + "\n"
	os.WriteFile(p, []byte(line+line), 0o644)
	tr := NewTranscript(p)
	if err := tr.Refresh(flat); err != nil {
		t.Fatal(err)
	}
	// 4 in + 20 out + 1h write 2×4 + 0.2 read = 32.2
	if math.Abs(tr.Cost-32.2) > 1e-9 || tr.Requests != 1 {
		t.Fatalf("cost=%v requests=%d", tr.Cost, tr.Requests)
	}
	// An appended half line is held until its newline arrives.
	f, _ := os.OpenFile(p, os.O_APPEND|os.O_WRONLY, 0)
	f.WriteString(`{"type":"ai-title","aiTitle":"gato"`)
	f.Close()
	tr.Refresh(flat)
	if tr.Title != "" {
		t.Fatal("parsed a partial line")
	}
	f, _ = os.OpenFile(p, os.O_APPEND|os.O_WRONLY, 0)
	f.WriteString("}\n")
	f.Close()
	tr.Refresh(flat)
	if tr.Title != "gato" || tr.LastText != "oi" {
		t.Fatalf("title=%q last=%q", tr.Title, tr.LastText)
	}
}

func TestEndedConversation(t *testing.T) {
	p := filepath.Join(t.TempDir(), "s.jsonl")
	os.WriteFile(p, []byte(`{"type":"assistant","requestId":"r1","timestamp":"2026-10-15T10:53:11Z","message":{"model":"claude-opus-5-5","content":[{"type":"tool_use","id":"t1","name":"EndConversation","input":{}}]}}
{"type":"ended-by-model","timestamp":"2026-10-15T10:53:11Z"}
`), 0o644)
	tr := NewTranscript(p)
	tr.Refresh(flat)
	if !tr.Ended {
		t.Fatal("conversation should be ended")
	}
}
