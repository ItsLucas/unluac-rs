-- Final high scratch slots must not become named locals rooted across later calls.
local rows = {
    { 1, 2, 3, tag = { nil, 5 } }, { 1, 2, 3, tag = { nil, 7 } },
    { 1, 2, 3, tag = { nil, 9 } }, { 1, 2, 3, tag = { nil, 11 } },
    { 1, 2, 3, tag = { nil, 13 } }, { 1, 2, 3, tag = { nil, 15 } },
    { 1, 2, 3, tag = { nil, 17 } }, { 1, 2, 3, tag = { nil, 19 } },
    { 1, 2, 3, tag = { nil, 21 } },
}
local weak = setmetatable({}, { __mode = "v" })
weak[1] = rows[1].tag
weak[2] = rows[9].tag
rows[1] = nil
rows[9] = nil
collectgarbage("collect")
collectgarbage("collect")
assert(weak[1] == nil, "overwritten internal scratch must not become an extra local root")
assert(weak[2] == nil, "a transient final scratch root must not become a surviving child local")
print("regress_356", weak[1] == nil, weak[2] == nil, #rows)
