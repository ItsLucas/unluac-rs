-- Fixed multi-results retain unused slots, nil padding, and native iterator width.
-- unluac: expect-not-contains [[unluac error]]
local weak = setmetatable({}, {__mode = "v"})
local trace = {}
local mode = 1
local function event(label)
    collectgarbage("collect")
    trace[#trace + 1] = label .. ":" .. (weak.pair and "pair-live" or "pair-dead")
        .. ":" .. (weak.four and "four-live" or "four-dead")
        .. ":" .. (weak.three and "three-live" or "three-dead")
end
function Frame434Player(value)
    event("player")
    if value == 0 then return nil end
    return {value = value, owner = function()
        event("owner")
        local unused = {}
        weak.pair = unused
        if mode == 2 then return unused end
        return unused, 1
    end}
end
function Frame434Scene(value)
    event("scene")
    if value == 0 then return nil end
    return {value = value}
end
function Frame434Count() event("count"); return 2 end
function Frame434Tick(index) event("tick:" .. index) end
function Frame434Warn() event("warn") end
function Frame434Args(value) event("args"); return value, 99 end
function Frame434Coords(player)
    event("coords")
    assert(player.value == 1)
    local unused = {}
    weak.four = unused
    if mode == 3 then return unused end
    return unused, 11, 22, 33
end
function Frame434Triple()
    event("triple")
    local unused = {}
    weak.three = unused
    if mode == 3 then return unused end
    return unused, 44, 55
end
function Frame434Pairs(rows) event("pairs"); return pairs(rows) end
function Frame434Use(key, value) event("use:" .. key); assert(type(value) == "table") end
function Frame434Sink(player, rows, x, y, z, a, b)
    event("sink")
    assert(player.value == 1 and #rows == 2)
    assert(rows[1][1] == x and rows[1][2] == y)
    assert(rows[2][1] == z and rows[2][2] == a and rows[2][3] == b)
end
function Frame434Build(left, right)
    local player = Frame434Player(left)
    if not player then return end
    local scene = Frame434Scene(right)
    if not scene then return end
    local unusedPair, owner = player.owner()
    if owner ~= left then Frame434Warn(); return end
    local count = Frame434Count()
    for index = 1, count do Frame434Tick(index) end
    local unusedFour, x, y, z = Frame434Coords(player)
    local unusedThree, a, b = Frame434Triple()
    local rows = {{x, y}, {z, a, b}}
    for key, value in Frame434Pairs(rows) do Frame434Use(key, value) end
    Frame434Sink(player, rows, x, y, z, a, b)
end
setfenv(Frame434Build, setmetatable({}, {
    __index = function(_, key) event("env:" .. key); return _G[key] end,
}))
Frame434Build(1, 1)
weak = setmetatable({}, {__mode = "v"})
mode = 2
Frame434Build(1, 1)
weak = setmetatable({}, {__mode = "v"})
mode = 3
Frame434Build(1, 1)
weak = setmetatable({}, {__mode = "v"})
Frame434Build(0, 1)
weak = setmetatable({}, {__mode = "v"})
Frame434Build(1, 0)
print(table.concat(trace, "|"))
