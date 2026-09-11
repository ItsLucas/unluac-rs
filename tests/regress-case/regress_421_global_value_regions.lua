-- Atomic scalar/closure global statements preserve the next constructor's frame.
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = "v"})
local fail_at = 0
local step = 0
local function event(name)
    collectgarbage("collect")
    step = step + 1
    trace[#trace + 1] = name .. ":" .. (weak.group and "live" or "dead")
    if step == fail_at then error("prefix421-stop", 0) end
end
function Global421Load(value)
    event("load:" .. value)
    return {}, {}
end
Global421Catalog = setmetatable({}, {
    __index = function(_, key)
        event("catalog:" .. key)
        local group = setmetatable({}, {
            __index = function(_, field)
                event("group:" .. field)
                return 16
            end,
        })
        weak.group = group
        return group
    end,
})
function Global421Text(index, label)
    event("text:" .. label)
    return {index = index}, "discarded"
end
function Global421Build(captured)
    Global421Load("prefix")
    Global421Value = Global421Catalog.group.value
    Global421Number = 64
    Global421Arithmetic = 4 * Global421Number
    Global421Pair = Global421Number + Global421Arithmetic
    Global421Closure = function(value) return value + 1 end
    function Global421Captured(value) return captured + value end
    Global421Rows = {
        {label = Global421Text(1, "one"), value = 11},
        {label = Global421Text(2, "two"), value = 12},
    }
    Global421Nil = nil
    Global421Bool = true
end
setfenv(Global421Build, setmetatable({}, {
    __index = function(_, key)
        event("env:" .. key)
        return _G[key]
    end,
    __newindex = function(_, key, value)
        event("store:" .. key)
        _G[key] = value
    end,
}))
Global421Rows = "old"
Global421Build(17)
assert(Global421Value == 16 and Global421Pair == 320)
assert(Global421Closure(3) == 4 and Global421Captured(3) == 20)
assert(Global421Rows[2].label.index == 2 and Global421Rows[1].value == 11)
assert(Global421Nil == nil and Global421Bool == true)
print(table.concat(trace, "|"))
for fail = 1, step do
    fail_at = fail
    step = 0
    trace = {}
    Global421Rows = "old"
    local ok, err = pcall(Global421Build, 17)
    assert(not ok and err == "prefix421-stop")
    print(fail, step, type(Global421Rows), table.concat(trace, "|"))
end
