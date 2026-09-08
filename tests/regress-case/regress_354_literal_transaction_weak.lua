-- Consumed internal tables must not leave extra named roots in the recovered source.
local rows = {
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 7 } },
    { 1, 2, 3, tag = { nil, 9 } }, { 1, 2, 3, tag = { nil, 11 } },
    { 1, 2, 3, tag = { nil, 13 } }, { 1, 2, 3, tag = { nil, 15 } },
    { 1, 2, 3, tag = { nil, 17 } }, { 1, 2, 3, tag = { nil, 19 } },
    { 1, 2, 3, tag = { nil, 21 } },
}
local weak = setmetatable({}, { __mode = "v" })
weak[1] = rows[1].tag
weak[2] = rows[3].tag
rows[1] = nil
rows[3] = nil
collectgarbage("collect")
collectgarbage("collect")
assert(weak[1] == nil and weak[2] == nil, "removed producer locals must not prolong child lifetime")
print("regress_354", weak[1] == nil, weak[2] == nil, #rows)
