-- These records collide with implicit list keys and are not an atomic candidate.
local function run(enabled)
    if enabled then
        local t = { [1] = 91, 11, 22, [2] = 92 }
        assert(t[1] == 11 and t[2] == 22)
        local u = { [0] = 1, [0] = 2, [1] = 3 }
        assert(u[0] == 2 and u[1] == 3)
        print("regress_392", t[1], t[2], u[0], u[1])
    end
end
run(true)
