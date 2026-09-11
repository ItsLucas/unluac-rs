-- GETUPVAL 列表项按原序物化；嵌套数组与中途回调不能回读被更新后的 upvalue。
-- unluac: expect-not-contains [[unluac error]]
local first, second, third, fourth, fifth
local trace = {}
local weak = setmetatable({}, {__mode = "v"})
local calls = 0
function UpvalueItems433Box(value)
    local box = {n = value}
    weak[value] = box
    return box
end
local function observe(where)
    collectgarbage("collect")
    trace[#trace + 1] = where .. ":" .. (weak[2] and "live" or "dead") .. ":" .. (weak[3] and "live" or "dead")
end
function UpvalueItems433Pulse(where)
    calls = calls + 1
    observe(where)
    if calls == UpvalueItems433Fail then error("upvalue-items-stop", 0) end
    second = nil
    third = UpvalueItems433Box(20 + calls)
    fourth = third
    return first, "discarded"
end
function UpvalueItems433Build()
    local result = {
        first, UpvalueItems433Pulse("outer"), second, third,
        {first, second}, {second, UpvalueItems433Pulse("inner"), third},
        {third, fourth}, {fourth, fifth},
    }
    UpvalueItems433Root = result
end
function UpvalueItems433Reset()
    first = UpvalueItems433Box(1)
    second = UpvalueItems433Box(2)
    third = UpvalueItems433Box(3)
    fourth = UpvalueItems433Box(4)
    fifth = UpvalueItems433Box(5)
end
setfenv(UpvalueItems433Build, setmetatable({}, {
    __index = function(_, key)
        observe("get:" .. key)
        return _G[key]
    end,
    __newindex = function(_, key, value)
        observe("store:" .. key)
        _G[key] = value
    end,
}))
for fail = 0, 2 do
    weak = setmetatable({}, {__mode = "v"})
    UpvalueItems433Reset()
    trace = {}
    calls = 0
    UpvalueItems433Fail = fail
    UpvalueItems433Root = "old"
    local ok, err = pcall(UpvalueItems433Build)
    if fail == 0 then
        assert(ok and #UpvalueItems433Root == 8)
        assert(UpvalueItems433Root[1] == first and UpvalueItems433Root[3] == nil)
        assert(UpvalueItems433Root[4].n == 21 and UpvalueItems433Root[6][3].n == 22)
        print(#UpvalueItems433Root[5], #UpvalueItems433Root[6], UpvalueItems433Root[7][1].n)
    else
        assert(not ok and err == "upvalue-items-stop" and UpvalueItems433Root == "old")
    end
    print(fail, table.concat(trace, "|"))
end
