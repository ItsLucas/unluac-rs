-- Original synthetic: once-only nested branches, numeric records and nested list rows.
local function run(p, q)
    if p then
        if q then
            if p <= 4 and q <= 20 then
                local matrix = {
                    [0] = { [0] = { { nil, 11, 12 } }, [1] = {
                        { 21, 22, 23, tag = { nil, 61 } },
                        { 24, 25, 26, tag = { nil, 62 } },
                        { 27, 28, 29, tag = { nil, 63 } },
                    } },
                    [1] = { [0] = {}, [1] = { { 31, 32, 33 }, { 34, 35, 36 } } },
                    [2] = { [0] = { { 41, 42, 43 } }, [1] = { { 51, 52, 53 } } },
                }
                assert(#matrix[0][0][1] == 3 and matrix[0][0][1][2] == 11)
                assert(matrix[0][1][1] ~= matrix[0][1][2])
                matrix[0][1][1][2] = 99
                assert(matrix[0][1][2][2] == 25 and matrix[1][1][1][2] == 32)
                matrix[0][1][2] = nil
                assert(#matrix[0][1] == 3, "nested list allocation must survive a middle hole")
                local weak = setmetatable({}, { __mode = "v" })
                weak[1] = matrix[0][1][3].tag
                matrix[0][1][3].tag = nil
                collectgarbage("collect")
                collectgarbage("collect")
                assert(weak[1] == nil, "nested literal producers must not become extra local roots")
                print("regress_390", #matrix[0][1], matrix[2][1][1][3])
            end
        end
    end
end
run(1, 1)
run(false, 1)
run(1, false)
run(5, 1)
