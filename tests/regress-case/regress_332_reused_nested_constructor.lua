-- Nested records keep their lookup snapshots while the outer slot is reused.
local source = { left = 23, right = 29 }
local groups = {
    first = { { value = source.left, limit = 2 }, { value = source.right, limit = 4 } },
    second = { { value = source.right, limit = 6 }, { value = source.left, limit = 8 } },
}
local function inspect()
    print("regress_332", groups.first[1].value, groups.first[2].limit,
        groups.second[1].value, groups.second[2].limit)
end
inspect()
