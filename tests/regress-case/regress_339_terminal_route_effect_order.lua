-- Shared terminal calls retain lookup order, short-circuiting and open return packs.
-- unluac: expect-not-contains [[goto]]
-- unluac: expect-not-contains [[unluac error]]
local function finish(value)
    io.write("return;")
    return value, nil, value + 10, nil
end

local function search(predicates, values)
    for index, value in ipairs(values) do
        if index ~= 2 then
            if predicates.first(value) or predicates.second(value) then
                return finish(value)
            end
            if predicates.third(value) or predicates.fourth(value) then
                return finish(-value)
            end
        end
    end
    return "missing"
end

local function run(first, second, third, fourth)
    local predicates = setmetatable({}, {
        __index = function(_, key)
            io.write("lookup-", key, ";")
            return function(value)
                io.write("call-", key, "-", value, ";")
                if key == "first" then
                    return first
                elseif key == "second" then
                    return second
                elseif key == "third" then
                    return third
                end
                return fourth
            end
        end
    })
    print("regress_339", search(predicates, {3, 4, 5}))
end

run(false, false)
run(nil, true)
run(true, false)
run(0, true)
run(false, "")
run(false, false, true, false)
run(false, false, nil, 0)
