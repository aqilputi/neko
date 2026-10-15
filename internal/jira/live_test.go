package jira

import (
	"os"
	"testing"
)

// TestLiveFetch hits Jira through claude -p (costs ~$0.1).
// NEKO_JIRA_LIVE=1 NEKO_JIRA_SITE=acme.atlassian.net [NEKO_JIRA_CLOUD_ID=<uuid>].
func TestLiveFetch(t *testing.T) {
	if os.Getenv("NEKO_JIRA_LIVE") == "" {
		t.Skip("set NEKO_JIRA_LIVE=1")
	}
	site := os.Getenv("NEKO_JIRA_SITE")
	if site == "" {
		t.Skip("set NEKO_JIRA_SITE")
	}
	c, err := Fetch(Config{Site: site, CloudID: os.Getenv("NEKO_JIRA_CLOUD_ID")}.WithDefaults())
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("%d issues, $%.3f", len(c.Issues), c.CostUSD)
	for _, i := range c.Issues[:min(3, len(c.Issues))] {
		t.Logf("%s [%s/%s] %s %s", i.Key, i.Status, i.StatusCat, i.Summary, i.URL)
	}
}
