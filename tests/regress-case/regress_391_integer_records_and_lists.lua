local function run(enabled)
    if enabled then
        do
            local t = {
                [0] = { nil, 31 },
                { nil, 11 }, { nil, 12 },
                [7] = { nil, 77 },
            }
            assert(#t == 2 and #t[0] == 2 and #t[7] == 2)
            assert(t[1] ~= t[2] and t[0] ~= t[7])
            local weak = setmetatable({}, { __mode = "v" })
            weak[1] = t[0]
            t[0] = nil
            collectgarbage("collect")
            collectgarbage("collect")
            assert(weak[1] == nil)
            t[7][1] = 70
            print("regress_391", #t, #t[7], t[7][2], t[2][2])
        end
    end
end
run(true)
run(false)
