-- Canonical pairs loops retain their enclosing source frame and iterator protocol.
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local step = 0
local fail_at = 0
local function event(name)
    collectgarbage("collect")
    step = step + 1
    trace[#trace + 1] = name
    if step == fail_at then error("loop425-stop", 0) end
end
function Loop425Load(value) event("load:" .. value) end
function Loop425Text(index)
    event("text:" .. index)
    return {index = index}, "discarded"
end
function Loop425Build(marker)
    Loop425Load("prefix")
    function Loop425Read() return marker end
    Loop425First = {alpha = 2, beta = 4}
    Loop425Factor = 3
    for key, value in pairs(Loop425First) do
        Loop425First[key] = value * Loop425Factor
    end
    Loop425Second = {alpha = 0, beta = 0}
    for key, value in pairs(Loop425First) do
        Loop425Second[key] = value
    end
    Loop425Rows = {
        {label = Loop425Text(1), value = 11},
        {label = Loop425Text(2), value = 12},
    }
end
setfenv(Loop425Build, setmetatable({}, {
    __index = function(_, key) event("env:" .. key); return _G[key] end,
    __newindex = function(_, key, value) event("store:" .. key); _G[key] = value end,
}))
Loop425Rows = "old"
Loop425Build(425)
assert(Loop425Read() == 425)
assert(Loop425First.alpha == 6 and Loop425First.beta == 12)
assert(Loop425Second.alpha == 6 and Loop425Second.beta == 12)
assert(Loop425Rows[2].label.index == 2 and Loop425Rows[1].value == 11)
print(table.concat(trace, "|"))
for fail = 1, step do
    fail_at = fail
    step = 0
    trace = {}
    Loop425Rows = "old"
    local ok, err = pcall(Loop425Build, 425)
    assert(not ok and err == "loop425-stop")
    print(fail, step, type(Loop425Rows), table.concat(trace, "|"))
end
