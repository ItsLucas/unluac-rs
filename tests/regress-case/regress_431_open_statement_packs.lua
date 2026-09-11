-- 开放尾表直接返回，以及开放参数调用与另一分支内开放数组之后的查表调用。
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = "v"})
Open431Target = {PLAYER = 7}
function Open431Text(group, index)
    collectgarbage("collect")
    trace[#trace + 1] = "text:" .. group .. ":" .. index .. ":" .. (weak.extra and "live" or "dead")
    if Open431Fail == index then error("open-frame-stop", 0) end
    if index == Open431TailIndex then
        local values = {}
        for item = 1, Open431Count do
            if item % 3 ~= 0 then values[item] = {index = item} end
        end
        return unpack(values, 1, Open431Count)
    end
    local value = {index = index}
    if index % 2 == 0 then value = nil end
    local extra = {extra = index}
    weak.extra = extra
    return value, extra, "discarded"
end
function Open431Sink(...)
    collectgarbage("collect")
    trace[#trace + 1] = "sink:" .. select("#", ...) .. ":" .. (weak.extra and "live" or "dead")
    for index = 1, select("#", ...) do
        trace[#trace + 1] = "arg:" .. index .. ":" .. type((select(index, ...)))
    end
end
function Open431Return()
    return {
        Open431Text(33, 1),
        Open431Text(33, 2),
        Open431Text(33, 3),
    }
end
function Open431Dispatch(actor, unused1, unused2, unused3)
    if actor.force == 0 then
        actor.message(Open431Text(1, 0))
    else
        local values = {
            Open431Text(1, 1),
            Open431Text(1, 2),
            Open431Text(1, 3),
            Open431Text(1, 4),
            Open431Text(1, 5),
        }
        actor.open(Open431Target.PLAYER, actor.id, values[actor.force], "first", "second")
    end
end
local actor = setmetatable({_force = 0, _id = 19}, {
    __index = function(self, key)
        collectgarbage("collect")
        trace[#trace + 1] = "get:" .. key .. ":" .. (weak.extra and "live" or "dead")
        if key == "force" then return self._force end
        if key == "id" then return self._id end
        return Open431Sink
    end,
})
for count = 0, 12 do
    Open431Count = count
    Open431TailIndex = 3
    trace = {}
    weak = setmetatable({}, {__mode = "v"})
    local values = Open431Return()
    assert(values[1].index == 1 and values[2] == nil)
    for index = 1, count do
        if index % 3 == 0 then assert(values[index + 2] == nil)
        else assert(values[index + 2].index == index) end
    end
    print("return", count, #values, table.concat(trace, "|"))
    for branch = 0, 2 do
        actor._force = branch
        Open431TailIndex = branch == 0 and 0 or 5
        trace = {}
        weak = setmetatable({}, {__mode = "v"})
        Open431Dispatch(actor, 100, 200, 300)
        print("dispatch", count, branch, table.concat(trace, "|"))
    end
end
Open431Count = 9
Open431TailIndex = 5
actor._force = 1
for fail = 1, 5 do
    trace = {}
    weak = setmetatable({}, {__mode = "v"})
    Open431Fail = fail
    local ok, err = pcall(Open431Dispatch, actor, 100, 200, 300)
    assert(not ok and err == "open-frame-stop")
    print("fail", fail, table.concat(trace, "|"))
end
