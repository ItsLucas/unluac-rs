-- Captured table-library lookups keep their exact SSA identities and stack slots.
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local weak = setmetatable({}, {__mode = "v"})
local function event(name)
    collectgarbage("collect")
    trace[#trace + 1] = name .. ":" .. (weak.library and "live" or "dead")
end
function Capture426Load(value) event("load:" .. value) end
Capture426Value = 426
function Capture426Build(marker)
    Capture426Load("prefix")
    local sort = Capture426Library.sort
    local insert = Capture426Library.insert
    local rows = {
        {nBuffID = 711, nSkillID = 39906, nPriority = 1},
        {nBuffID = 714, nSkillID = 39907, nPriority = 2},
        {nBuffID = 666, nSkillID = Capture426Value, nPriority = 3},
    }
    function Capture426Unrelated(value) return value + 1 end
    function Capture426Apply() return rows, insert, sort, marker end
end
setfenv(Capture426Build, setmetatable({}, {
    __index = function(_, key)
        event("env:" .. key)
        if key == "Capture426Library" then
            local library = setmetatable({}, {
                __index = function(_, name) event("field:" .. name); return table[name] end,
            })
            weak.library = library
            return library
        end
        return _G[key]
    end,
    __newindex = function(_, key, value) event("store:" .. key); _G[key] = value end,
}))
Capture426Build(426)
local rows, insert, sort, marker = Capture426Apply()
assert(rows[3].nSkillID == 426 and rows[1].nBuffID == 711)
assert(insert == table.insert and sort == table.sort and marker == 426)
assert(Capture426Unrelated(2) == 3)
print(table.concat(trace, "|"))
