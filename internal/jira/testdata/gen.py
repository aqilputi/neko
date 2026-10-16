#!/usr/bin/env python3
"""Regenerates the canned `claude -p --output-format stream-json` outputs used
by jira_test.go. Run from internal/jira: python3 testdata/gen.py"""
import json

P = "mcp__claude_ai_Atlassian_Rovo__"


def ln(o):
    return json.dumps(o, ensure_ascii=False)


def init():
    return ln({"type": "system", "subtype": "init",
               "mcp_servers": [{"name": "claude.ai Atlassian Rovo", "status": "connected"}]})


def use(id, tool):
    return ln({"type": "assistant", "message": {"content": [
        {"type": "text", "text": "vou chamar"},
        {"type": "tool_use", "id": id, "name": P + tool, "input": {}}]}})


def res(id, content, err=False):
    return ln({"type": "user", "message": {"content": [
        {"type": "tool_result", "tool_use_id": id, "is_error": err, "content": content}]}})


def result(cost, text, denials=()):
    return ln({"type": "result", "subtype": "success", "total_cost_usd": cost,
               "result": text, "permission_denials": list(denials)})


def w(name, lines):
    with open("testdata/" + name, "w") as f:
        f.write("\n".join(lines) + "\n")


search = {"issues": {"nodes": [
    {"key": "ABC-1", "webUrl": "https://acme.atlassian.net/browse/ABC-1", "fields": {
        "summary": "Corrigir login", "description": "Passos:\n\n\n\n1\\. abrir \\[app\\]",
        "updated": "2026-10-15T09:30:00.000-0300",
        "status": {"name": "Em andamento", "statusCategory": {"key": "indeterminate"}},
        "issuetype": {"name": "Bug"}, "priority": {"name": "High"}}},
    {"key": "ABC-2", "fields": {
        "summary": "Sem link", "description": None, "updated": "",
        "status": {"name": "To Do", "statusCategory": {"key": "new"}},
        "issuetype": {"name": "Task"}, "priority": {"name": "Medium"}}}]}}

s = json.dumps(search)
half = len(s) // 2
# Non-JSON and broken lines must be skipped; the tool result comes as
# [{type:text,text}] parts that must be concatenated.
w("search_ok.jsonl", [init(), "not json at all", "{broken", use("t1", "searchJiraIssuesUsingJql"),
                      res("t1", [{"type": "text", "text": s[:half]}, {"type": "text", "text": s[half:]}]),
                      result(0.042, "ok")])
w("search_malformed.jsonl", [use("t1", "searchJiraIssuesUsingJql"),
                             res("t1", "<html>502 Bad Gateway</html>"), result(0.01, "ok")])
w("create_ok.jsonl", [use("c1", "createJiraIssue"),
                      res("c1", json.dumps({"id": "10001", "key": "ABC-42", "self": "https://x"})),
                      result(0.03, "ABC-42")])
w("create_text_key.jsonl", [use("c1", "createJiraIssue"),
                            res("c1", "Issue created: ABC-77 (see link)"), result(0.03, "ABC-77")])
w("create_no_key.jsonl", [use("c1", "createJiraIssue"), res("c1", "{\"id\":\"10001\"}"),
                          result(0.03, "done")])
# A PreToolUse hook blocked the write: is_error tool_result + permission_denials.
w("create_denied_hook.jsonl", [use("c1", "createJiraIssue"),
                               res("c1", "PreToolUse:mcp hook error: Jira writes are blocked.\n"
                                         "Run jira-allow to open a window.", True),
                               result(0.02, "I could not create it.",
                                      [{"tool_name": P + "createJiraIssue", "tool_use_id": "c1", "tool_input": {}}])])
# Denied before any tool_result was emitted: the reason is the final text.
w("denied_no_tool_result.jsonl", [use("c1", "createJiraIssue"),
                                  result(0.01, "Permission   denied\nfor createJiraIssue",
                                         [{"tool_name": P + "createJiraIssue"}])])
# Jira itself rejected the call: is_error but no permission denial.
w("jira_error.jsonl", [use("c1", "createJiraIssue"),
                       res("c1", "Field 'summary' is required", True), result(0.02, "erro")])
# The model called another tool; its result must not be mistaken for ours.
w("wrong_tool.jsonl", [use("x1", "getVisibleJiraProjects"), res("x1", "[]"),
                       result(0.01, "não consegui chamar a ferramenta")])
w("no_tool.jsonl", [init(), result(0.0, "O conector Atlassian não está disponível.")])
