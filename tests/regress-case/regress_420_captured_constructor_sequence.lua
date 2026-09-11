-- 连续捕获声明保留源码槽位；早先临时槽被后来的捕获根复用时按定义区分。
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = 'v'})
function Seq420Load(value) return {}, {} end
function Seq420Make(value)
    collectgarbage('collect')
    trace[#trace + 1] = value .. ':' .. (weak.last and 'live' or 'dead')
    if Seq420Fail == value then error('sequence-stop', 0) end
    local result = {value = value}
    weak.last = result
    return result, nil
end
function Seq420Build()
    Seq420Load('prefix')
    Seq420Seed = {values = {1, 2, 3}}
    local count = 2
    local first = {[10] = {value = 2}, [20] = {value = 3}}
    local second = {
        [10] = {rows = {{name = Seq420Make(1), nums = {nil, 2, 3}, flags = {true, false}}}},
        [20] = {rows = {{name = Seq420Make(2), nums = {4, nil, 6}, flags = {false, true}}}},
    }
    function Seq420Read() return count, first, second end
    function Seq420Write(value) second = value end
end
Seq420Build()
local n, a, b = Seq420Read()
assert(n == 2 and a[10].value == 2 and b[20].rows[1].name.value == 2)
assert(b[10].rows[1].flags[1] and not b[20].rows[1].flags[1])
print(#b[10].rows[1].nums, #b[20].rows[1].nums, table.concat(trace, '|'))
Seq420Write({})
local _, _, c = Seq420Read()
assert(c ~= b)
local old = Seq420Read
for fail = 1, 2 do
    Seq420Fail = fail
    local ok, err = pcall(Seq420Build)
    assert(not ok and err == 'sequence-stop' and Seq420Read == old)
end
print('sequence-ok')
