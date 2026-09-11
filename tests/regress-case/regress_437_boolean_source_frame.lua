-- Materialized comparison values keep false/true writes and pinned destination slots.
-- unluac: expect-not-contains [[unluac error]]
local weak = setmetatable({}, {__mode = "v"})
local trace = {}
local function event(label)
    collectgarbage("collect")
    trace[#trace + 1] = label .. ":" .. (weak.retained and "live" or "dead")
end
function Frame437Retained()
    event("retain")
    local value = {}
    weak.retained = value
    return value
end
function Frame437Read(value) event("read:" .. value); return value end
Frame437Kind = setmetatable({}, {__index = function(_, key)
    event("key:" .. key)
    if key == "value" then return 1 end
    return 2
end})
function Frame437Use(label, rows)
    event("use:" .. label)
    for index = 1, #rows do assert(type(rows[index]) == "boolean") end
    trace[#trace + 1] = tostring(rows[1]) .. ":" .. tostring(rows[2])
end
function Frame437Build(left, right)
    local retained = Frame437Retained()
    local ready = Frame437Read(left) == Frame437Kind.value
    if ready then
        local flags = {ready, ready}
        Frame437Use("ready", flags)
    end
    local ordered = left <= right
    ready = Frame437Read(right) ~= Frame437Kind.value
    local greater = Frame437Read(left) < Frame437Kind.limit
    local same = left == right
    local rows = {ready, ordered, greater, same}
    Frame437Use("final", rows)
end
setfenv(Frame437Build, setmetatable({}, {
    __index = function(_, key) event("env:" .. key); return _G[key] end,
}))
Frame437Build(1, 1)
weak = setmetatable({}, {__mode = "v"})
Frame437Build(1, 2)
weak = setmetatable({}, {__mode = "v"})
Frame437Build(2, 1)
print(table.concat(trace, "|"))
