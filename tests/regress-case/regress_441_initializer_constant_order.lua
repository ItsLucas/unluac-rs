-- 独立声明不能提前插入调用里的常量；赋值父键则在整个 RHS 之前插入。
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
function Pool441Sink(value, extra)
    trace[#trace + 1] = 'call:' .. value .. ':' .. tostring(extra)
    return extra or value
end
Pool441Fresh = 40000
function Pool441Build(object, tag)
    local rows = {{nil, 2, 3}}
    Pool441Sink = 31415 < Pool441Sink(Pool441Fresh)
    return rows
end
local object = {method = Pool441Sink}
local rows = Pool441Build(object, 0)
assert(Pool441Sink == true)
print(#rows, #rows[1], table.concat(trace, '|'))
