-- An ancestor's captured home stays protected in the atomic-only branch traversal.
local function run(enabled)
    local held = { 91, 92, 93 }
    local function read()
        return held
    end
    if enabled then
        do
            held = { 11, 12, 13 }
            assert(read() == held and read()[2] == 12)
        end
    end
    print("regress_393", read()[1], read()[2], read()[3])
end
run(true)
run(false)
