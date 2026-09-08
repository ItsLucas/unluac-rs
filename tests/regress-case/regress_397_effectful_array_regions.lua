-- Original fixture: ordered effects and fixed call results in nested array constructors.
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = "v"})
local token_mt = {
    __concat = function(left, right)
        trace[#trace + 1] = "concat:" .. left.id .. ":" .. right.id
        return left.id .. "/" .. right.id
    end
}

function Region397Value(n)
    assert(Region397Rows == "old")
    trace[#trace + 1] = "value:" .. n
    collectgarbage("collect")
    trace[#trace + 1] = weak[1] and "root:live" or "root:dead"
    if n % 2 == 0 then return nil, "discarded" end
    return n, "discarded"
end

function Region397Suffix(n)
    trace[#trace + 1] = "suffix:" .. n
    local token = setmetatable({id = n}, token_mt)
    weak[1] = token
    return token
end

function Region397Object()
    return setmetatable({}, {
        __index = function(_, key)
            trace[#trace + 1] = "get:" .. key
            return setmetatable({id = 7}, token_mt)
        end
    })
end

function Region397Build()
    local object = Region397Object()
    Region397Rows = {
        -- region397 rows begin
        {Region397Value(1), "" .. object.name .. Region397Suffix(11), 9, (Region397Value(1))},
        {Region397Value(2), "" .. object.name .. Region397Suffix(12), 9, (Region397Value(2))},
        -- region397 rows end
    }
end

Region397Rows = "old"
Region397Build()
for i, row in ipairs(Region397Rows) do
    assert(row[2] == "7/" .. (10 + i) and row[3] == 9 and row[5] == nil)
    assert(row[1] == row[4])
end
print(#Region397Rows, table.concat(trace, "|"))
