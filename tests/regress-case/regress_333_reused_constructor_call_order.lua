-- Calls and subsequent slot overwrites must stay before the final fixed batch.
local events = {}
local function mark(value)
    events[#events + 1] = value
    return value
end
local groups = {
    first = { { name = mark("a"), enabled = true }, { name = mark("b"), enabled = false } },
    second = { { name = mark("c"), enabled = false }, { name = mark("d"), enabled = true } },
}
print("regress_333", table.concat(events, ","), groups.first[1].name,
    groups.first[2].enabled, groups.second[1].name, groups.second[2].enabled)
