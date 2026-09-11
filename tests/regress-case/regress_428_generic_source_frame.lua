local trace = {}
local weak = setmetatable({}, {__mode = "v"})
local limit = 4
local failure = 0
local records = {
    {id = 11, kind = 1}, {id = 12, kind = 2},
    {id = 13, kind = 1}, {id = 14, kind = 1},
}
Frame428Defs = setmetatable({}, {
    __index = function(_, key)
        collectgarbage("collect")
        trace[#trace + 1] = "index:" .. key .. ":" .. (weak.iterator and "live" or "dead")
        return records[key]
    end,
})
function Frame428NextDefs(state, control)
    local key = control + 1
    trace[#trace + 1] = "next:" .. key
    if failure == 3 and key == 3 then error("fixture428-stop", 0) end
    if key > limit then return nil end
    local value = {}
    weak.iterator = value
    return key, value
end
function Frame428NextRows(state, control)
    local key = control + 1
    if key > #state then return nil end
    return key, state[key]
end
function Frame428Pairs(state)
    if state == Frame428Defs then return Frame428NextDefs, state, 0 end
    return Frame428NextRows, state, 0
end
function Frame428Observe(value)
    collectgarbage("collect")
    collectgarbage("collect")
    assert(weak.iterator == nil)
    trace[#trace + 1] = "output:" .. value
end
local player = {id = 73, name = "hero"}
player.level = function(id)
    collectgarbage("collect")
    assert(weak.iterator ~= nil)
    trace[#trace + 1] = "level:" .. id
    if failure == 1 and id == 13 then error("fixture428-stop", 0) end
    if id == 13 then return 0 end
    return id - 9
end
player.remove = function(id)
    trace[#trace + 1] = "remove:" .. id
    if failure == 2 and id == 14 then error("fixture428-stop", 0) end
end

function Frame428Build(player, kind)
    local count = 0
    local rows = {}
    for key, value in Frame428Pairs(Frame428Defs) do
        local level = player.level(Frame428Defs[key].id)
        if kind == Frame428Defs[key].kind and 0 < level then
            player.remove(Frame428Defs[key].id)
            count = count + 1
            rows[#rows + 1] = {Frame428Defs[key].id, level}
        end
    end
    local message = "result:" .. player.id .. ":" .. player.name .. ":"
    for key, value in Frame428Pairs(rows) do
        message = message .. value[1] .. ":" .. value[2] .. ";"
    end
    message = message .. "."
    Frame428Observe(message)
    return count
end

collectgarbage("stop")
limit = 0
assert(Frame428Build(player, 1) == 0)
limit = 4
assert(Frame428Build(player, 1) == 2)
assert(Frame428Build(player, 2) == 1)
for f = 1, 3 do
    failure = f
    local ok, err = pcall(Frame428Build, player, 1)
    assert(not ok and err == "fixture428-stop")
    trace[#trace + 1] = "error:" .. f
end
print(table.concat(trace, ","))
