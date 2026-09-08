-- Static first definitions are not entry-nil overwrites on the next loop iteration.
local weak = setmetatable({}, { __mode = "v" })
local function make()
    return {}
end
for iteration = 1, 2 do
    do
        local a, b, c, d = 61, 62, 63, 64
        local payload = make()
        weak[1] = payload
        collectgarbage("collect")
    end
    local rows = { 1, 2, 3, 4, 5, 6, 7, 8, 9, tag = true }
    collectgarbage("collect")
    assert(weak[1] == nil, "loop scratch overwrite must not use entry-nil evidence")
    print("regress_357", iteration, weak[1] == nil, #rows)
end
