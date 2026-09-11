-- Shared false successors must preserve every short-circuit condition and else.
-- unluac: expect-not-contains [[unluac error]]
local weak = setmetatable({}, {__mode = "v"})
local trace = {}
local kind = 1
local function event(label)
    collectgarbage("collect")
    trace[#trace + 1] = label .. ":" .. (weak.retained and "live" or "dead")
end
function Frame435Retained()
    event("retain")
    local value = {}
    weak.retained = value
    return value
end
function Frame435Check(value) event("check:" .. value); return value end
Frame435Kind = setmetatable({}, {__index = function(_, key) event("kind"); return kind end})
function Frame435Use(label, values) event("use:" .. label); assert(#values == 2) end
function Frame435Build(left, right)
    local retained = Frame435Retained()
    if left == 1 and Frame435Check(right) == 1 and Frame435Kind.value == 1 then
        local row = {left, right}
        Frame435Use("first", row)
    elseif left == 2 and Frame435Check(right) == 1 then
        local row = {right, left}
        Frame435Use("second", row)
    else
        local row = {right, right}
        Frame435Use("other", row)
    end
    local final = {left, right}
    Frame435Use("final", final)
end
setfenv(Frame435Build, setmetatable({}, {
    __index = function(_, key) event("env:" .. key); return _G[key] end,
}))
Frame435Build(1, 1)
weak = setmetatable({}, {__mode = "v"})
Frame435Build(1, 2)
weak = setmetatable({}, {__mode = "v"})
Frame435Build(2, 1)
weak = setmetatable({}, {__mode = "v"})
Frame435Build(2, 2)
weak = setmetatable({}, {__mode = "v"})
Frame435Build(3, 1)
weak = setmetatable({}, {__mode = "v"})
kind = 2
Frame435Build(1, 1)
print(table.concat(trace, "|"))
