local weak = setmetatable({}, {__mode = "v"})
local events = {}

function Statement422Produce(a, b)
    local object = {}
    weak.object = object
    return object
end

function Statement422Consume(...) end

function Statement422Observe(rows, keep)
    collectgarbage("collect")
    collectgarbage("collect")
    events[#events + 1] = (weak.object and "live" or "dead") .. ":" .. #rows
        .. ":" .. #rows[9] .. ":" .. tostring(rows[9][2]) .. ":" .. keep
end

function Statement422Build(flag, keep)
    if flag then
        Statement422Consume(keep, 3, 4, 5, 6, {0, 0, 0}, false, true,
            {{value = Statement422Produce(1, 2), font = 13}}, false)
        local rows = {{1, true}, {2, true}, {3, true}, {4, true}, {5, true},
            {6, true}, {7, true}, {8, true}, {9, true}, {10, true}, {11, true}}
        Statement422Observe(rows, keep)
    end
end

collectgarbage("stop")
Statement422Build(false, 41)
Statement422Build(true, 42)
Statement422Build(true, 43)
assert(table.concat(events, ",") == "dead:11:2:true:42,dead:11:2:true:43")
print(table.concat(events, ","))
