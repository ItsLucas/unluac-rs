-- 嵌套 record 与数组必须保留构造顺序、数组空洞和临时对象的存活期。
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = "v"})
local calls = 0
local reads = 0
local leaf_mt = {
    __index = function(self, key)
        trace[#trace + 1] = "field:" .. self.id .. ":" .. key
        return self.id
    end,
}
Nested418Catalog = setmetatable({}, {
    __index = function(_, key)
        collectgarbage("collect")
        trace[#trace + 1] = "scratch:" .. (weak.scratch and "live" or "dead")
        reads = reads + 1
        local value = setmetatable({id = reads}, leaf_mt)
        weak.scratch = value
        return value
    end,
})
function Nested418Load(label)
    trace[#trace + 1] = "load:" .. label
    return {}, {}
end
function Nested418Call(index, label)
    collectgarbage("collect")
    calls = calls + 1
    trace[#trace + 1] = "call:" .. label .. ":" .. (weak[calls - 1] and "live" or "dead")
    if Nested418Fail == calls then error("nested-record-stop", 0) end
    local value = {index = index, label = label}
    weak[calls] = value
    return value, "discarded", nil
end
function Nested418Nil()
    return nil, 77
end
function Nested418Build()
    Nested418Load("prefix")
    Nested418Seed = {empty = {}, data = {0, 1}}
    Nested418Root = {
        empty = {},
        rows = {
            {info = {name = Nested418Call(1, "first"), code = 11}, numbers = {Nested418Nil(), 2, 3}, empty = {}},
            {info = {name = Nested418Call(2, "second"), code = 12}, numbers = {4, Nested418Nil(), 6}, empty = {}},
        },
        record = {
            first = Nested418Catalog.group.a,
            child = {second = Nested418Catalog.group.b, pad = 3},
        },
    }
    Nested418After = {field = {items = {1, 2}, empty = {}}, value = Nested418Call(3, "third")}
end
Nested418Root = "old"
Nested418After = "old"
Nested418Build()
assert(next(Nested418Seed.empty) == nil and Nested418Seed.data[2] == 1)
assert(next(Nested418Root.empty) == nil and #Nested418Root.rows == 2)
assert(Nested418Root.rows[1].info.name.label == "first")
assert(Nested418Root.rows[2].info.code == 12 and Nested418Root.rows[2].numbers[2] == nil)
assert(Nested418Root.record.first == 1 and Nested418Root.record.child.second == 2)
assert(Nested418After.value.index == 3 and Nested418After.field.items[2] == 2)
print(#Nested418Root.rows[1].numbers, #Nested418Root.rows[2].numbers, table.concat(trace, "|"))
for fail = 1, 3 do
    calls = 0
    reads = 0
    trace = {}
    weak = setmetatable({}, {__mode = "v"})
    Nested418Fail = fail
    Nested418Root = "old"
    Nested418After = "old"
    local ok, err = pcall(Nested418Build)
    assert(not ok and err == "nested-record-stop" and Nested418After == "old")
    if fail <= 2 then assert(Nested418Root == "old")
    else assert(type(Nested418Root) == "table") end
    print(fail, calls, reads, type(Nested418Root), table.concat(trace, "|"))
end
