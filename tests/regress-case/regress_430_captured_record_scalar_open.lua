-- A captured record ends before the next captured scalar declaration.
-- The later open array and every following statement retain their exact slots.
-- unluac: expect-not-contains [[unluac error]]
local weak = setmetatable({}, {__mode = "v"})
local trace = {}
local function event(name)
    collectgarbage("collect")
    trace[#trace + 1] = name
end
function Lion430Load(name) event("load:" .. name) end
function Lion430Text(index, label)
    event("text:" .. label)
    if index == 8 then
        local values = {}
        for i = 1, Lion430TailCount do
            if i % 4 ~= 0 then
                local value = {index = i}
                values[i] = value
                weak[i] = value
            end
        end
        return unpack(values, 1, Lion430TailCount)
    end
    local value = {index = index}
    weak[-index - 1] = value
    return value, "discarded"
end
function Lion430Build()
    Lion430Load("one")
    Lion430Load("two")
    Lion430Load("three")
    Lion430Load("four")
    Lion430Seed = {
        {a = 0, b = 0}, {a = 0, b = 0}, {a = 0, b = 0}, {a = 0, b = 0}, {a = 0, b = 0},
        {a = 0, b = 0}, {a = 0, b = 0}, {a = 0, b = 0}, {a = 0, b = 0}, {a = 0, b = 0},
    }
    function Lion430One() end
    function Lion430Two() end
    function Lion430Three() end
    function Lion430Four() end
    local record = {
        a = 1, b = 1, text = Lion430Text(0, "record"), items = {{1, 2, 3}},
    }
    local count = 1
    local labels = {
        Lion430Text(1, "one"), Lion430Text(2, "two"), Lion430Text(3, "three"),
        Lion430Text(4, "four"), Lion430Text(5, "five"), Lion430Text(6, "six"),
        Lion430Text(7, "seven"), Lion430Text(8, "tail"),
    }
    local ids = {451, 452, 453, 454, 455, 456, 457, 458}
    function Lion430Apply() return ids, count, labels, record end
    function Lion430UnApply() end
    function Lion430Remove() end
    function Lion430Warning() end
end
setfenv(Lion430Build, setmetatable({}, {
    __index = function(_, key) event("env:" .. key); return _G[key] end,
    __newindex = function(_, key, value)
        event("store:" .. key)
        _G[key] = value
        if key == "Lion430Apply" then
            local ids, count, labels, record = value()
            assert(#ids == 8 and ids[8] == 458 and count == 1)
            assert(record.a == 1 and record.text.index == 0 and #record.items[1] == 3)
            print("length", Lion430TailCount, #labels, labels[8] == nil)
            for index in pairs(labels) do labels[index] = nil end
            record.text = nil
            collectgarbage("collect")
            local alive = 0
            for index = -8, Lion430TailCount do
                if weak[index] then alive = alive + 1 end
            end
            print("retained", alive)
        end
    end,
}))
for _, width in ipairs({0, 1, 3, 60, 120}) do
    weak = setmetatable({}, {__mode = "v"})
    trace = {}
    Lion430TailCount = width
    Lion430Build()
    print(table.concat(trace, "|"))
end
