-- Relational source direction preserves producer slots and constant interning order.
-- unluac: expect-not-contains [[unluac error]]
local weak = setmetatable({}, {__mode = "v"})
local trace = {}
local current = 18
local function event(label)
    collectgarbage("collect")
    trace[#trace + 1] = label .. ":" .. (weak.retained and "live" or "dead")
end
function Frame440Retained()
    event("retain")
    local value = {}
    weak.retained = value
    return value
end
function Frame440Call(value, label)
    event("call:" .. tostring(value) .. ":" .. tostring(label))
    return current
end
function Frame440EqCall(value) event("eqcall"); return value end
Frame440Field = setmetatable({}, {__index = function(_, key) event("field:" .. key); return 17 end})
function Frame440Use(rows)
    event("use")
    assert(#rows == 2)
    trace[#trace + 1] = tostring(rows[1]) .. ":" .. tostring(rows[2])
end
function Frame440Build(left, right)
    local retained = Frame440Retained()
    if 17 < Frame440Call(left) then
        local flags = {left, right}
        Frame440Use(flags)
    end
    local rows = {left, right}
    Frame440Use(rows)
end
setfenv(Frame440Build, setmetatable({}, {
    __index = function(_, key) event("env:" .. key); return _G[key] end,
}))
current = 18
Frame440Build(18, 1)
weak = setmetatable({}, {__mode = "v"})
current = 16
Frame440Build(16, 2)
weak = setmetatable({}, {__mode = "v"})
current = 17
Frame440Build(17, 3)
print(table.concat(trace, "|"))
