-- Example ~/.config/nekuroddo/init.lua (plan §5). Loaded by TestExampleInit
-- against app fakes.

-- 1. Auto-group: every session working under a svc-* directory joins #svc.
local function in_group(id, tag)
  for _, t in ipairs(nk.group.of(id)) do
    if t == tag then return true end
  end
  return false
end

nk.on("snapshot.changed", function()
  local ids = {}
  for _, s in ipairs(nk.sessions()) do
    if s.cwd:find("/svc%-") and not in_group(s.id, "svc") then
      ids[#ids + 1] = s.id
    end
  end
  if #ids > 0 then nk.group.add("svc", ids) end
end)

-- 2. A peer message about #deploy becomes a desktop notification (or a
-- toast, on a client without the "notify" capability).
nk.on("peer.message", function(ev)
  if ev.text:find("[#deploy", 1, true) then
    local msg = "deploy: " .. ev.from .. ": " .. ev.text
    if nk.ui.has("notify") then nk.ui.notify(msg) else nk.ui.toast(msg) end
  end
end)

-- 3. deploy-all: four agents, one per service, grouped in #deploy.
nk.command("deploy-all", function()
  local ids = {}
  for _, dir in ipairs({ "~/dev/svc-a", "~/dev/svc-b", "~/dev/svc-c", "~/dev/svc-d" }) do
    ids[#ids + 1] = nk.dispatch{ dir = dir, prompt = "Suba em staging e confirme o healthcheck.", perm = "acceptEdits" }
  end
  nk.group.add("deploy", ids)
  nk.ui.toast(("deploy-all: %d agents em #deploy"):format(#ids))
end, { doc = "dispara os deploys e agrupa #deploy", keys = { "ctrl+shift+d" } })
