-- In-place arithmetic preserves operand order, metamethods and errors.
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local serial = 0
local fail_at = 0
local mt = {}
local function number(value)
    if type(value) == "table" then return value.n end
    return value
end
local function apply(op, a, b)
    serial = serial + 1
    a, b = number(a), number(b)
    trace[#trace + 1] = op .. ":" .. a .. ":" .. b
    assert(Record412Rows == "old")
    if serial == fail_at then error("arithmetic-stop", 0) end
    -- Keep returning objects so every arithmetic instruction is observable.
    return setmetatable({n = serial}, mt)
end
mt.__add = function(a,b) return apply("add",a,b) end
mt.__sub = function(a,b) return apply("sub",a,b) end
mt.__mul = function(a,b) return apply("mul",a,b) end
mt.__div = function(a,b) return apply("div",a,b) end
mt.__mod = function(a,b) return apply("mod",a,b) end
mt.__pow = function(a,b) return apply("pow",a,b) end
Record412Catalog = setmetatable({}, {__index = function(_, key)
    trace[#trace + 1] = "read:" .. key
    return setmetatable({n = 11}, mt)
end})
function Record412Build()
    Record412Rows = {
        {a = 2 * Record412Catalog.a * 3, b = (Record412Catalog.b + 4) - 5},
        {a = 6 / (Record412Catalog.c % 7), b = (Record412Catalog.d ^ 2) ^ 3},
        {a = 8 - (9 + Record412Catalog.e), b = 2 ^ (Record412Catalog.f / 3)},
    }
end
for stop = 0, 12 do
    serial = 0
    fail_at = stop
    Record412Rows = "old"
    local ok, err = pcall(Record412Build)
    if stop == 0 then
        assert(ok and #Record412Rows == 3)
    else
        assert(not ok and err == "arithmetic-stop" and Record412Rows == "old")
    end
    print(stop, ok, serial)
end
print(table.concat(trace, "|"))
