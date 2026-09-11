-- 字段调用和算术保持低槽的实际读取时点，回调可更新同一捕获参数。
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = "v"})
local tick = 0
local mt = {}
local function number(value)
    if type(value) == "table" then return value.n end
    return value
end
local function observe(kind, a, b)
    collectgarbage("collect")
    tick = tick + 1
    trace[#trace + 1] = kind .. ":" .. tostring(number(a)) .. ":" .. tostring(number(b)) .. ":" .. (weak.result and "live" or "dead")
    if tick == Fields433Fail then error("field-expression-stop", 0) end
end
function Fields433Box(value)
    return setmetatable({n = value}, mt)
end
function Fields433Replacement()
    return Fields433Box(100 + tick)
end
local function apply(kind, a, b)
    observe(kind, a, b)
    local value = Fields433Box(tick + 10)
    weak.result = value
    return value
end
mt.__add = function(a, b) return apply("add", a, b) end
mt.__sub = function(a, b) return apply("sub", a, b) end
mt.__mul = function(a, b) return apply("mul", a, b) end
mt.__div = function(a, b) return apply("div", a, b) end
mt.__mod = function(a, b) return apply("mod", a, b) end
mt.__pow = function(a, b) return apply("pow", a, b) end
function Fields433Call(value)
    observe("call", value, 0)
    Fields433Mutate()
    local result = Fields433Box(tick + 20)
    weak.result = result
    return result, "discarded"
end
Fields433Catalog = setmetatable({}, {__index = function(_, key)
    observe("lookup:" .. key, 0, 0)
    Fields433Mutate()
    return Fields433Call
end})
function Fields433Load(value) return {}, {} end
function Fields433Build(x, y, z, angle, delta, scale, callee)
    function Fields433Mutate()
        x = Fields433Replacement()
        z = x
        angle = x
    end
    Fields433Load("prefix")
    Fields433Root = {
        {x = x + scale * Fields433Catalog.cos(angle + delta), y = y + scale * Fields433Catalog.sin(angle + delta), z = z},
        {x = x + scale * Fields433Catalog.cos(delta - angle), y = y - scale * Fields433Catalog.sin(delta - angle), z = z},
        {x = x - scale * Fields433Catalog.cos(angle), y = y - scale * Fields433Catalog.sin(angle), z = z},
        {a = Fields433Catalog.cos(angle * delta), b = Fields433Catalog.sin(angle / delta)},
        {a = Fields433Catalog.cos(angle % delta), b = Fields433Catalog.sin(angle ^ delta)},
        {value = callee(angle), z = z},
    }
end
for fail = 0, 41 do
    Fields433Fail = fail
    Fields433Root = "old"
    trace = {}
    weak = setmetatable({}, {__mode = "v"})
    tick = 0
    local ok, err = pcall(Fields433Build, Fields433Box(2), Fields433Box(3), Fields433Box(4), Fields433Box(5), Fields433Box(6), Fields433Box(7), Fields433Call)
    if fail == 0 then
        assert(ok and #Fields433Root == 6 and Fields433Root[1].z.n > 100)
    else
        assert(not ok and err == "field-expression-stop" and Fields433Root == "old")
    end
    print(fail, tick, table.concat(trace, "|"))
end
