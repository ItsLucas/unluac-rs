-- Closed global initializers preserve the stack for the next constructor.
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = "v"})
local serial = 0
function Module417Load(value)
    trace[#trace + 1] = "load:" .. value
    return {}, {}
end
function Module417Text(index, label)
    collectgarbage("collect")
    serial = serial + 1
    trace[#trace + 1] = label .. ":" .. (weak[serial - 1] and "live" or "dead")
    if Module417Fail == serial then error("field-call-stop", 0) end
    local result = {index = index, label = label}
    weak[serial] = result
    return result, "discarded", nil
end
function Module417Build()
    Module417Load("prefix")
    Module417Seed = {{a = 0, b = 1}, {a = 2, b = 3}}
    Module417Rows = {
        -- region417 rows begin
        {label = Module417Text(1, "first"), value = 11},
        {label = Module417Text(2, "second"), value = 12},
        -- region417 rows end
    }
    Module417Groups = {{2, 3, 4}, {5, 6}}
    Module417Nested = {
        {{label = Module417Text(3, "third"), value = 13}},
        {{label = Module417Text(4, "fourth"), value = 14}},
    }
end
Module417Rows = "old"
Module417Nested = "old"
Module417Build()
assert(#Module417Seed == 2 and Module417Seed[2].b == 3)
assert(Module417Rows[1].label.label == "first" and Module417Rows[2].value == 12)
assert(Module417Groups[1][3] == 4 and Module417Nested[2][1].label.index == 4)
print(table.concat(trace, "|"))
for fail = 1, 4 do
    serial = 0
    Module417Fail = fail
    Module417Rows = "old"
    Module417Nested = "old"
    local ok, err = pcall(Module417Build)
    assert(not ok and err == "field-call-stop" and Module417Nested == "old")
    if fail <= 2 then assert(Module417Rows == "old")
    else assert(type(Module417Rows) == "table") end
    print(fail, serial, type(Module417Rows), Module417Nested)
end
