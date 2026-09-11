-- Numeric-for hidden controls and body locals keep their original source slots.
-- unluac: expect-not-contains [[unluac error]]
local weak = setmetatable({}, {__mode = "v"})
local trace = {}
local function event(label)
    collectgarbage("collect")
    trace[#trace + 1] = label .. ":" .. (weak.emit and "emit-live" or "emit-dead")
        .. ":" .. (weak.target and "target-live" or "target-dead")
end
Frame432Kind = {ACTIVE = 1}
function Frame432Player(value)
    event("player")
    if value == 0 then return nil end
    return {value = value}
end
function Frame432Scene(value)
    event("scene")
    if value == 0 then return nil end
    return setmetatable({}, {
        __index = function(_, key)
            event("field:" .. key)
            if key == "kind" then return value end
            if key == "id" then return 10 end
            if key == "map" then return 20 end
            if key == "x" then return 11 end
            if key == "y" then return 22 end
            if key == "z" then return 33 end
            if key == "owner" then
                return function() event("owner"); return {id = 44} end
            end
            if key == "emit" then
                local callback = function(id, map, index, count)
                    event("emit:" .. index)
                    assert(id == 10 and map == 20 and count == 1)
                end
                weak.emit = callback
                return callback
            end
            if key == "lookup" then
                return function(index, count)
                    event("lookup:" .. index)
                    assert(count == 1)
                    if index == 102 then return nil end
                    local target = setmetatable({}, {
                        __newindex = function(_, key, value)
                            event("store:" .. index .. ":" .. key .. ":" .. value)
                        end,
                    })
                    weak.target = target
                    return target
                end
            end
            error("unexpected scene key", 0)
        end,
    })
end
function Frame432Sink(player, coords)
    event("sink")
    assert(player.value == 1 and #coords == 4 and coords[4] == 44)
end
function Frame432Build(left, right)
    local player = Frame432Player(left)
    if not player then return end
    local scene = Frame432Scene(right)
    if not scene then return end
    if scene.kind == 1 then
        for index = 1, 4 do
            scene.emit(scene.id, scene.map, 100 + index, 1)
        end
        local coords = {scene.x, scene.y, scene.z, scene.owner().id}
        for index = 1, 4 do
            local target = scene.lookup(100 + index, 1)
            if target then
                target.value = coords[index]
            end
        end
        Frame432Sink(player, coords)
    end
end
setfenv(Frame432Build, setmetatable({}, {
    __index = function(_, key) event("env:" .. key); return _G[key] end,
}))
Frame432Build(1, 1)
weak = setmetatable({}, {__mode = "v"})
Frame432Build(0, 1)
weak = setmetatable({}, {__mode = "v"})
Frame432Build(1, 0)
weak = setmetatable({}, {__mode = "v"})
Frame432Build(1, 2)
print(table.concat(trace, "|"))
