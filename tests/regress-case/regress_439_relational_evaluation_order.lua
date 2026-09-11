-- 输出排版不得交换字段读取与调用；两者可修改状态、触发 GC 或抛出异常。
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = 'v'})
local serial = 0
local failure = 0
local calls = 0
local mt = {}
local function event(label)
    collectgarbage('collect')
    calls = calls + 1
    trace[#trace + 1] = label .. ':' .. (weak.field and 'live' or 'nil')
    if calls == failure then error('pretty439-stop', 0) end
end
mt.__lt = function(a, b) event('lt'); return a.value < b.value end
mt.__le = function(a, b) event('le'); return a.value <= b.value end
Pretty439Object = setmetatable({}, {
    __index = function(_, key)
        event('field:' .. key)
        local value = setmetatable({value = serial}, mt)
        weak.field = value
        return value
    end,
})
function Pretty439Read(amount)
    event('call')
    serial = serial + amount
    return setmetatable({value = serial}, mt)
end
function Pretty439Build(object, amount)
    local rows = {{nil, 2, 3}}
    if object.value < Pretty439Read(amount) then rows[1] = {1, 2} end
    if object.value <= Pretty439Read(amount) then rows[2] = {3, 4} end
    return rows
end
collectgarbage('stop')
for amount = -1, 1 do
    for fail = 0, 6 do
        calls = 0
        failure = fail
        local ok, result = pcall(Pretty439Build, Pretty439Object, amount)
        if ok then print(amount, fail, #result, result[1][1], result[2] and result[2][1])
        else assert(result == 'pretty439-stop'); print(amount, fail, 'exception') end
    end
end
print(table.concat(trace, '|'))
