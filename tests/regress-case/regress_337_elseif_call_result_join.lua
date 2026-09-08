-- An elseif chain keeps each call and its value-copy on the selected edge.
-- unluac: expect-not-contains [[goto]]
-- unluac: expect-not-contains [[unluac error]]
local function choose(value)
    io.write("choose", value, ";")
    return {value}
end

local function select_value(key)
    local value = {}
    if key == 1 then
        value = choose(11)
    elseif key == 2 or key == 3 then
        value = choose(22)
    elseif key == 4 then
        value = choose(33)
    elseif key == 5 then
        value = choose(44)
    else
        return "absent"
    end
    if value then
        return value[1]
    end
end

for key = 0, 6 do
    print("regress_337", key, select_value(key))
end
