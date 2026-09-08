-- regress_341_short_circuit_shared_continuation:
-- A short-circuit endpoint may enter either arm; only their shared tail is a merge.
-- unluac: expect-not-contains [[goto]]
local function dispatch(flags, object)
    if (flags.a or flags.b) and object.enabled then
        if object.first() then
            print("first")
        elseif object.second() then
            object.value = 2
        end
        print("then-tail")
        if object.third() then
            object.value = 3
        end
    else
        if object.fourth() and object.fifth() then
            print("else")
        end
        print("else-tail")
    end
    if flags.after then
        print("after")
    end
end

local function predicate(name, value)
    return function()
        print(name)
        return value
    end
end

for mask = 0, 31 do
    local a = mask % 2 == 1
    local b = math.floor(mask / 2) % 2 == 1
    local enabled = math.floor(mask / 4) % 2 == 1
    local first = math.floor(mask / 8) % 2 == 1
    local later = math.floor(mask / 16) % 2 == 1
    local object = {
        enabled = enabled,
        first = predicate("first?", first),
        second = predicate("second?", later),
        third = predicate("third?", later),
        fourth = predicate("fourth?", first),
        fifth = predicate("fifth?", later),
        value = 0,
    }
    print("regress_341", mask)
    dispatch({a = a, b = b, after = later}, object)
    print("value", object.value)
end
