-- A captured constructor root must retain its upvalue identity and scratch stack.
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = "v"})
local serial = 0
local mt = {}
mt.__add = function(a, b)
    trace[#trace + 1] = "add"
    return a.id + b.id
end
mt.__mul = function(a, b)
    trace[#trace + 1] = "mul"
    a.id = a.id * b
    return a
end
Record413Catalog = setmetatable({}, {__index = function(_, key)
    collectgarbage("collect")
    trace[#trace + 1] = weak[serial] and "live" or "dead"
    serial = serial + 1
    local object = setmetatable({id = serial}, mt)
    weak[serial] = object
    return object
end})
function Record413Build()
    local rows = {
        -- region413 rows begin
        {a = Record413Catalog.a + Record413Catalog.b * 2, padding = 0},
        {a = Record413Catalog.a + Record413Catalog.b * 3, padding = 0},
        -- region413 rows end
    }
    function Record413Read() return rows end
    function Record413Replace(value) rows = value end
end
Record413Build()
local read1 = Record413Read
local replace1 = Record413Replace
local first = read1()
assert(#first == 2 and first[1].a == 5 and first[2].a == 15)
Record413Build()
local second = Record413Read()
assert(first ~= second and read1() == first)
replace1({"replacement"})
assert(read1()[1] == "replacement" and Record413Read() == second)
Record413Replace(nil)
assert(Record413Read() == nil and read1()[1] == "replacement")
print(first[1].a, first[2].a, second[1].a, second[2].a)
print(table.concat(trace, "|"))
