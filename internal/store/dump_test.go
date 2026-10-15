package store

import (
	"fmt"
	"os"
	"strings"
	"testing"

	"github.com/aqilputi/neko/internal/config"
)

// TestDump prints the live session list (NEKO_DUMP=1); NEKO_DUMP=names also
// prints where each label comes from and the subagents.
func TestDump(t *testing.T) {
	if os.Getenv("NEKO_DUMP") == "" {
		t.Skip()
	}
	cfg, _ := config.Load()
	for _, s := range New(cfg).Refresh().Sessions {
		if q := os.Getenv("NEKO_DUMP_Q"); q != "" && !strings.Contains(strings.ToLower(s.Label()+s.Context), q) {
			continue
		}
		if os.Getenv("NEKO_DUMP") == "names" {
			fmt.Printf("S %-40.40q name=%q title=%q short=%s kind=%s user=%.60q\n", s.Label(), s.Name, s.Title, s.ShortID, s.Kind, s.LastUser)
			for _, sa := range s.Subs {
				fmt.Printf("   sub id=%s desc=%q type=%s\n", sa.ID, sa.Description, sa.Type)
			}
			continue
		}
		fmt.Printf("%-45.45s kind=%-11s state=%-8s attn=%d stale=%v last=%s ctx=%s\n", s.Label(), s.Kind, s.State, s.Attention, s.Stale, s.LastAt.Format("02/01 15:04"), s.Context)
	}
}
