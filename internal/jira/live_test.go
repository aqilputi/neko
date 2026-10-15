package jira

import (
	"os"
	"testing"
)

// TestLiveFetch hits Jira through claude -p (costs ~$0.1). NEKO_JIRA_LIVE=1.
func TestLiveFetch(t *testing.T) {
	if os.Getenv("NEKO_JIRA_LIVE") == "" {
		t.Skip("set NEKO_JIRA_LIVE=1")
	}
	c, err := Fetch(Config{Site: "visio-ai.atlassian.net", CloudID: "099502f7-be3e-4a49-9c78-30e413a6170d"})
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("%d issues, $%.3f", len(c.Issues), c.CostUSD)
	for _, i := range c.Issues[:min(3, len(c.Issues))] {
		t.Logf("%s [%s/%s] %s %s", i.Key, i.Status, i.StatusCat, i.Summary, i.URL)
	}
}
