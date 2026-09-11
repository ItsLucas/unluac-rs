-- 分支臂内部可直接跳到父臂的已证出口，不能执行被越过的 elseif 或后继作用域。
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = 'v'})
local failure = 0
local count = 0
function Arm438Test(flags, index)
    collectgarbage('collect')
    count = count + 1
    trace[#trace + 1] = 'test:' .. index .. ':' .. (weak.object and 'live' or 'nil')
    if count == failure then error('arm438-stop', 0) end
    return flags[index]
end
function Arm438Make()
    local object = {}
    weak.object = object
    return object
end
function Arm438Observe(object, tag)
    collectgarbage('collect')
    trace[#trace + 1] = 'observe:' .. tag .. ':' .. (weak.object and 'live' or 'nil')
end
function Arm438Build(flags, tag)
    local rows = {{nil, 2, 3}}
    if Arm438Test(flags, 1) and Arm438Test(flags, 2) then
        if Arm438Test(flags, 3) then
            local object = Arm438Make()
            if Arm438Test(flags, 4) then
                Arm438Observe(object, 'inner-true')
                return rows
            else
                Arm438Observe(object, 'inner-false')
                return rows
            end
        end
    elseif Arm438Test(flags, 5) then
        Arm438Observe(rows, 'elseif')
    else
        Arm438Observe(rows, 'else')
    end
    Arm438Observe(rows, tag)
    return rows
end
collectgarbage('stop')
for mask = 0, 31 do
    local flags = {}
    for index = 1, 5 do flags[index] = math.floor(mask / 2 ^ (index - 1)) % 2 == 1 end
    for fail = 0, 5 do
        count = 0
        failure = fail
        local ok, result = pcall(Arm438Build, flags, 'tail')
        if ok then print(mask, fail, #result, #result[1])
        else assert(result == 'arm438-stop'); print(mask, fail, 'exception') end
    end
end
print(table.concat(trace, '|'))
