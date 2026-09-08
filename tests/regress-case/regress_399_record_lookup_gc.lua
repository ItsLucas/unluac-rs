-- Scratch lookup objects must not become extra persistent source locals.
-- unluac: expect-not-contains [[unluac error]]
local weak = setmetatable({}, {__mode = "v"})
local trace = {}
local serial = 0
local leaf_mt = {
    __index = function(self, key)
        trace[#trace + 1] = "value:" .. self.id .. ":" .. key
        return self.id
    end
}
Record399Catalog = setmetatable({}, {
    __index = function(_, key)
        collectgarbage("collect")
        trace[#trace + 1] = weak[serial] and "previous:live" or "previous:dead"
        serial = serial + 1
        local value = setmetatable({id = serial}, leaf_mt)
        weak[serial] = value
        return value
    end
})
function Record399Load()
    collectgarbage("collect")
    return {}, {}
end
function Record399Build()
    Record399Load()
    Record399Rows = {
        {a = Record399Catalog.group.a, padding = 0, b = Record399Catalog.group.b},
        {a = Record399Catalog.group.a, padding = 0, b = Record399Catalog.group.b},
    }
end
Record399Build()
assert(Record399Rows[1].a == 1 and Record399Rows[2].b == 4)
print(table.concat(trace, "|"))
