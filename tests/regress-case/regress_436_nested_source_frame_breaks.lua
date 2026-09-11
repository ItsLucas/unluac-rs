-- 嵌套循环退出只结束当前循环，保留正文对象和被覆盖临时槽的 GC 时点。
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = 'v'})
local failure = 0
function Break436Make(index)
    local object = {index = index}
    weak.object = object
    return object
end
function Break436Observe(object, index)
    collectgarbage('collect')
    assert(weak.object == object)
    trace[#trace + 1] = 'observe:' .. object.index .. ':' .. index
    if failure == object.index * 10 + index then error('break436-stop', 0) end
    local scratch = {}
    weak.scratch = scratch
    return scratch
end
function Break436After()
    collectgarbage('collect')
    collectgarbage('collect')
    trace[#trace + 1] = 'after:' .. (weak.object and 'object' or 'nil') .. ':' .. (weak.scratch and 'scratch' or 'nil')
end
function Break436Build(rows, stop)
    local result = {}
    for key, value in ipairs(rows) do
        local object = Break436Make(key)
        local count = 0
        for index = 1, 3 do
            if index == stop then break end
            Break436Observe(object, index)
            count = count + 1
        end
        result[#result + 1] = {key, count}
        if value == stop then break end
    end
    Break436After()
    return result
end
collectgarbage('stop')
for stop = 0, 4 do
    local result = Break436Build({1, 2, 3}, stop)
    for _, row in ipairs(result) do print(stop, row[1], row[2]) end
end
for _, point in ipairs({12, 21, 32}) do
    failure = point
    local ok, err = pcall(Break436Build, {1, 2, 3}, 0)
    assert(not ok and err == 'break436-stop')
    print('exception', point)
end
print(table.concat(trace, '|'))
