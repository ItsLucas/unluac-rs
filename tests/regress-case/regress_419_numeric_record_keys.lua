-- 数值记录键仍按字段写次数分配，保持嵌套 SETLIST、调用和旧对象覆盖顺序。
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = "v"})
local calls = 0
function Numeric419Load(value)
    return {}, {}
end
function Numeric419Nil()
    return nil, 77
end
function Numeric419Call(index, count)
    collectgarbage("collect")
    calls = calls + 1
    trace[#trace + 1] = "call:" .. index .. ":" .. (weak[calls - 1] and "live" or "dead")
    if Numeric419Fail == calls then error("numeric-key-stop", 0) end
    local value = {index = index, count = count}
    weak[calls] = value
    return value, "discarded"
end
function Numeric419Build()
    Numeric419Load("prefix")
    Numeric419Root = {
        [10] = {n = 1, tShow = {{text = Numeric419Call(1, 2), ids = {Numeric419Nil(), 2, 3}}}},
        [20] = {n = 2, tShow = {{text = Numeric419Call(2, 3), ids = {4, Numeric419Nil(), 6}}}},
        [-10] = {n = 3, tShow = {{text = Numeric419Call(3, 4), ids = {7, (Numeric419Nil()), 8}}}},
        [0.25] = {n = 4, tShow = {{text = Numeric419Call(4, 5), ids = {Numeric419Nil(), Numeric419Nil(), 9}}}},
        [1] = true,
        [2] = false,
        [3] = nil,
        label = "ready",
    }
end
Numeric419Root = "old"
Numeric419Build()
assert(Numeric419Root[10].tShow[1].text.index == 1)
assert(Numeric419Root[20].tShow[1].text.count == 3)
assert(Numeric419Root[-10].tShow[1].ids[2] == nil)
assert(Numeric419Root[0.25].n == 4)
assert(Numeric419Root[1] and Numeric419Root[2] == false and Numeric419Root[3] == nil)
assert(Numeric419Root.label == "ready")
print(#Numeric419Root, #Numeric419Root[10].tShow[1].ids, #Numeric419Root[20].tShow[1].ids)
print(#Numeric419Root[-10].tShow[1].ids, #Numeric419Root[0.25].tShow[1].ids, table.concat(trace, "|"))
for fail = 1, 4 do
    calls = 0
    trace = {}
    weak = setmetatable({}, {__mode = "v"})
    Numeric419Root = "old"
    Numeric419Fail = fail
    local ok, err = pcall(Numeric419Build)
    assert(not ok and err == "numeric-key-stop" and Numeric419Root == "old")
    print(fail, calls, table.concat(trace, "|"))
end
