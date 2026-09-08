-- Ordered lookup chains must stay interleaved with the original record writes.
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local reads = 0
local leaf = setmetatable({}, {
    __index = function(_, key)
        assert(Record398Rows == "old")
        reads = reads + 1
        trace[#trace + 1] = "field:" .. key .. ":" .. reads
        if key == "missing" then return nil end
        return reads
    end
})
Record398Catalog = setmetatable({}, {
    __index = function(_, key)
        trace[#trace + 1] = "group:" .. key
        return leaf
    end
})
function Record398Load(name)
    trace[#trace + 1] = "load:" .. name
    return "discarded", {}
end
function Record398Build()
    Record398Load("first")
    Record398Load("second")
    Record398Rows = {
        -- region398 rows begin
        {alpha = Record398Catalog.group.alpha, missing = Record398Catalog.group.missing, pad = 3},
        {alpha = Record398Catalog.group.alpha, missing = Record398Catalog.group.missing, pad = 3},
        -- region398 rows end
    }
end
Record398Rows = "old"
Record398Build()
assert(type(Record398Rows[1]) == "table" and Record398Rows[1].missing == nil)
print(#Record398Rows, reads, table.concat(trace, "|"))
