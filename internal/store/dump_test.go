package store

import (
	"fmt"
	"os"
	"strings"
	"testing"

	"github.com/aqilputi/neko/internal/config"
)

// TestDump prints the live session list (NEKO_DUMP=1).
func TestDump(t *testing.T) {
	if os.Getenv("NEKO_DUMP") == "" {
		t.Skip()
	}
	cfg, _ := config.Load()
	for _, s := range New(cfg).Refresh().Sessions {
		if q := os.Getenv("NEKO_DUMP_Q"); q != "" && !strings.Contains(strings.ToLower(s.Label()+s.Context), q) {
			continue
		}
		fmt.Printf("%-45.45s kind=%-11s state=%-8s attn=%d stale=%v last=%s ctx=%s\n", s.Label(), s.Kind, s.State, s.Attention, s.Stale, s.LastAt.Format("02/01 15:04"), s.Context)
	}
}
