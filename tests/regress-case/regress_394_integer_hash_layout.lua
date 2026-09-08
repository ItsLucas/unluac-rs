-- Numeric records are not list fields: hash-only allocation is observable through #.
local function run(enabled)
    if enabled then
        local t = { [1] = nil, [2] = 55 }
        assert(#t == 0 and t[2] == 55)
        t[1] = 44
        assert(#t == 2)
        print("regress_394", #t, t[1], t[2])
    end
end
run(true)

local function mutate()
    local t = { [1] = nil, [2] = 55 }
    t[3] = 66
    t[1] = 44
    t[1] = nil
    assert(#t == 3, "later integer writes must not be reabsorbed into the hash initializer")
    print("regress_394_mutation", #t, t[2], t[3])
end
mutate()
