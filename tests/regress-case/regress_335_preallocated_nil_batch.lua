-- A nil-valued lookup must retain the VM's reserved array slots and batch snapshots.
local events = {}
local source = setmetatable({}, {
    __index = function(_, index)
        events[#events + 1] = index
        collectgarbage("collect")
        return { value = index == 1 and "present" or nil }
    end,
})
local function build(input)
    local rows = {}
    for index = 1, 2 do
        rows[#rows + 1] = { input[index].value, index }
    end
    return rows
end
local rows = build(source)
assert(#rows[1] == 2 and #rows[2] == 2)
assert(rows[1][1] == "present" and rows[2][1] == nil)
print("regress_335", #rows[1], #rows[2], rows[1][2], rows[2][2],
    table.concat(events, ","))
