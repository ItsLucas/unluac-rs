-- A terminal body shared by two ordered, effectful predicates inside a loop.
-- unluac: expect-not-contains [[goto]]
-- unluac: expect-not-contains [[unluac error]]
local function probe(name, value)
    io.write(name)
    return value
end

local function search(values)
    for index, value in ipairs(values) do
        if index ~= 2 then
            if probe("a", value == 1) or probe("b", value == 2) then
                return true
            end
            if probe("c", value == 3) or probe("d", value == 4) then
                return true
            end
            if probe("e", value == 5) then
                return true
            end
        end
    end
    return false
end

for value = 0, 5 do
    print("regress_336", value, search({value, 1, 0}))
end
