local weak = setmetatable({}, {__mode = "v"})
local trace = {}
local token = 17
local mode = 5
local failure = 0
local actor = {}

function Frame424Produce(a, b)
    trace[#trace + 1] = "produce:" .. a .. ":" .. b
    if failure == 1 then error("fixture-stop", 0) end
    local object = {}
    weak.scratch = object
    return object
end
function Frame424Consume(...)
    trace[#trace + 1] = "consume"
    if failure == 2 then error("fixture-stop", 0) end
end
function Frame424Get(id)
    trace[#trace + 1] = "get:" .. id
    if mode == 0 then return nil end
    return actor
end
actor.unbind = function() trace[#trace + 1] = "unbind" end
actor.close = function() trace[#trace + 1] = "close" end
actor.dynamic = function() trace[#trace + 1] = "dynamic"; return 1468 end
actor.setdynamic = function(n) trace[#trace + 1] = "setdynamic:" .. n end
actor.scene = function()
    trace[#trace + 1] = "scene"
    if mode == 1 then return nil end
    local scene = {}
    weak.scene = scene
    return scene
end
actor.quest = function(id)
    trace[#trace + 1] = "quest:" .. id
    if mode == 2 then return false end
    return 41
end
actor.phase = function(id)
    trace[#trace + 1] = "phase:" .. id
    if mode == 3 then return 2 end
    return 1
end
actor.id = 73
actor.del = function(id, n) trace[#trace + 1] = "del:" .. id .. ":" .. n end
actor.setquest = function(id, n, value)
    trace[#trace + 1] = "setquest:" .. id .. ":" .. n .. ":" .. value
end
Frame424Clock = {fps = 30}
Frame424Library = {
    correct = function(player)
        trace[#trace + 1] = "correct"
        return mode ~= 4
    end,
    observe = function(player, number, rows)
        collectgarbage("collect")
        collectgarbage("collect")
        trace[#trace + 1] = "observe:" .. #rows .. ":" .. #rows[9]
            .. ":" .. tostring(rows[9][2]) .. ":" .. (weak.scene and "scene-live" or "scene-dead")
            .. ":" .. (weak.scratch and "scratch-live" or "scratch-dead")
        assert(weak.scene ~= nil and weak.scratch == nil)
    end,
    remove = function(player, n) trace[#trace + 1] = "remove:" .. n end,
    create = function(player, n) trace[#trace + 1] = "create:" .. n end,
}

function Frame424Build(id, p1, p2, p3, p4, p5, p6, p7, p8, p9)
    local player = Frame424Get(id)
    if not player then return end
    player.unbind()
    player.close()
    if player.dynamic() == 1468 then player.setdynamic(0) end
    local scene = player.scene()
    if not scene then return end
    local quest = player.quest(token)
    local phase = player.phase(token)
    if quest and phase == 1 then
        if Frame424Library.correct(player) then
            Frame424Consume(player.id, "fixture", 8, 16, Frame424Clock.fps * 2,
                {0, 0, 0}, false, true, {{text = Frame424Produce(1, 2), font = 13}}, false)
            local rows = {{17, true}, {83, true}, {84, true}, {85, true}, {86, true},
                {87, true}, {88, true}, {89, true}, {90, true}, {91, true}, {92, true}}
            Frame424Library.observe(player, 802, rows)
            Frame424Library.remove(player, 31061)
            Frame424Library.create(player, 31061)
            player.del(34217, 1)
            player.setquest(quest, 0, 0)
            player.setquest(quest, 1, 0)
            player.setquest(quest, 2, 0)
        end
    end
end

collectgarbage("stop")
for m = 0, 5 do
    mode = m
    trace[#trace + 1] = "mode:" .. m
    Frame424Build(77)
end
for f = 1, 2 do
    failure = f
    local ok, err = pcall(Frame424Build, 78)
    assert(not ok and err == "fixture-stop")
    trace[#trace + 1] = "error:" .. f
end
print(table.concat(trace, ","))
