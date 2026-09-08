-- A scalar constructor field must still release the previous object in its VM home.
local function build(weak, make)
    do
        local p1, p2, p3, p4, p5 = 99, 98, 97, 96, 95
        local payload = make()
        weak[1] = payload
        collectgarbage("collect")
    end
    local row = { 1, 2, 3, 4, 5, 6, 7, 8, 9, tag = true }
    collectgarbage("collect")
    assert(weak[1] == nil, "constructor must overwrite the old call result")
    print("regress_345", weak[1] ~= nil, #row)
end
build(setmetatable({}, { __mode = "v" }), function() return {} end)
