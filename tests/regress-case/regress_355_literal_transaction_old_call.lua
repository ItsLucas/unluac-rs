-- A scalar in the first row releases an old call result, not an internal producer.
local function build(weak, make)
    do
        local a, b, c, d = 41, 42, 43, 44
        local payload = make()
        weak[1] = payload
        collectgarbage("collect")
        assert(weak[1] ~= nil)
    end
    local rows = { 1, 2, 3, 4, 5, 6, 7, 8, 9, tag = true }
    collectgarbage("collect")
    collectgarbage("collect")
    assert(weak[1] == nil, "scalar write must release the old call-result root")
    print("regress_355", weak[1] == nil, #rows)
end
build(setmetatable({}, { __mode = "v" }), function() return {} end)
