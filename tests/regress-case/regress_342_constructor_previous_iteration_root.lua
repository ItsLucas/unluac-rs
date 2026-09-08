-- Reserving nil fields must not clear the previous iteration's lookup-result slot.
local function build(rows, input)
    for index = 1, 2 do
        rows[#rows + 1] = { input[index].value, index }
    end
end

local rows = {}
local weak = setmetatable({}, { __mode = "v" })
local input = setmetatable({}, {
    __index = function(_, index)
        if index == 1 then
            local payload = {}
            weak[1] = payload
            return { value = payload }
        end
        rows[1][1] = nil
        collectgarbage("collect")
        print("regress_342", weak[1] ~= nil)
        assert(weak[1] ~= nil)
        return { value = 7 }
    end,
})
build(rows, input)
