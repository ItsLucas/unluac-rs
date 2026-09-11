-- 固定调用键必须单次求值并在子表构造期间存活；闭包捕获既有参数和外层upvalue。
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = "v"})
local version = 0
function Dynamic423Load(value) return {}, {} end
function Dynamic423Key(index, label)
    collectgarbage("collect")
    trace[#trace + 1] = "key:" .. index .. ":" .. (weak["v" .. (index - 1)] and "live" or "dead")
    if Dynamic423FailKey == index then error("dynamic-key-stop", 0) end
    if Dynamic423NilKey == index then return nil, "discarded" end
    local key = {index = index, label = label}
    weak["k" .. index] = key
    return key, "discarded", nil
end
function Dynamic423Value(index, label)
    collectgarbage("collect")
    trace[#trace + 1] = "value:" .. index .. ":" .. (weak["k" .. index] and "live" or "dead")
    if Dynamic423FailValue == index then error("dynamic-value-stop", 0) end
    local value = {index = index, label = label}
    weak["v" .. index] = value
    return value, "discarded"
end
function Dynamic423Build(parameter)
    Dynamic423Load("prefix")
    Dynamic423Root = {
        [10] = {
            update = function(value)
                parameter = parameter + value
                version = version + 1
                return parameter, version
            end,
            read = function() return parameter, version end,
            items = {{1, 2}, {3, 4}},
        },
        [Dynamic423Key(1, "first")] = {
            items = {{value = Dynamic423Value(1, "one"), read = function() return parameter, version end}},
        },
        [Dynamic423Key(2, "second")] = {
            items = {{value = Dynamic423Value(2, "two"), read = function() return parameter, version end}},
        },
        [Dynamic423Key(3, "third")] = {
            items = {{value = Dynamic423Value(3, "three"), read = function() return parameter, version end}},
        },
    }
end
Dynamic423Root = "old"
Dynamic423Build(10)
assert(Dynamic423Root[weak.k1].items[1].value.index == 1)
assert(Dynamic423Root[weak.k2].items[1].value.index == 2)
assert(Dynamic423Root[weak.k3].items[1].value.index == 3)
assert(Dynamic423Root[10].items[2][1] == 3)
local first_read = Dynamic423Root[10].read
local value, current = Dynamic423Root[10].update(7)
assert(value == 17 and current == 1)
value, current = Dynamic423Root[weak.k3].items[1].read()
assert(value == 17 and current == 1)
print(value, current, table.concat(trace, "|"))
for fail = 1, 3 do
    for kind = 1, 3 do
        weak = setmetatable({}, {__mode = "v"})
        trace = {}
        Dynamic423Root = "old"
        Dynamic423FailKey = kind == 1 and fail or nil
        Dynamic423FailValue = kind == 2 and fail or nil
        Dynamic423NilKey = kind == 3 and fail or nil
        local ok, err = pcall(Dynamic423Build, 20)
        assert(not ok and Dynamic423Root == "old")
        if kind == 3 then assert(tostring(err):find("table index is nil", 1, true))
        else assert(err == (kind == 1 and "dynamic-key-stop" or "dynamic-value-stop")) end
        print(fail, kind, table.concat(trace, "|"))
    end
end
Dynamic423FailKey = nil
Dynamic423FailValue = nil
Dynamic423NilKey = nil
Dynamic423Build(30)
assert(Dynamic423Root[10].read ~= first_read)
assert(first_read() == 17 and Dynamic423Root[10].read() == 30)
